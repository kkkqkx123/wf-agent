use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use layertwine::core::file_node::FileNode;
use layertwine::core::snapshot::{Snapshot, SnapshotContent};
use layertwine::core::types::SnapshotId;
use layertwine::storage::repository::{MetadataStore, SnapshotStore};
use layertwine::storage::sqlite::SqliteStorage;
use layertwine::StorageResult;

use crate::branch::BranchStorageAdapter;
use crate::error::CheckpointError;
use crate::file::map_layertwine_error;
use wf_common::gate::ConcurrencyGate;

pub trait GitCheckpointAdapter: Send + Sync {
    fn save_checkpoint(
        &self,
        checkpoint_id: &str,
        data: &[u8],
        metadata: &HashMap<String, String>,
    ) -> impl std::future::Future<Output = Result<(), CheckpointError>> + Send;

    fn get_checkpoint(
        &self,
        checkpoint_id: &str,
    ) -> impl std::future::Future<Output = Result<Option<Vec<u8>>, CheckpointError>> + Send;

    fn list_checkpoints(
        &self,
        parent_id: Option<&str>,
    ) -> impl std::future::Future<Output = Result<Vec<String>, CheckpointError>> + Send;

    fn delete_checkpoint(
        &self,
        checkpoint_id: &str,
    ) -> impl std::future::Future<Output = Result<bool, CheckpointError>> + Send;

    fn batch_save(
        &self,
        items: &[(String, Vec<u8>, HashMap<String, String>)],
    ) -> impl std::future::Future<Output = Result<(), CheckpointError>> + Send;
}

fn map_storage_result<T>(result: StorageResult<T>) -> Result<T, CheckpointError> {
    result.map_err(map_layertwine_error)
}

const CP_KEY_PREFIX: &str = "wf-checkpoint:";
const CP_PARENT_PREFIX: &str = "wf-checkpoint-parent:";
const BRANCH_KEY_PREFIX: &str = "wf-checkpoint-branch:";
const BRANCH_CPS_PREFIX: &str = "wf-checkpoint-branch-cps:";

/// Real layertwine backend adapter: persists checkpoint blobs as structured
/// snapshots inside layertwine's SQLite storage (via its `SnapshotStore` /
/// `MetadataStore` repository traits). An O(1) metadata index maps the
/// checkpoint id to the content-addressed snapshot id, and per-parent index
/// lists support `list_checkpoints(parent)`.
pub struct LayertwineGitAdapter {
    storage: SqliteStorage,
}

impl LayertwineGitAdapter {
    pub fn new_in_memory() -> Result<Self, CheckpointError> {
        let storage = map_storage_result(SqliteStorage::new_full_in_memory())?;
        Ok(Self { storage })
    }

    pub fn new(path: &Path) -> Result<Self, CheckpointError> {
        let storage = map_storage_result(SqliteStorage::new_full(path))?;
        Ok(Self { storage })
    }

    fn snapshot_key(checkpoint_id: &str) -> String {
        format!("{CP_KEY_PREFIX}{}", checkpoint_id)
    }

    fn parent_key(parent_id: &str) -> String {
        format!("{CP_PARENT_PREFIX}{}", parent_id)
    }

    fn branch_key(branch: &str) -> String {
        format!("{BRANCH_KEY_PREFIX}{}", branch)
    }

    fn branch_cps_key(branch: &str) -> String {
        format!("{BRANCH_CPS_PREFIX}{}", branch)
    }

    fn load_id_list(&self, key: &str) -> Result<Vec<String>, CheckpointError> {
        match map_storage_result(self.storage.load_metadata(key))? {
            Some(list) if !list.is_empty() => Ok(list.split(',').map(String::from).collect()),
            _ => Ok(Vec::new()),
        }
    }

    fn store_id_list(&self, key: &str, ids: &[String]) -> Result<(), CheckpointError> {
        map_storage_result(self.storage.store_metadata(key, &ids.join(",")))
    }

    /// List checkpoint ids recorded on a branch (branch-scoped listing).
    pub fn list_branch_checkpoints(&self, branch: &str) -> Result<Vec<String>, CheckpointError> {
        self.load_id_list(&Self::branch_cps_key(branch))
    }
}

