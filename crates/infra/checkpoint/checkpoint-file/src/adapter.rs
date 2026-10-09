use std::sync::Arc;

use crate::branch::ExecutionPointerAdapter;
use crate::storage::SqliteStorage;
use checkpoint_base::error::CheckpointError;

/// Storage adapter for execution pointer heads.
/// Execution pointer heads live in `meta_kv` under `exec_pointer/`.
/// File branches (review/feature/main) live as Git refs.
/// A pointer created without a resolvable base head starts headless
/// (reported as `None` until its first checkpoint).
pub struct SqliteBackend {
    storage: SqliteStorage,
}

impl SqliteBackend {
    /// Create an adapter sharing the connection of an existing
    /// `SqliteStorage` instance (used by `FileCheckpointManager::with_sqlite`
    /// to share the runtime's storage connection).
    pub fn from_shared(storage: Arc<SqliteStorage>) -> Self {
        Self {
            storage: storage.share(),
        }
    }
}

/// Execution pointer heads: `meta_kv` rows under `exec_pointer/` only.
/// File branches (review/feature/main) live as Git refs; execution pointers
/// keep a lightweight pointer here.
impl SqliteBackend {
    fn pointer_key(branch: &str) -> String {
        format!("exec_pointer/{branch}")
    }

    /// Update the pointer head (execution namespace). Any non-empty
    /// id is accepted; missing pointers are created.
    pub fn set_pointer_head(
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
            .store_metadata(&Self::pointer_key(branch), checkpoint_id)?;
        Ok(())
    }

    /// Read the pointer head. Missing pointers and headless
    /// pointers (empty marker) both report `None`.
    pub fn get_pointer_head(&self, branch: &str) -> Result<Option<String>, CheckpointError> {
        use crate::storage::MetadataStore;

        Ok(self
            .storage
            .load_metadata(&Self::pointer_key(branch))?
            .filter(|head| !head.is_empty()))
    }

    /// Synchronous pointer existence check (headless pointers exist).
    pub fn pointer_exists_now(&self, branch: &str) -> Result<bool, CheckpointError> {
        use crate::storage::MetadataStore;

        Ok(self
            .storage
            .load_metadata(&Self::pointer_key(branch))?
            .is_some())
    }

    /// Head lookup shared by create-time base inheritance.
    fn native_pointer_head(&self, branch: &str) -> Option<String> {
        self.get_pointer_head(branch).ok().flatten()
    }
}

/// Production `ExecutionPointerAdapter` over the sqlite backend: execution
/// pointer values live in `meta_kv`. A new pointer inherits the base
/// pointer's head when available, otherwise starts headless (reported as
/// `None` until its first checkpoint).
impl ExecutionPointerAdapter for SqliteBackend {
    async fn create_pointer(&self, name: &str, base: Option<&str>) -> Result<(), CheckpointError> {
        if !crate::branch::is_execution_pointer_name(name) {
            return Err(CheckpointError::Branch(format!(
                "execution pointer name must start with 'execution-pointer/' and carry an id: '{name}'"
            )));
        }
        if let Some(base_name) = base {
            if !crate::branch::is_execution_pointer_name(base_name) {
                return Err(CheckpointError::Branch(format!(
                    "base must be an execution pointer name, got '{base_name}'"
                )));
            }
        }
        if self
            .pointer_exists_now(name)
            .map_err(|e| CheckpointError::Branch(e.to_string()))?
        {
            return Err(CheckpointError::Branch(format!(
                "branch '{name}' already exists"
            )));
        }
        if let Some(head) = base.and_then(|b| self.native_pointer_head(b)) {
            self.set_pointer_head(name, &head)
                .map_err(|e| CheckpointError::Branch(e.to_string()))?;
        } else {
            use crate::storage::MetadataStore;
            self.storage
                .store_metadata(&Self::pointer_key(name), "")
                .map_err(|e| CheckpointError::Branch(e.to_string()))?;
        }
        Ok(())
    }

    async fn pointer_exists(&self, name: &str) -> Result<bool, CheckpointError> {
        self.pointer_exists_now(name)
            .map_err(|e| CheckpointError::Branch(e.to_string()))
    }
}
