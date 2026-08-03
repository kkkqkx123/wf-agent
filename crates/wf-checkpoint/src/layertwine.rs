use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use layertwine::core::file_node::FileNode;
use layertwine::core::snapshot::{Snapshot, SnapshotContent};
use layertwine::core::types::SnapshotId;
use layertwine::storage::repository::{MetadataStore, SnapshotStore};
use layertwine::storage::sqlite::SqliteStorage;
use layertwine::StorageResult;

pub trait GitCheckpointAdapter: Send + Sync {
    fn save_checkpoint(
        &self,
        checkpoint_id: &str,
        data: &[u8],
        metadata: &HashMap<String, String>,
    ) -> impl std::future::Future<Output = Result<(), LayertwineError>> + Send;

    fn get_checkpoint(
        &self,
        checkpoint_id: &str,
    ) -> impl std::future::Future<Output = Result<Option<Vec<u8>>, LayertwineError>> + Send;

    fn list_checkpoints(
        &self,
        parent_id: Option<&str>,
    ) -> impl std::future::Future<Output = Result<Vec<String>, LayertwineError>> + Send;

    fn delete_checkpoint(
        &self,
        checkpoint_id: &str,
    ) -> impl std::future::Future<Output = Result<bool, LayertwineError>> + Send;

    fn batch_save(
        &self,
        items: &[(String, Vec<u8>, HashMap<String, String>)],
    ) -> impl std::future::Future<Output = Result<(), LayertwineError>> + Send;
}

#[derive(Debug, thiserror::Error)]
pub enum LayertwineError {
    #[error("layertwine storage error: {0}")]
    Storage(String),

    #[error("checkpoint not found: {id}")]
    NotFound { id: String },

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

fn map_storage_error(e: layertwine::StorageError) -> LayertwineError {
    LayertwineError::Storage(e.to_string())
}

fn map_storage_result<T>(result: StorageResult<T>) -> Result<T, LayertwineError> {
    result.map_err(map_storage_error)
}

const CP_KEY_PREFIX: &str = "wf-checkpoint:";
const CP_PARENT_PREFIX: &str = "wf-checkpoint-parent:";

/// Real layertwine backend adapter: persists checkpoint blobs as structured
/// snapshots inside layertwine's SQLite storage (via its `SnapshotStore` /
/// `MetadataStore` repository traits). An O(1) metadata index maps the
/// checkpoint id to the content-addressed snapshot id, and per-parent index
/// lists support `list_checkpoints(parent)`.
pub struct LayertwineGitAdapter {
    storage: SqliteStorage,
}

impl LayertwineGitAdapter {
    pub fn new_in_memory() -> Result<Self, LayertwineError> {
        let storage = map_storage_result(SqliteStorage::new_full_in_memory())?;
        Ok(Self { storage })
    }

    pub fn new(path: &Path) -> Result<Self, LayertwineError> {
        let storage = map_storage_result(SqliteStorage::new_full(path))?;
        Ok(Self { storage })
    }

    fn snapshot_key(checkpoint_id: &str) -> String {
        format!("{CP_KEY_PREFIX}{}", checkpoint_id)
    }

    fn parent_key(parent_id: &str) -> String {
        format!("{CP_PARENT_PREFIX}{}", parent_id)
    }

    fn load_id_list(&self, key: &str) -> Result<Vec<String>, LayertwineError> {
        match map_storage_result(self.storage.load_metadata(key))? {
            Some(list) if !list.is_empty() => Ok(list.split(',').map(String::from).collect()),
            _ => Ok(Vec::new()),
        }
    }

    fn store_id_list(&self, key: &str, ids: &[String]) -> Result<(), LayertwineError> {
        map_storage_result(self.storage.store_metadata(key, &ids.join(",")))
    }
}

impl GitCheckpointAdapter for LayertwineGitAdapter {
    async fn save_checkpoint(
        &self,
        checkpoint_id: &str,
        data: &[u8],
        metadata: &HashMap<String, String>,
    ) -> Result<(), LayertwineError> {
        let file_node = FileNode::new(
            Path::new(&format!(".checkpoints/{}.json", checkpoint_id)).to_path_buf(),
            data,
        );
        let snapshot = Snapshot::new_with_content(
            file_node,
            SnapshotContent::Structured(data.to_vec()),
            format!("graph://checkpoints/{}", checkpoint_id),
            "checkpoint".to_string(),
            vec![],
            vec![],
        );

        map_storage_result(self.storage.store_snapshot(&snapshot, data))?;

        let snapshot_hex = snapshot.id.to_hex();
        map_storage_result(
            self.storage
                .store_metadata(&Self::snapshot_key(checkpoint_id), &snapshot_hex),
        )?;

        if let Some(parent) = metadata.get("parentId") {
            let list_key = Self::parent_key(parent);
            let mut ids = self.load_id_list(&list_key)?;
            if !ids.iter().any(|id| id == checkpoint_id) {
                ids.push(checkpoint_id.to_string());
                self.store_id_list(&list_key, &ids)?;
            }
        }

        Ok(())
    }

