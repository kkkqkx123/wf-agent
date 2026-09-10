use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use layertwine::storage::repository::GraphBlobStore;
use layertwine::storage::sqlite::SqliteStorage;

use crate::branch::BranchStorageAdapter;
use crate::error::CheckpointError;
use crate::file_util::map_layertwine_error;
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

/// Real layertwine backend adapter: persists opaque graph/workflow checkpoint
/// blobs in the dedicated `graph_blobs` table (indexed `parent_id` /
/// `branch_id` columns). Execution branch heads live in layertwine's native
/// `branches` table; a branch created without a resolvable base head starts
/// at the genesis sentinel (reported as `None` until its first checkpoint).
pub struct LayertwineGitAdapter {
    storage: SqliteStorage,
}

/// Sentinel head for branches with no checkpoint yet. Chosen as a fixed
/// content id so headless branches have a valid native row; readers map it
/// back to `None`.
fn genesis_head() -> layertwine::core::types::CheckpointId {
    layertwine::core::types::CheckpointId::from_content(b"wf-execution-branch-genesis")
}

fn is_genesis(head: &layertwine::core::types::CheckpointId) -> bool {
    *head == genesis_head()
}

impl LayertwineGitAdapter {
    pub fn new_in_memory() -> Result<Self, CheckpointError> {
        let storage = SqliteStorage::new_full_in_memory().map_err(map_layertwine_error)?;
        Ok(Self { storage })
    }

    pub fn new(path: &Path) -> Result<Self, CheckpointError> {
        let storage = SqliteStorage::new_full(path).map_err(map_layertwine_error)?;
        Ok(Self { storage })
    }

    /// Create an adapter sharing the connection of an existing
    /// `SqliteStorage` instance (used by `FileCheckpointManager::with_sqlite`
    /// to share the runtime's storage connection).
    pub fn from_shared(storage: Arc<SqliteStorage>) -> Self {
        Self {
            storage: storage.share(),
        }
    }

    /// List checkpoint ids recorded on a branch (branch-scoped listing,
    /// indexed `graph_blobs` query only).
    pub fn list_branch_checkpoints(&self, branch: &str) -> Result<Vec<String>, CheckpointError> {
        let mut ids = self
            .storage
            .list_graph_blob_ids_by_branch(branch)
            .map_err(map_layertwine_error)?;
        ids.sort();
        Ok(ids)
    }
}

impl GitCheckpointAdapter for LayertwineGitAdapter {
    async fn save_checkpoint(
        &self,
        checkpoint_id: &str,
        data: &[u8],
        metadata: &HashMap<String, String>,
    ) -> Result<(), CheckpointError> {
        // Dedicated blob table: no snapshot rows, no comma-joined lists.
        let parent = metadata.get("parentId").map(String::as_str);
        let branch = metadata.get("branchId").map(String::as_str);
        self.storage
            .store_graph_blob(checkpoint_id, data, parent, branch)
            .map_err(map_layertwine_error)?;
        Ok(())
    }

    async fn get_checkpoint(
        &self,
        checkpoint_id: &str,
    ) -> Result<Option<Vec<u8>>, CheckpointError> {
        let blob = self
            .storage
            .load_graph_blob(checkpoint_id)
            .map_err(map_layertwine_error)?;
        Ok(blob.map(|b| b.data))
    }

    async fn list_checkpoints(
        &self,
        parent_id: Option<&str>,
    ) -> Result<Vec<String>, CheckpointError> {
        let mut ids = self
            .storage
            .list_graph_blob_ids(parent_id)
            .map_err(map_layertwine_error)?;
        ids.sort();
        Ok(ids)
    }

    async fn delete_checkpoint(&self, checkpoint_id: &str) -> Result<bool, CheckpointError> {
        self.storage
            .delete_graph_blob(checkpoint_id)
            .map_err(map_layertwine_error)
    }