impl GitCheckpointAdapter for LayertwineGitAdapter {
    async fn save_checkpoint(
        &self,
        checkpoint_id: &str,
        data: &[u8],
        metadata: &HashMap<String, String>,
    ) -> Result<(), CheckpointError> {
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

        if let Some(branch) = metadata.get("branchId") {
            let list_key = Self::branch_cps_key(branch);
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
    ) -> Result<Option<Vec<u8>>, CheckpointError> {
        let snapshot_hex = match map_storage_result(
            self.storage
                .load_metadata(&Self::snapshot_key(checkpoint_id)),
        )? {
            Some(hex) if !hex.is_empty() => hex,
            _ => return Ok(None),
        };

        let snapshot_id = SnapshotId::from_hex(&snapshot_hex).ok_or_else(|| {
            CheckpointError::Serialization(format!(
                "invalid stored snapshot id '{}' for checkpoint {}",
                snapshot_hex, checkpoint_id
            ))
        })?;

        let snapshot =
            map_storage_result(self.storage.get_snapshot(&snapshot_id)).map_err(|e| match e {
                CheckpointError::NotFound { .. } => CheckpointError::NotFound {
                    id: checkpoint_id.to_string(),
                },
                other => other,
            })?;

        Ok(snapshot.content.map(|content| content.to_bytes()))
    }

    async fn list_checkpoints(
        &self,
        parent_id: Option<&str>,
    ) -> Result<Vec<String>, CheckpointError> {
        match parent_id {
            Some(parent) => self.load_id_list(&Self::parent_key(parent)),
            None => {
                // No global enumeration over metadata keys; per-parent lists
                // are the canonical listing entry point.
                Ok(Vec::new())
            }
        }
    }

    async fn delete_checkpoint(&self, checkpoint_id: &str) -> Result<bool, CheckpointError> {
        let key = Self::snapshot_key(checkpoint_id);
        let exists = matches!(
            map_storage_result(self.storage.load_metadata(&key))?,
            Some(hex) if !hex.is_empty()
        );
        if !exists {
            return Ok(false);
        }

        // Real pointer deletion: the metadata index row is removed. The
        // content-addressed snapshot blob stays (INSERT-ONLY) and is
        // reclaimed separately by physical GC.
        map_storage_result(self.storage.delete_metadata(&key))?;

        Ok(true)
    }

    async fn batch_save(
        &self,
        items: &[(String, Vec<u8>, HashMap<String, String>)],
    ) -> Result<(), CheckpointError> {
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

    /// Clone this adapter sharing the underlying SQLite connection.
    pub fn share(&self) -> Self {
        Self {
            storage: self.storage.share(),
        }
    }
}

/// Production `BranchStorageAdapter` over the layertwine backend: branches
/// are registered in the metadata store under `wf-checkpoint-branch:{name}`
/// (value `{base}|{created_at}`), and each branch keeps its own checkpoint
/// id list (`wf-checkpoint-branch-cps:{name}`) for branch-scoped isolation.
/// Merging unions the source branch's checkpoint list into the target's.
impl BranchStorageAdapter for LayertwineGitAdapter {
    async fn create_branch(&self, name: &str, base: Option<&str>) -> Result<(), CheckpointError> {
        let key = Self::branch_key(name);
        match map_storage_result(self.storage.load_metadata(&key))
            .map_err(|e| CheckpointError::Branch(e.to_string()))?
        {
            Some(_) => Err(CheckpointError::Branch(format!(
                "branch '{}' already exists",
                name
            ))),
            _ => {
                let value = format!(
                    "{}|{}",
                    base.unwrap_or_default(),
                    chrono::Utc::now().timestamp_millis()
                );
                map_storage_result(self.storage.store_metadata(&key, &value))
                    .map_err(|e| CheckpointError::Branch(e.to_string()))?;
                Ok(())
            }
        }
    }

    async fn delete_branch(&self, name: &str) -> Result<(), CheckpointError> {
        map_storage_result(self.storage.delete_metadata(&Self::branch_key(name)))
            .map_err(|e| CheckpointError::Branch(e.to_string()))?;
        map_storage_result(self.storage.delete_metadata(&Self::branch_cps_key(name)))
            .map_err(|e| CheckpointError::Branch(e.to_string()))?;
        Ok(())
    }

    async fn list_branches(&self) -> Result<Vec<String>, CheckpointError> {
        let entries = map_storage_result(self.storage.list_metadata_by_prefix(BRANCH_KEY_PREFIX))
            .map_err(|e| CheckpointError::Branch(e.to_string()))?;
        Ok(entries
            .into_iter()
            .filter_map(|(key, _)| {
                key.strip_prefix(BRANCH_KEY_PREFIX)
                    .map(|name| name.to_string())
            })
            .collect())
    }

    async fn branch_exists(&self, name: &str) -> Result<bool, CheckpointError> {
        let value = map_storage_result(self.storage.load_metadata(&Self::branch_key(name)))
            .map_err(|e| CheckpointError::Branch(e.to_string()))?;
        Ok(value.is_some())
    }

    async fn merge_branch(&self, source: &str, target: &str) -> Result<(), CheckpointError> {
        // Storage-level merge: the target's checkpoint list becomes the
        // union of both branches' lists (source history absorbed).
        let source_ids = self
            .list_branch_checkpoints(source)
            .map_err(|e| CheckpointError::Branch(e.to_string()))?;
        let mut target_ids = self
            .list_branch_checkpoints(target)
            .map_err(|e| CheckpointError::Branch(e.to_string()))?;
        for id in source_ids {
            if !target_ids.contains(&id) {
                target_ids.push(id);
            }
        }
        self.store_id_list(&Self::branch_cps_key(target), &target_ids)
            .map_err(|e| CheckpointError::Branch(e.to_string()))?;
        Ok(())
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
    ) -> Result<(), CheckpointError> {
        let mut branches = self.branches.write().await;
        branches.insert(checkpoint_id.to_string(), data.to_vec());
        let mut metadata = self.metadata.write().await;
        metadata.insert(checkpoint_id.to_string(), meta.clone());
        Ok(())
    }

    async fn get_checkpoint(
        &self,
        checkpoint_id: &str,
    ) -> Result<Option<Vec<u8>>, CheckpointError> {
        let branches = self.branches.read().await;
        Ok(branches.get(checkpoint_id).cloned())
    }

    async fn list_checkpoints(
        &self,
        parent_id: Option<&str>,
    ) -> Result<Vec<String>, CheckpointError> {
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

    async fn delete_checkpoint(&self, checkpoint_id: &str) -> Result<bool, CheckpointError> {
        let mut branches = self.branches.write().await;
        let mut metadata = self.metadata.write().await;
        let removed = branches.remove(checkpoint_id).is_some();
        metadata.remove(checkpoint_id);
        Ok(removed)
    }

    async fn batch_save(
        &self,
        items: &[(String, Vec<u8>, HashMap<String, String>)],
    ) -> Result<(), CheckpointError> {
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

    fn build_metadata(&self, entity_type: &str, entity_id: &str) -> HashMap<String, String> {
        let mut meta = HashMap::new();
        meta.insert("entityType".to_string(), entity_type.to_string());
        meta.insert("entityId".to_string(), entity_id.to_string());
        meta.insert("parentId".to_string(), entity_id.to_string());
        meta.insert(
            "timestamp".to_string(),
            chrono::Utc::now().timestamp_millis().to_string(),
        );
        meta
    }

    /// Save a checkpoint, validating its structure first (save-time
    /// validation; throws on invalid blobs).
    pub async fn save(
        &self,
        checkpoint_id: &str,
        data: &[u8],
        entity_type: &str,
        entity_id: &str,
    ) -> Result<(), CheckpointError> {
        Self::validate_checkpoint_structure(data)?;
        let meta = self.build_metadata(entity_type, entity_id);
        self.adapter
            .save_checkpoint(checkpoint_id, data, &meta)
            .await
    }

    /// Load a checkpoint and validate its structure, warning (not failing)
    /// on structural issues (load-time structural validation).
    pub async fn load(&self, checkpoint_id: &str) -> Result<Option<Vec<u8>>, CheckpointError> {
        let data = self.adapter.get_checkpoint(checkpoint_id).await?;
        if let Some(data) = &data {
            for warning in Self::validate_checkpoint_structure_soft(data) {
                tracing::warn!(
                    checkpoint_id = %checkpoint_id,
                    "checkpoint structure warning: {}",
                    warning
                );
            }
        }
        Ok(data)
    }

    /// Batch save with per-item metadata; the adapter decides batching
    /// strategy (the real backend writes sequentially, in-memory uses a
    /// single lock).
    pub async fn batch_save(
        &self,
        items: &[(String, Vec<u8>, String, String)],
    ) -> Result<(), CheckpointError> {
        for (id, data, entity_type, entity_id) in items {
            self.save(id, data, entity_type, entity_id).await?;
        }
        Ok(())
    }

    /// Batch load with bounded concurrency (batches of 10); per-item
    /// failures yield `None` instead of failing the whole batch.
    pub async fn batch_load(&self, ids: &[String]) -> Result<Vec<Option<Vec<u8>>>, CheckpointError>
    where
        T: 'static,
    {
        const BATCH_CONCURRENCY: usize = 10;
        let gate = Arc::new(
            ConcurrencyGate::new(BATCH_CONCURRENCY),
        );
        let mut handles = Vec::new();
        for id in ids {
            let adapter = self.adapter.clone();
            let id = id.clone();
            let gate = gate.clone();
            handles.push(tokio::spawn(async move {
                let _permit = match gate.acquire_wait().await {
                    Ok(permit) => permit,
                    Err(e) => {
                        return Err(CheckpointError::Internal(format!(
                            "batch load gate acquire failed: {e}"
                        )))
                    }
                };
                adapter.get_checkpoint(&id).await
            }));
        }
        let mut results = Vec::with_capacity(handles.len());
        for handle in handles {
            match handle.await {
                Ok(Ok(data)) => results.push(data),
                _ => results.push(None),
            }
        }
        Ok(results)
    }

    pub async fn list(&self) -> Result<Vec<String>, CheckpointError> {
        self.adapter.list_checkpoints(None).await
    }

    /// List checkpoints whose parent entity matches `parent_id`.
    pub async fn list_by_parent(&self, parent_id: &str) -> Result<Vec<String>, CheckpointError> {
        self.adapter.list_checkpoints(Some(parent_id)).await
    }

    /// Delete a checkpoint from the backend. Returns true when it existed.
    pub async fn delete(&self, checkpoint_id: &str) -> Result<bool, CheckpointError> {
        self.adapter.delete_checkpoint(checkpoint_id).await
    }

    /// Validate the checkpoint structure before saving: the blob must be
    /// JSON carrying a non-empty `id` and a `type` of `FULL`/`DELTA`;
    /// throws on invalid blobs.
    pub fn validate_checkpoint_structure(data: &[u8]) -> Result<(), CheckpointError> {
        let value: serde_json::Value = serde_json::from_slice(data).map_err(|e| {
            CheckpointError::Serialization(format!("checkpoint blob is not JSON: {}", e))
        })?;
        match value.get("id").and_then(|v| v.as_str()) {
            Some(id) if !id.is_empty() => {}
            _ => {
                return Err(CheckpointError::Serialization(
                    "checkpoint missing non-empty 'id'".to_string(),
                ))
            }
        }
        match value.get("type").and_then(|v| v.as_str()) {
            Some(t) if t.eq_ignore_ascii_case("full") || t.eq_ignore_ascii_case("delta") => Ok(()),
            _ => Err(CheckpointError::Serialization(format!(
                "checkpoint has invalid 'type' (expected FULL/DELTA): {}",
                value
                    .get("type")
                    .map(|t| t.to_string())
                    .unwrap_or_else(|| "<missing>".to_string())
            ))),
        }
    }

    /// Load-time structural validation, returning warnings instead of
    /// failing.
    pub fn validate_checkpoint_structure_soft(data: &[u8]) -> Vec<String> {
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(data) else {
            return vec!["checkpoint blob is not JSON".to_string()];
        };
        let mut warnings = Vec::new();
        if value
            .get("id")
            .and_then(|v| v.as_str())
            .is_none_or(str::is_empty)
        {
            warnings.push("checkpoint missing 'id'".to_string());
        }
        if value.get("type").is_none() {
            warnings.push("checkpoint missing 'type'".to_string());
        }
        match value.get("type").and_then(|v| v.as_str()) {
            Some(t) if t.eq_ignore_ascii_case("delta") => {
                if value
                    .get("baseCheckpointId")
                    .and_then(|v| v.as_str())
                    .is_none_or(str::is_empty)
                {
                    warnings.push("delta checkpoint missing 'baseCheckpointId'".to_string());
                }
                if value
                    .get("previousCheckpointId")
                    .and_then(|v| v.as_str())
                    .is_none_or(str::is_empty)
                {
                    warnings.push("delta checkpoint missing 'previousCheckpointId'".to_string());
                }
                if value.get("delta").is_none() {
                    warnings.push("delta checkpoint missing 'delta'".to_string());
                }
            }
            Some(t) if t.eq_ignore_ascii_case("full") && value.get("snapshot").is_none() => {
                warnings.push("full checkpoint missing 'snapshot'".to_string());
            }
            _ => {}
        }
        warnings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal valid checkpoint blob: JSON with id + type (FULL/DELTA).
    fn make_blob(id: &str, cp_type: &str) -> Vec<u8> {
        serde_json::json!({
            "id": id,
            "type": cp_type,
            "snapshot": {"state": "ok"},
        })
        .to_string()
        .into_bytes()
    }

    #[tokio::test]
    async fn save_and_load_checkpoint() {
        let adapter = Arc::new(InMemoryGitAdapter::new());
        let bridge = LayertwineCheckpointBridge::new(adapter);

        bridge
            .save("cp-1", &make_blob("cp-1", "FULL"), "workflow", "exec-1")
            .await
            .unwrap();

        let data = bridge.load("cp-1").await.unwrap();
        assert_eq!(data, Some(make_blob("cp-1", "FULL")));
    }

    #[tokio::test]
    async fn save_rejects_invalid_checkpoint_structure() {
        let adapter = Arc::new(InMemoryGitAdapter::new());
        let bridge = LayertwineCheckpointBridge::new(adapter);

        let err = bridge
            .save("cp-1", b"not-json", "workflow", "exec-1")
            .await
            .unwrap_err();
        assert!(matches!(err, CheckpointError::Serialization(_)));

        let err = bridge
            .save("cp-2", b"{}", "workflow", "exec-1")
            .await
            .unwrap_err();
        assert!(matches!(err, CheckpointError::Serialization(_)));
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
            .save("cp-1", &make_blob("cp-1", "FULL"), "workflow", "exec-1")
            .await
            .unwrap();
        bridge
            .save("cp-2", &make_blob("cp-2", "FULL"), "workflow", "exec-2")
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
            .save("cp-1", &make_blob("cp-1", "FULL"), "workflow", "exec-1")
            .await
            .unwrap();
        bridge
            .save("cp-2", &make_blob("cp-2", "FULL"), "workflow", "exec-2")
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
            .save("cp-1", &make_blob("cp-1", "FULL"), "workflow", "exec-1")
            .await
            .unwrap();

        assert!(bridge.delete("cp-1").await.unwrap());
        assert!(!bridge.delete("cp-1").await.unwrap());
        assert!(bridge.load("cp-1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn bridge_batch_save_and_load() {
        let adapter = Arc::new(InMemoryGitAdapter::new());
        let bridge = LayertwineCheckpointBridge::new(adapter);

        let items = vec![
            (
                "cp-1".to_string(),
                make_blob("cp-1", "FULL"),
                "workflow".to_string(),
                "exec-1".to_string(),
            ),
            (
                "cp-2".to_string(),
                make_blob("cp-2", "DELTA"),
                "workflow".to_string(),
                "exec-1".to_string(),
            ),
        ];
        bridge.batch_save(&items).await.unwrap();

        let loaded = bridge
            .batch_load(&["cp-1".to_string(), "cp-2".to_string()])
            .await
            .unwrap();
        assert_eq!(loaded.len(), 2);
        assert!(loaded[0].is_some());
        assert!(loaded[1].is_some());

        // Missing ids yield None, not a batch failure.
        let loaded = bridge
            .batch_load(&["cp-1".to_string(), "nope".to_string()])
            .await
            .unwrap();
        assert!(loaded[0].is_some());
        assert!(loaded[1].is_none());
    }

    #[test]
    fn structure_validation_soft_reports_warnings() {
        let delta = serde_json::json!({
            "id": "cp-1",
            "type": "DELTA",
            "snapshot": {"state": 1},
        })
        .to_string()
        .into_bytes();
        let warnings =
            LayertwineCheckpointBridge::<InMemoryGitAdapter>::validate_checkpoint_structure_soft(
                &delta,
            );
        assert!(
            warnings.iter().any(|w| w.contains("baseCheckpointId")),
            "delta without baseCheckpointId reported"
        );
        assert!(
            warnings.iter().any(|w| w.contains("previousCheckpointId")),
            "delta without previousCheckpointId reported"
        );

        let full = make_blob("cp-2", "FULL");
        let warnings =
            LayertwineCheckpointBridge::<InMemoryGitAdapter>::validate_checkpoint_structure_soft(
                &full,
            );
        assert!(warnings.is_empty());
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

    // ---- Real backend branch isolation (BranchStorageAdapter) ----

    fn make_branch_meta(branch: &str) -> HashMap<String, String> {
        let mut meta = HashMap::new();
        meta.insert("branchId".to_string(), branch.to_string());
        meta
    }

    #[tokio::test]
    async fn real_backend_branch_lifecycle() {
        use crate::branch::BranchManager;
        use crate::branch::ExecutionBranchManager;

        let adapter = make_real_adapter();
        let manager = ExecutionBranchManager::new(adapter, "main");

        // Create the default branch registry entry explicitly.
        manager.create_branch("main", None).await.unwrap();
        manager
            .create_branch("feature", Some("main"))
            .await
            .unwrap();

        let mut branches = manager.list_branches().await.unwrap();
        branches.sort();
        assert_eq!(branches, vec!["feature".to_string(), "main".to_string()]);

        manager.switch_branch("feature").await.unwrap();
        assert_eq!(manager.current_branch().await.unwrap(), "feature");

        manager.delete_branch("feature").await.unwrap();
        let branches = manager.list_branches().await.unwrap();
        assert!(!branches.contains(&"feature".to_string()));
    }

    #[tokio::test]
    async fn real_backend_duplicate_branch_rejected() {
        use crate::branch::BranchStorageAdapter;

        let adapter = make_real_adapter();
        adapter.create_branch("main", None).await.unwrap();
        let err = adapter.create_branch("main", None).await.unwrap_err();
        assert!(matches!(err, crate::error::CheckpointError::Branch(_)));
    }

    #[tokio::test]
    async fn real_backend_branch_scoped_checkpoints() {
        use crate::branch::BranchManager;
        use crate::branch::ExecutionBranchManager;

        let adapter = make_real_adapter();
        let probe = adapter.share();
        let manager = ExecutionBranchManager::new(adapter, "main");
        manager.create_branch("main", None).await.unwrap();
        manager
            .create_branch("feature", Some("main"))
            .await
            .unwrap();

        // Checkpoints on two branches stay isolated.
        probe
            .save_checkpoint("cp-main-1", b"m1", &make_branch_meta("main"))
            .await
            .unwrap();
        probe
            .save_checkpoint("cp-main-2", b"m2", &make_branch_meta("main"))
            .await
            .unwrap();
        probe
            .save_checkpoint("cp-feat-1", b"f1", &make_branch_meta("feature"))
            .await
            .unwrap();

        let mut main_cps = probe.list_branch_checkpoints("main").unwrap();
        main_cps.sort();
        assert_eq!(
            main_cps,
            vec!["cp-main-1".to_string(), "cp-main-2".to_string()]
        );
        assert_eq!(
            probe.list_branch_checkpoints("feature").unwrap(),
            vec!["cp-feat-1".to_string()]
        );

        // Merge absorbs the source branch's checkpoints into the target.
        manager.merge_branch("feature", "main").await.unwrap();
        let mut merged = probe.list_branch_checkpoints("main").unwrap();
        merged.sort();
        assert_eq!(merged.len(), 3);
        assert!(merged.contains(&"cp-feat-1".to_string()));
    }
}
