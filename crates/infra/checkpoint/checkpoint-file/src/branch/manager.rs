use checkpoint_base::error::CheckpointError;
use dashmap::DashMap;

pub trait BranchStorageAdapter: Send + Sync {
    fn create_branch(
        &self,
        name: &str,
        base: Option<&str>,
    ) -> impl std::future::Future<Output = Result<(), CheckpointError>> + Send;
    fn delete_branch(
        &self,
        name: &str,
    ) -> impl std::future::Future<Output = Result<(), CheckpointError>> + Send;
    fn list_branches(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<String>, CheckpointError>> + Send;
    fn branch_exists(
        &self,
        name: &str,
    ) -> impl std::future::Future<Output = Result<bool, CheckpointError>> + Send;

    /// Reassign the history of `source` into `target` at the storage level.
    /// The default implementation is a no-op: generic adapters without merge
    /// semantics treat the manager-level bookkeeping (base relationship) as
    /// the merge result.
    fn merge_execution_history(
        &self,
        source: &str,
        target: &str,
    ) -> impl std::future::Future<Output = Result<(), CheckpointError>> + Send {
        async move {
            let _ = (source, target);
            Ok(())
        }
    }
}

pub trait BranchManager: Send + Sync {
    fn create_branch(
        &self,
        branch_name: &str,
        base_branch: Option<&str>,
    ) -> impl std::future::Future<Output = Result<(), CheckpointError>> + Send;
    fn switch_branch(
        &self,
        branch_name: &str,
    ) -> impl std::future::Future<Output = Result<(), CheckpointError>> + Send;
    fn merge_execution_branch(
        &self,
        source: &str,
        target: &str,
    ) -> impl std::future::Future<Output = Result<(), CheckpointError>> + Send;
    fn delete_branch(
        &self,
        branch_name: &str,
    ) -> impl std::future::Future<Output = Result<(), CheckpointError>> + Send;
    fn list_branches(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<String>, CheckpointError>> + Send;
    fn current_branch(
        &self,
    ) -> impl std::future::Future<Output = Result<String, CheckpointError>> + Send;
}

#[derive(Debug, Clone)]
pub struct BranchInfo {
    pub name: String,
    pub created_at: i64,
    pub base_branch: Option<String>,
}

pub struct ExecutionBranchManager<S: BranchStorageAdapter> {
    storage: S,
    current: tokio::sync::RwLock<String>,
    cache: DashMap<String, BranchInfo>,
}

fn ensure_execution_name(name: &str) -> Result<(), CheckpointError> {
    if crate::branch::is_execution_branch_name(name) {
        Ok(())
    } else {
        Err(CheckpointError::Branch(format!(
            "execution branch name must start with 'execution/' and carry an id: '{name}'"
        )))
    }
}

impl<S: BranchStorageAdapter> ExecutionBranchManager<S> {
    pub fn new(storage: S, default_branch: impl Into<String>) -> Self {
        Self {
            storage,
            current: tokio::sync::RwLock::new(default_branch.into()),
            cache: DashMap::new(),
        }
    }
}

impl<S: BranchStorageAdapter> BranchManager for ExecutionBranchManager<S> {
    async fn create_branch(
        &self,
        branch_name: &str,
        base_branch: Option<&str>,
    ) -> Result<(), CheckpointError> {
        ensure_execution_name(branch_name)?;
        if let Some(base) = base_branch {
            ensure_execution_name(base)?;
        }
        self.storage.create_branch(branch_name, base_branch).await?;
        self.cache.insert(
            branch_name.to_string(),
            BranchInfo {
                name: branch_name.to_string(),
                created_at: chrono::Utc::now().timestamp_millis(),
                base_branch: base_branch.map(String::from),
            },
        );
        Ok(())
    }

    async fn switch_branch(&self, branch_name: &str) -> Result<(), CheckpointError> {
        ensure_execution_name(branch_name)?;
        let exists = self.storage.branch_exists(branch_name).await?;
        if !exists {
            return Err(CheckpointError::Branch(format!(
                "branch '{}' does not exist",
                branch_name
            )));
        }
        let mut current = self.current.write().await;
        *current = branch_name.to_string();
        Ok(())
    }

