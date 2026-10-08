use std::sync::Arc;

use crate::branch::BranchStorageAdapter;
use crate::storage::SqliteStorage;
use checkpoint_base::error::CheckpointError;

/// Storage adapter for execution branch heads.
/// Execution branch heads live in `meta_kv` under `exec_branch/`.
/// File branches (review/feature/main) live as Git refs.
/// A branch created without a resolvable base head starts headless
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

/// Execution branch heads: `meta_kv` rows under `exec_branch/` only.
/// File branches (review/feature/main) live as Git refs; execution branches
/// keep a lightweight pointer here.
impl SqliteBackend {
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

    async fn branch_exists(&self, name: &str) -> Result<bool, CheckpointError> {
        self.branch_exists_now(name)
            .map_err(|e| CheckpointError::Branch(e.to_string()))
    }
}