    async fn batch_save(
        &self,
        items: &[(String, Vec<u8>, HashMap<String, String>)],
    ) -> Result<(), CheckpointError> {
        // Sequential inserts; listing is indexed so batch throughput is
        // dominated by inserts. A future SAVEPOINT-wrapped bulk insert can
        // replace this loop.
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

    /// Clone this adapter sharing the underlying Sqlite connection.
    pub fn share(&self) -> Self {
        Self {
            storage: self.storage.share(),
        }
    }

    /// Update the branch head pointer (execution namespace, native
    /// `branches` table only). The id must parse as a content id; otherwise
    /// a `Branch` error is returned instead of splitting state across a KV
    /// fallback.
    pub fn set_branch_head(
        &self,
        branch: &str,
        checkpoint_id: &str,
    ) -> Result<(), CheckpointError> {
        use layertwine::core::types::CheckpointId;
        use layertwine::storage::repository::CheckpointPersist;

        let Some(head) = CheckpointId::from_hex(checkpoint_id) else {
            return Err(CheckpointError::Branch(format!(
                "branch head must be a content id, got '{checkpoint_id}'"
            )));
        };
        match self.storage.get_branch(branch) {
            Ok(_) => self.storage.update_branch_head(branch, &head),
            Err(layertwine::StorageError::NotFound(_)) => self
                .storage
                .store_branch(&layertwine::checkpoint::branch::Branch::new(branch, head)),
            Err(e) => Err(e),
        }
        .map_err(map_layertwine_error)?;
        Ok(())
    }

    /// Read the branch head pointer from the native table. Headless branches
    /// (genesis sentinel) and missing branches both report `None`.
    pub fn get_branch_head(&self, branch: &str) -> Result<Option<String>, CheckpointError> {
        use layertwine::storage::repository::CheckpointPersist;

        match self.storage.get_branch(branch) {
            Ok(native) if is_genesis(&native.head) => Ok(None),
            Ok(native) => Ok(Some(native.head.to_hex())),
            Err(layertwine::StorageError::NotFound(_)) => Ok(None),
            Err(e) => Err(map_layertwine_error(e)),
        }
    }

    /// Synchronous branch existence check against the native table.
    pub fn branch_exists_now(&self, branch: &str) -> Result<bool, CheckpointError> {
        use layertwine::storage::repository::CheckpointPersist;

        match self.storage.get_branch(branch) {
            Ok(_) => Ok(true),
            Err(layertwine::StorageError::NotFound(_)) => Ok(false),
            Err(e) => Err(map_layertwine_error(e)),
        }
    }

    /// Native head lookup shared by create-time base inheritance. Genesis
    /// heads count as absent so new branches do not inherit the sentinel.
    fn native_head(&self, branch: &str) -> Option<layertwine::core::types::CheckpointId> {
        use layertwine::storage::repository::CheckpointPersist;

        self.storage
            .get_branch(branch)
            .ok()
            .map(|b| b.head)
            .filter(|head| !is_genesis(head))
    }
}

/// Production `BranchStorageAdapter` over the layertwine backend: branches
/// live only in the native `branches` table. A new branch inherits the base
/// branch's real head when available, otherwise starts at the genesis
/// sentinel (reported as `None` until its first checkpoint).
impl BranchStorageAdapter for LayertwineGitAdapter {
    async fn create_branch(&self, name: &str, base: Option<&str>) -> Result<(), CheckpointError> {
        use layertwine::storage::repository::CheckpointPersist;

        if self
            .branch_exists_now(name)
            .map_err(|e| CheckpointError::Branch(e.to_string()))?
        {
            return Err(CheckpointError::Branch(format!(
                "branch '{name}' already exists"
            )));
        }
        // Inherit the base branch head when it names an existing branch with
        // a real head. Raw checkpoint ids are not inherited: forked execution
        // branches stay headless until their own first checkpoint (see
        // `checkpoint_updates_branch_head`).
        let head = base
            .and_then(|b| self.native_head(b))
            .unwrap_or_else(genesis_head);
        self.storage
            .store_branch(&layertwine::checkpoint::branch::Branch::new(name, head))
            .map_err(map_layertwine_error)
            .map_err(|e| CheckpointError::Branch(e.to_string()))?;
        Ok(())
    }