    /// Merge execution branch `source` into `target`.
    ///
    /// Validates that both branches exist and are distinct, then delegates the
    /// storage-level history reassignment to the adapter and records the merge
    /// relationship in the cache (the target's base branch becomes `source`).
    /// This never merges file contents; file content merges live on the
    /// commit DAG entry points.
    async fn merge_execution_branch(
        &self,
        source: &str,
        target: &str,
    ) -> Result<(), CheckpointError> {
        ensure_execution_name(source)?;
        ensure_execution_name(target)?;
        if source == target {
            return Err(CheckpointError::Branch(format!(
                "cannot merge branch '{}' into itself",
                source
            )));
        }
        if !self.storage.branch_exists(source).await? {
            return Err(CheckpointError::Branch(format!(
                "source branch '{}' does not exist",
                source
            )));
        }
        if !self.storage.branch_exists(target).await? {
            return Err(CheckpointError::Branch(format!(
                "target branch '{}' does not exist",
                target
            )));
        }

        self.storage.merge_execution_history(source, target).await?;

        if let Some(mut info) = self.cache.get_mut(target) {
            info.base_branch = Some(source.to_string());
        } else {
            self.cache.insert(
                target.to_string(),
                BranchInfo {
                    name: target.to_string(),
                    created_at: chrono::Utc::now().timestamp_millis(),
                    base_branch: Some(source.to_string()),
                },
            );
        }

        Ok(())
    }

    async fn delete_branch(&self, branch_name: &str) -> Result<(), CheckpointError> {
        ensure_execution_name(branch_name)?;
        self.storage.delete_branch(branch_name).await?;
        self.cache.remove(branch_name);
        Ok(())
    }

    async fn list_branches(&self) -> Result<Vec<String>, CheckpointError> {
        self.storage.list_branches().await
    }

    async fn current_branch(&self) -> Result<String, CheckpointError> {
        Ok(self.current.read().await.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct MemoryBranchStorage {
        branches: tokio::sync::RwLock<Vec<String>>,
    }

    impl BranchStorageAdapter for MemoryBranchStorage {
        async fn create_branch(
            &self,
            name: &str,
            _base: Option<&str>,
        ) -> Result<(), CheckpointError> {
            let mut branches = self.branches.write().await;
            if branches.iter().any(|b| b == name) {
                return Err(CheckpointError::Branch(format!(
                    "branch '{}' already exists",
                    name
                )));
            }
            branches.push(name.to_string());
            Ok(())
        }

        async fn delete_branch(&self, name: &str) -> Result<(), CheckpointError> {
            let mut branches = self.branches.write().await;
            branches.retain(|b| b != name);
            Ok(())
        }

        async fn list_branches(&self) -> Result<Vec<String>, CheckpointError> {
            Ok(self.branches.read().await.clone())
        }

        async fn branch_exists(&self, name: &str) -> Result<bool, CheckpointError> {
            Ok(self.branches.read().await.iter().any(|b| b == name))
        }
    }

    fn make_manager() -> ExecutionBranchManager<MemoryBranchStorage> {
        ExecutionBranchManager::new(
            MemoryBranchStorage {
                branches: tokio::sync::RwLock::new(vec!["execution/main".to_string()]),
            },
            "execution/main",
        )
    }

    #[tokio::test]
    async fn create_and_list_branches() {
        let manager = make_manager();
        manager
            .create_branch("execution/feature", Some("execution/main"))
            .await
            .unwrap();

        let mut branches = manager.list_branches().await.unwrap();
        branches.sort();
        assert_eq!(branches, vec!["execution/feature", "execution/main"]);
    }

    #[tokio::test]
    async fn switch_branch_requires_existence() {
        let manager = make_manager();
        assert!(manager.switch_branch("execution/missing").await.is_err());
    }

    #[tokio::test]
    async fn bare_names_rejected() {
        let manager = make_manager();
        assert!(manager.create_branch("main", None).await.is_err());
        assert!(manager.switch_branch("main").await.is_err());
        assert!(manager.delete_branch("main").await.is_err());
        assert!(manager
            .merge_execution_branch("execution/main", "main")
            .await
            .is_err());
    }

    #[tokio::test]
    async fn merge_execution_branch_links_base_relationship() {
        let manager = make_manager();
        manager
            .create_branch("execution/feature", Some("execution/main"))
            .await
            .unwrap();

        manager
            .merge_execution_branch("execution/feature", "execution/main")
            .await
            .unwrap();

        let cached = manager.cache.get("execution/main").unwrap();
        assert_eq!(cached.base_branch.as_deref(), Some("execution/feature"));
    }

    #[tokio::test]
    async fn merge_self_rejected() {
        let manager = make_manager();
        let err = manager
            .merge_execution_branch("execution/main", "execution/main")
            .await
            .unwrap_err();
        assert!(matches!(err, CheckpointError::Branch(_)));
    }

    #[tokio::test]
    async fn merge_missing_source_rejected() {
        let manager = make_manager();
        let err = manager
            .merge_execution_branch("execution/nope", "execution/main")
            .await
            .unwrap_err();
        assert!(matches!(err, CheckpointError::Branch(_)));
    }

    #[tokio::test]
    async fn delete_branch_removes() {
        let manager = make_manager();
        manager
            .create_branch("execution/temp", Some("execution/main"))
            .await
            .unwrap();
        manager.delete_branch("execution/temp").await.unwrap();
        assert!(!manager
            .list_branches()
            .await
            .unwrap()
            .contains(&"execution/temp".to_string()));
    }
}