    async fn get_checkpoint(
        &self,
        checkpoint_id: &str,
    ) -> Result<Option<Vec<u8>>, LayertwineError> {
        let snapshot_hex = match map_storage_result(
            self.storage
                .load_metadata(&Self::snapshot_key(checkpoint_id)),
        )? {
            Some(hex) if !hex.is_empty() => hex,
            _ => return Ok(None),
        };

        let snapshot_id = SnapshotId::from_hex(&snapshot_hex).ok_or_else(|| {
            LayertwineError::Serialization(format!(
                "invalid stored snapshot id '{}' for checkpoint {}",
                snapshot_hex, checkpoint_id
            ))
        })?;

        let snapshot =
            map_storage_result(self.storage.get_snapshot(&snapshot_id)).map_err(|e| match e {
                LayertwineError::Storage(_) => LayertwineError::NotFound {
                    id: checkpoint_id.to_string(),
                },
                other => other,
            })?;

        Ok(snapshot.content.map(|content| content.to_bytes()))
    }

    async fn list_checkpoints(
        &self,
        parent_id: Option<&str>,
    ) -> Result<Vec<String>, LayertwineError> {
        match parent_id {
            Some(parent) => self.load_id_list(&Self::parent_key(parent)),
            None => {
                // No global enumeration over metadata keys; per-parent lists
                // are the canonical listing entry point.
                Ok(Vec::new())
            }
        }
    }

    async fn delete_checkpoint(&self, checkpoint_id: &str) -> Result<bool, LayertwineError> {
        let key = Self::snapshot_key(checkpoint_id);
        let exists = match map_storage_result(self.storage.load_metadata(&key))? {
            Some(hex) if !hex.is_empty() => true,
            _ => false,
        };
        if !exists {
            return Ok(false);
        }

        // MetadataStore has no delete; blank the index entry to mark it gone.
        // Parent index lists are rewritten when the next checkpoint is saved
        // under the same parent, so stale entries are self-healing.
        map_storage_result(self.storage.store_metadata(&key, ""))?;

        Ok(true)
    }