    async fn delete_branch(&self, name: &str) -> Result<(), CheckpointError> {
        use layertwine::storage::repository::CheckpointPersist;

        match self.storage.delete_branch(name) {
            Ok(()) => Ok(()),
            Err(layertwine::StorageError::NotFound(_)) => Ok(()),
            Err(e) => {
                Err(map_layertwine_error(e)).map_err(|e| CheckpointError::Branch(e.to_string()))
            }
        }
    }

    async fn list_branches(&self) -> Result<Vec<String>, CheckpointError> {
        use layertwine::storage::repository::CheckpointPersist;

        let native = self
            .storage
            .list_branches()
            .map_err(map_layertwine_error)
            .map_err(|e| CheckpointError::Branch(e.to_string()))?;
        let mut names: Vec<String> = native
            .into_iter()
            .map(|branch| branch.name)
            .filter(|name| {
                crate::branch::classify_branch(name) == crate::branch::BranchKind::Execution
            })
            .collect();
        names.sort();
        Ok(names)
    }

    async fn branch_exists(&self, name: &str) -> Result<bool, CheckpointError> {
        self.branch_exists_now(name)
            .map_err(|e| CheckpointError::Branch(e.to_string()))
    }

    async fn merge_branch(&self, source: &str, target: &str) -> Result<(), CheckpointError> {
        use layertwine::storage::repository::GraphBlobStore;

        // Storage-level merge: re-point the source's blobs at the target
        // (indexed columns), then move the head.
        let source_ids = self
            .storage
            .list_graph_blob_ids_by_branch(source)
            .map_err(map_layertwine_error)
            .map_err(|e| CheckpointError::Branch(e.to_string()))?;
        for id in &source_ids {
            if let Some(mut blob) = self
                .storage
                .load_graph_blob(id)
                .map_err(map_layertwine_error)
                .map_err(|e| CheckpointError::Branch(e.to_string()))?
            {
                blob.branch_id = Some(target.to_string());
                self.storage
                    .store_graph_blob(
                        &blob.id,
                        &blob.data,
                        blob.parent_id.as_deref(),
                        blob.branch_id.as_deref(),
                    )
                    .map_err(map_layertwine_error)
                    .map_err(|e| CheckpointError::Branch(e.to_string()))?;
            }
        }
        // Move the head pointer when the source has a real head.
        if let Some(head) = self
            .get_branch_head(source)
            .map_err(|e| CheckpointError::Branch(e.to_string()))?
        {
            self.set_branch_head(target, &head)
                .map_err(|e| CheckpointError::Branch(e.to_string()))?;
        }
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
        let gate = Arc::new(ConcurrencyGate::new(BATCH_CONCURRENCY));
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
        let manager = ExecutionBranchManager::new(adapter, "execution/main");

        // Create the default execution branch explicitly (namespaced names
        // only; bare names belong to the feature namespace).
        manager.create_branch("execution/main", None).await.unwrap();
        manager
            .create_branch("execution/feature", Some("execution/main"))
            .await
            .unwrap();

        let mut branches = manager.list_branches().await.unwrap();
        branches.sort();
        assert_eq!(
            branches,
            vec![
                "execution/feature".to_string(),
                "execution/main".to_string()
            ]
        );

        manager.switch_branch("execution/feature").await.unwrap();
        assert_eq!(manager.current_branch().await.unwrap(), "execution/feature");

        manager.delete_branch("execution/feature").await.unwrap();
        let branches = manager.list_branches().await.unwrap();
        assert!(!branches.contains(&"execution/feature".to_string()));
    }

    #[tokio::test]
    async fn real_backend_duplicate_branch_rejected() {
        use crate::branch::BranchStorageAdapter;

        let adapter = make_real_adapter();
        adapter.create_branch("execution/main", None).await.unwrap();
        let err = adapter
            .create_branch("execution/main", None)
            .await
            .unwrap_err();
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
