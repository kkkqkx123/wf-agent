use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use crate::branch::BranchStorageAdapter;
use crate::storage::{GraphBlobStore, SqliteStorage};
use checkpoint_base::error::CheckpointError;
use wf_common::gate::ConcurrencyGate;

pub trait CheckpointBackend: Send + Sync {
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

/// Storage adapter over two separate tables.
/// Responsibility split inside this file: graph blobs live in `graph_blobs`
/// (execution state), execution branch heads live in `meta_kv` under
/// `exec_branch/`. File branches (review/feature/main) live as Git refs.
/// A branch created without a resolvable base head starts headless
/// (reported as `None` until its first checkpoint).
pub struct SqliteBackend {
    storage: SqliteStorage,
}

impl SqliteBackend {
    pub fn new_in_memory() -> Result<Self, CheckpointError> {
        let storage = SqliteStorage::new_full_in_memory()?;
        Ok(Self { storage })
    }

    pub fn new(path: &Path) -> Result<Self, CheckpointError> {
        let storage = SqliteStorage::new_full(path)?;
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
        let mut ids = self.storage.list_graph_blob_ids_by_branch(branch)?;
        ids.sort();
        Ok(ids)
    }
}

impl CheckpointBackend for SqliteBackend {
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
            .store_graph_blob(checkpoint_id, data, parent, branch)?;
        Ok(())
    }

    async fn get_checkpoint(
        &self,
        checkpoint_id: &str,
    ) -> Result<Option<Vec<u8>>, CheckpointError> {
        let blob = self.storage.load_graph_blob(checkpoint_id)?;
        Ok(blob.map(|b| b.data))
    }

    async fn list_checkpoints(
        &self,
        parent_id: Option<&str>,
    ) -> Result<Vec<String>, CheckpointError> {
        let mut ids = self.storage.list_graph_blob_ids(parent_id)?;
        ids.sort();
        Ok(ids)
    }