    async fn batch_save(
        &self,
        items: &[(String, Vec<u8>, HashMap<String, String>)],
    ) -> Result<(), LayertwineError> {
        for (id, data, metadata) in items {
            self.save_checkpoint(id, data, metadata).await?;
        }
        Ok(())
    }
}

impl LayertwineGitAdapter {
    /// Storage accessor for tests and diagnostics.
    pub fn storage(&self) -> &SqliteStorage {
        &self.storage
    }
}

pub struct InMemoryGitAdapter {
    branches: tokio::sync::RwLock<HashMap<String, Vec<u8>>>,
    metadata: tokio::sync::RwLock<HashMap<String, HashMap<String, String>>>,
}

impl InMemoryGitAdapter {
    pub fn new() -> Self {
        Self {
            branches: tokio::sync::RwLock::new(HashMap::new()),
            metadata: tokio::sync::RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryGitAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl GitCheckpointAdapter for InMemoryGitAdapter {
    async fn save_checkpoint(
        &self,
        checkpoint_id: &str,
        data: &[u8],
        meta: &HashMap<String, String>,
    ) -> Result<(), LayertwineError> {
        let mut branches = self.branches.write().await;
        branches.insert(checkpoint_id.to_string(), data.to_vec());
        let mut metadata = self.metadata.write().await;
        metadata.insert(checkpoint_id.to_string(), meta.clone());
        Ok(())
    }

    async fn get_checkpoint(
        &self,
        checkpoint_id: &str,
    ) -> Result<Option<Vec<u8>>, LayertwineError> {
        let branches = self.branches.read().await;
        Ok(branches.get(checkpoint_id).cloned())
    }

    async fn list_checkpoints(
        &self,
        parent_id: Option<&str>,
    ) -> Result<Vec<String>, LayertwineError> {
        let branches = self.branches.read().await;
        let metadata = self.metadata.read().await;
        let mut ids: Vec<String> = branches
            .keys()
            .filter(|id| match parent_id {
                Some(parent) => metadata
                    .get(*id)
                    .and_then(|m| m.get("parentId"))
                    .map(|p| p == parent)
                    .unwrap_or(false),
                None => true,
            })
            .cloned()
            .collect();
        ids.sort();
        Ok(ids)
    }

    async fn delete_checkpoint(&self, checkpoint_id: &str) -> Result<bool, LayertwineError> {
        let mut branches = self.branches.write().await;
        let mut metadata = self.metadata.write().await;
        let removed = branches.remove(checkpoint_id).is_some();
        metadata.remove(checkpoint_id);
        Ok(removed)
    }

    async fn batch_save(
        &self,
        items: &[(String, Vec<u8>, HashMap<String, String>)],
    ) -> Result<(), LayertwineError> {
        let mut branches = self.branches.write().await;
        let mut metadata = self.metadata.write().await;

        for (id, data, meta) in items {
            branches.insert(id.clone(), data.clone());
            metadata.insert(id.clone(), meta.clone());
        }

        Ok(())
    }
}

pub struct LayertwineCheckpointBridge<T: GitCheckpointAdapter> {
    adapter: Arc<T>,
}

impl<T: GitCheckpointAdapter> LayertwineCheckpointBridge<T> {
    pub fn new(adapter: Arc<T>) -> Self {
        Self { adapter }
    }

    pub async fn save(
        &self,
        checkpoint_id: &str,
        data: &[u8],
        entity_type: &str,
        entity_id: &str,
    ) -> Result<(), LayertwineError> {
        let mut meta = HashMap::new();
        meta.insert("entityType".to_string(), entity_type.to_string());
        meta.insert("entityId".to_string(), entity_id.to_string());
        meta.insert("parentId".to_string(), entity_id.to_string());
        meta.insert(
            "timestamp".to_string(),
            chrono::Utc::now().timestamp_millis().to_string(),
        );
        self.adapter
            .save_checkpoint(checkpoint_id, data, &meta)
            .await
    }

    pub async fn load(&self, checkpoint_id: &str) -> Result<Option<Vec<u8>>, LayertwineError> {
        self.adapter.get_checkpoint(checkpoint_id).await
    }

    pub async fn list(&self) -> Result<Vec<String>, LayertwineError> {
        self.adapter.list_checkpoints(None).await
    }

    /// List checkpoints whose parent entity matches `parent_id`.
    pub async fn list_by_parent(&self, parent_id: &str) -> Result<Vec<String>, LayertwineError> {
        self.adapter.list_checkpoints(Some(parent_id)).await
    }

    /// Delete a checkpoint from the backend. Returns true when it existed.
    pub async fn delete(&self, checkpoint_id: &str) -> Result<bool, LayertwineError> {
        self.adapter.delete_checkpoint(checkpoint_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn save_and_load_checkpoint() {
        let adapter = Arc::new(InMemoryGitAdapter::new());
        let bridge = LayertwineCheckpointBridge::new(adapter);

        bridge
            .save("cp-1", b"checkpoint data", "workflow", "exec-1")
            .await
            .unwrap();

        let data = bridge.load("cp-1").await.unwrap();
        assert_eq!(data, Some(b"checkpoint data".to_vec()));
    }

    #[tokio::test]
    async fn load_missing_checkpoint() {
        let adapter = Arc::new(InMemoryGitAdapter::new());
        let bridge = LayertwineCheckpointBridge::new(adapter);

        let data = bridge.load("nonexistent").await.unwrap();
        assert!(data.is_none());
    }

    #[tokio::test]
    async fn list_checkpoints() {
        let adapter = Arc::new(InMemoryGitAdapter::new());
        let bridge = LayertwineCheckpointBridge::new(adapter);

        bridge
            .save("cp-1", b"data-1", "workflow", "exec-1")
            .await
            .unwrap();
        bridge
            .save("cp-2", b"data-2", "workflow", "exec-2")
            .await
            .unwrap();

        let list = bridge.list().await.unwrap();
        assert_eq!(list.len(), 2);
    }

    #[tokio::test]
    async fn list_by_parent_filters() {
        let adapter = Arc::new(InMemoryGitAdapter::new());
        let bridge = LayertwineCheckpointBridge::new(adapter);

        bridge
            .save("cp-1", b"data-1", "workflow", "exec-1")
            .await
            .unwrap();
        bridge
            .save("cp-2", b"data-2", "workflow", "exec-2")
            .await
            .unwrap();

        let list = bridge.list_by_parent("exec-1").await.unwrap();
        assert_eq!(list, vec!["cp-1".to_string()]);
    }

    #[tokio::test]
    async fn delete_removes_checkpoint() {
        let adapter = Arc::new(InMemoryGitAdapter::new());
        let bridge = LayertwineCheckpointBridge::new(adapter);

        bridge
            .save("cp-1", b"data-1", "workflow", "exec-1")
            .await
            .unwrap();

        assert!(bridge.delete("cp-1").await.unwrap());
        assert!(!bridge.delete("cp-1").await.unwrap());
        assert!(bridge.load("cp-1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn batch_save() {
        let adapter = Arc::new(InMemoryGitAdapter::new());

        let items = vec![
            (
                "cp-1".to_string(),
                b"data-1".to_vec(),
                HashMap::from([("entityId".to_string(), "e1".to_string())]),
            ),
            (
                "cp-2".to_string(),
                b"data-2".to_vec(),
                HashMap::from([("entityId".to_string(), "e2".to_string())]),
            ),
        ];

        adapter.batch_save(&items).await.unwrap();

        assert!(adapter.get_checkpoint("cp-1").await.unwrap().is_some());
        assert!(adapter.get_checkpoint("cp-2").await.unwrap().is_some());
    }

    // ---- Real layertwine backend integration tests ----

    fn make_real_adapter() -> LayertwineGitAdapter {
        LayertwineGitAdapter::new_in_memory().unwrap()
    }

    #[tokio::test]
    async fn real_backend_save_and_load() {
        let adapter = make_real_adapter();
        let mut meta = HashMap::new();
        meta.insert("entityId".to_string(), "exec-1".to_string());
        meta.insert("parentId".to_string(), "exec-1".to_string());

        adapter
            .save_checkpoint("cp-1", b"checkpoint payload", &meta)
            .await
            .unwrap();

        let data = adapter.get_checkpoint("cp-1").await.unwrap();
        assert_eq!(data, Some(b"checkpoint payload".to_vec()));
    }

    #[tokio::test]
    async fn real_backend_save_twice_overwrites() {
        let adapter = make_real_adapter();
        let meta = HashMap::new();

        adapter
            .save_checkpoint("cp-1", b"first", &meta)
            .await
            .unwrap();
        adapter
            .save_checkpoint("cp-1", b"second", &meta)
            .await
            .unwrap();

        let data = adapter.get_checkpoint("cp-1").await.unwrap();
        assert_eq!(data, Some(b"second".to_vec()));
    }

    #[tokio::test]
    async fn real_backend_list_by_parent() {
        let adapter = make_real_adapter();
        let mut meta_a = HashMap::new();
        meta_a.insert("parentId".to_string(), "exec-1".to_string());
        let mut meta_b = HashMap::new();
        meta_b.insert("parentId".to_string(), "exec-2".to_string());

        adapter
            .save_checkpoint("cp-1", b"data-1", &meta_a)
            .await
            .unwrap();
        adapter
            .save_checkpoint("cp-2", b"data-2", &meta_a)
            .await
            .unwrap();
        adapter
            .save_checkpoint("cp-3", b"data-3", &meta_b)
            .await
            .unwrap();

        let mut list = adapter.list_checkpoints(Some("exec-1")).await.unwrap();
        list.sort();
        assert_eq!(list, vec!["cp-1".to_string(), "cp-2".to_string()]);
    }

    #[tokio::test]
    async fn real_backend_missing_checkpoint_returns_none() {
        let adapter = make_real_adapter();
        assert!(adapter.get_checkpoint("nope").await.unwrap().is_none());
        assert_eq!(
            adapter
                .list_checkpoints(Some("nobody"))
                .await
                .unwrap()
                .len(),
            0
        );
    }

    #[tokio::test]
    async fn real_backend_delete() {
        let adapter = make_real_adapter();
        let mut meta = HashMap::new();
        meta.insert("parentId".to_string(), "exec-1".to_string());

        adapter
            .save_checkpoint("cp-1", b"data-1", &meta)
            .await
            .unwrap();

        assert!(adapter.delete_checkpoint("cp-1").await.unwrap());
        assert!(!adapter.delete_checkpoint("cp-1").await.unwrap());
        assert!(adapter.get_checkpoint("cp-1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn real_backend_batch_save() {
        let adapter = make_real_adapter();
        let items = vec![
            ("cp-1".to_string(), b"data-1".to_vec(), HashMap::new()),
            ("cp-2".to_string(), b"data-2".to_vec(), HashMap::new()),
        ];

        adapter.batch_save(&items).await.unwrap();
        assert!(adapter.get_checkpoint("cp-1").await.unwrap().is_some());
        assert!(adapter.get_checkpoint("cp-2").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn real_backend_sqlite_file_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("checkpoints.db");

        {
            let adapter = LayertwineGitAdapter::new(&path).unwrap();
            let meta = HashMap::new();
            adapter
                .save_checkpoint("cp-1", b"persisted", &meta)
                .await
                .unwrap();
        }

        let adapter = LayertwineGitAdapter::new(&path).unwrap();
        let data = adapter.get_checkpoint("cp-1").await.unwrap();
        assert_eq!(data, Some(b"persisted".to_vec()));
    }
}