    async fn delete_checkpoint(&self, checkpoint_id: &str) -> Result<bool, CheckpointError> {
        self.storage.delete_graph_blob(checkpoint_id)
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

/// Execution branch heads: `meta_kv` rows under `exec_branch/` only.
/// File branches (review/feature/main) live as Git refs; execution branches
/// (graph-blob history) keep a lightweight pointer here.
impl SqliteBackend {
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

    fn branch_key(branch: &str) -> String {
        format!("exec_branch/{branch}")
    }

    /// Update the branch head pointer (execution namespace). Any non-empty
    /// id is accepted; missing branches are created.
    pub fn set_branch_head(
        &self,
        branch: &str,
        checkpoint_id: &str,
    ) -> Result<(), CheckpointError> {
        use crate::storage::MetadataStore;

        if checkpoint_id.is_empty() {
            return Err(CheckpointError::Branch(format!(
                "branch head must not be empty for '{branch}'"
            )));
        }
        self.storage
            .store_metadata(&Self::branch_key(branch), checkpoint_id)?;
        Ok(())
    }

    /// Read the branch head pointer. Missing branches and headless
    /// branches (empty marker) both report `None`.
    pub fn get_branch_head(&self, branch: &str) -> Result<Option<String>, CheckpointError> {
        use crate::storage::MetadataStore;

        Ok(self
            .storage
            .load_metadata(&Self::branch_key(branch))?
            .filter(|head| !head.is_empty()))
    }

    /// Synchronous branch existence check (headless branches exist).
    pub fn branch_exists_now(&self, branch: &str) -> Result<bool, CheckpointError> {
        use crate::storage::MetadataStore;

        Ok(self
            .storage
            .load_metadata(&Self::branch_key(branch))?
            .is_some())
    }

    /// Head lookup shared by create-time base inheritance.
    fn native_head(&self, branch: &str) -> Option<String> {
        self.get_branch_head(branch).ok().flatten()
    }

    /// Delete an execution branch pointer. Missing branches are a no-op.
    pub fn delete_branch_head(&self, branch: &str) -> Result<(), CheckpointError> {
        use crate::storage::MetadataStore;

        self.storage.delete_metadata(&Self::branch_key(branch))?;
        Ok(())
    }

    /// Execution-namespace branch names only. The namespace rule lives here
    /// alongside the pointer rows so callers never reimplement the filter.
    pub fn list_execution_branch_names(&self) -> Result<Vec<String>, CheckpointError> {
        let rows = self
            .storage
            .list_metadata_by_prefix("exec_branch/")
            .map_err(|e| CheckpointError::Branch(e.to_string()))?;
        let mut names: Vec<String> = rows
            .into_iter()
            .map(|(key, _)| key.trim_start_matches("exec_branch/").to_string())
            .filter(|name| crate::branch::is_execution_branch_name(name))
            .collect();
        names.sort();
        Ok(names)
    }
}

/// Production `BranchStorageAdapter` over the sqlite backend: execution
/// branch pointers live in `meta_kv`. A new branch inherits the base
/// branch's head when available, otherwise starts headless (reported as
/// `None` until its first checkpoint).
impl BranchStorageAdapter for SqliteBackend {
    async fn create_branch(&self, name: &str, base: Option<&str>) -> Result<(), CheckpointError> {
        if !crate::branch::is_execution_branch_name(name) {
            return Err(CheckpointError::Branch(format!(
                "execution branch name must start with 'execution/' and carry an id: '{name}'"
            )));
        }
        if let Some(base_name) = base {
            if !crate::branch::is_execution_branch_name(base_name) {
                return Err(CheckpointError::Branch(format!(
                    "base must be an execution branch name, got '{base_name}'"
                )));
            }
        }
        if self
            .branch_exists_now(name)
            .map_err(|e| CheckpointError::Branch(e.to_string()))?
        {
            return Err(CheckpointError::Branch(format!(
                "branch '{name}' already exists"
            )));
        }
        if let Some(head) = base.and_then(|b| self.native_head(b)) {
            self.set_branch_head(name, &head)
                .map_err(|e| CheckpointError::Branch(e.to_string()))?;
        } else {
            use crate::storage::MetadataStore;
            self.storage
                .store_metadata(&Self::branch_key(name), "")
                .map_err(|e| CheckpointError::Branch(e.to_string()))?;
        }
        Ok(())
    }

    async fn delete_branch(&self, name: &str) -> Result<(), CheckpointError> {
        if !crate::branch::is_execution_branch_name(name) {
            return Err(CheckpointError::Branch(format!(
                "execution branch name must start with 'execution/' and carry an id: '{name}'"
            )));
        }
        self.delete_branch_head(name)
            .map_err(|e| CheckpointError::Branch(e.to_string()))?;
        Ok(())
    }

    async fn list_branches(&self) -> Result<Vec<String>, CheckpointError> {
        self.list_execution_branch_names()
    }

    async fn branch_exists(&self, name: &str) -> Result<bool, CheckpointError> {
        self.branch_exists_now(name)
            .map_err(|e| CheckpointError::Branch(e.to_string()))
    }

    async fn merge_execution_history(
        &self,
        source: &str,
        target: &str,
    ) -> Result<(), CheckpointError> {
        use crate::storage::repository::AtomicOps;
        use crate::storage::GraphBlobStore;

        if !crate::branch::is_execution_branch_name(source) {
            return Err(CheckpointError::Branch(format!(
                "source must be an execution branch name, got '{source}'"
            )));
        }
        if !crate::branch::is_execution_branch_name(target) {
            return Err(CheckpointError::Branch(format!(
                "target must be an execution branch name, got '{target}'"
            )));
        }
        if source == target {
            return Err(CheckpointError::Branch(format!(
                "cannot merge branch '{source}' into itself"
            )));
        }
        self.storage.with_atomic(|storage| {
            let source_ids = storage
                .list_graph_blob_ids_by_branch(source)
                .map_err(|e| CheckpointError::Branch(e.to_string()))?;
            for id in &source_ids {
                if let Some(mut blob) = storage
                    .load_graph_blob(id)
                    .map_err(|e| CheckpointError::Branch(e.to_string()))?
                {
                    blob.branch_id = Some(target.to_string());
                    storage
                        .store_graph_blob(
                            &blob.id,
                            &blob.data,
                            blob.parent_id.as_deref(),
                            blob.branch_id.as_deref(),
                        )
                        .map_err(|e| CheckpointError::Branch(e.to_string()))?;
                }
            }
            if let Some(head) = self
                .get_branch_head(source)
                .map_err(|e| CheckpointError::Branch(e.to_string()))?
            {
                self.set_branch_head(target, &head)
                    .map_err(|e| CheckpointError::Branch(e.to_string()))?;
            }
            Ok(())
        })?;
        Ok(())
    }
}

/// Validation/serialization facade over the sqlite backend.
/// The former generic parameter only ever materialized as the in-memory
/// test double, which now lives in the test module.
pub struct CheckpointBridge {
    adapter: Arc<SqliteBackend>,
}

impl CheckpointBridge {
    pub fn new(adapter: Arc<SqliteBackend>) -> Self {
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
    pub async fn batch_load(
        &self,
        ids: &[String],
    ) -> Result<Vec<Option<Vec<u8>>>, CheckpointError> {
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
        let adapter = Arc::new(make_real_adapter());
        let bridge = CheckpointBridge::new(adapter);

        bridge
            .save("cp-1", &make_blob("cp-1", "FULL"), "workflow", "exec-1")
            .await
            .unwrap();

        let data = bridge.load("cp-1").await.unwrap();
        assert_eq!(data, Some(make_blob("cp-1", "FULL")));
    }

    #[tokio::test]
    async fn save_rejects_invalid_checkpoint_structure() {
        let adapter = Arc::new(make_real_adapter());
        let bridge = CheckpointBridge::new(adapter);

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
        let adapter = Arc::new(make_real_adapter());
        let bridge = CheckpointBridge::new(adapter);

        let data = bridge.load("nonexistent").await.unwrap();
        assert!(data.is_none());
    }

    #[tokio::test]
    async fn list_checkpoints() {
        let adapter = Arc::new(make_real_adapter());
        let bridge = CheckpointBridge::new(adapter);

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
        let adapter = Arc::new(make_real_adapter());
        let bridge = CheckpointBridge::new(adapter);

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
        let adapter = Arc::new(make_real_adapter());
        let bridge = CheckpointBridge::new(adapter);

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
        let adapter = Arc::new(make_real_adapter());
        let bridge = CheckpointBridge::new(adapter);

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
        let warnings = CheckpointBridge::validate_checkpoint_structure_soft(&delta);
        assert!(
            warnings.iter().any(|w| w.contains("baseCheckpointId")),
            "delta without baseCheckpointId reported"
        );
        assert!(
            warnings.iter().any(|w| w.contains("previousCheckpointId")),
            "delta without previousCheckpointId reported"
        );

        let full = make_blob("cp-2", "FULL");
        let warnings = CheckpointBridge::validate_checkpoint_structure_soft(&full);
        assert!(warnings.is_empty());
    }

    #[tokio::test]
    async fn batch_save() {
        let adapter = Arc::new(make_real_adapter());

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

    // ---- Real sqlite backend integration tests ----

    fn make_real_adapter() -> SqliteBackend {
        SqliteBackend::new_in_memory().unwrap()
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
            let adapter = SqliteBackend::new(&path).unwrap();
            let meta = HashMap::new();
            adapter
                .save_checkpoint("cp-1", b"persisted", &meta)
                .await
                .unwrap();
        }

        let adapter = SqliteBackend::new(&path).unwrap();
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
        assert!(matches!(
            err,
            checkpoint_base::error::CheckpointError::Branch(_)
        ));
    }

    #[tokio::test]
    async fn real_backend_branch_scoped_checkpoints() {
        use crate::branch::BranchManager;
        use crate::branch::ExecutionBranchManager;

        let adapter = make_real_adapter();
        let probe = adapter.share();
        let manager = ExecutionBranchManager::new(adapter, "execution/main");
        manager.create_branch("execution/main", None).await.unwrap();
        manager
            .create_branch("execution/feature", Some("execution/main"))
            .await
            .unwrap();

        // Checkpoints on two branches stay isolated.
        probe
            .save_checkpoint("cp-main-1", b"m1", &make_branch_meta("execution/main"))
            .await
            .unwrap();
        probe
            .save_checkpoint("cp-main-2", b"m2", &make_branch_meta("execution/main"))
            .await
            .unwrap();
        probe
            .save_checkpoint("cp-feat-1", b"f1", &make_branch_meta("execution/feature"))
            .await
            .unwrap();

        let mut main_cps = probe.list_branch_checkpoints("execution/main").unwrap();
        main_cps.sort();
        assert_eq!(
            main_cps,
            vec!["cp-main-1".to_string(), "cp-main-2".to_string()]
        );
        assert_eq!(
            probe.list_branch_checkpoints("execution/feature").unwrap(),
            vec!["cp-feat-1".to_string()]
        );

        // Merge absorbs the source branch's checkpoints into the target.
        manager
            .merge_execution_branch("execution/feature", "execution/main")
            .await
            .unwrap();
        let mut merged = probe.list_branch_checkpoints("execution/main").unwrap();
        merged.sort();
        assert_eq!(merged.len(), 3);
        assert!(merged.contains(&"cp-feat-1".to_string()));
    }
}
