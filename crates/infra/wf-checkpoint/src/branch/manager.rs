use crate::error::CheckpointError;
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

    /// Merge the history of `source` into `target` at the storage level.
    /// The default implementation is a no-op: generic adapters without merge
    /// semantics treat the manager-level bookkeeping (base relationship) as
    /// the merge result.
    fn merge_branch(
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
    fn merge_branch(
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

    /// Merge `source` into `target`.
    ///
    /// Validates that both branches exist and are distinct, then delegates the
    /// storage-level merge to the adapter and records the merge relationship
    /// in the cache (the target's base branch becomes `source`).
    async fn merge_branch(&self, source: &str, target: &str) -> Result<(), CheckpointError> {
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

        self.storage.merge_branch(source, target).await?;

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
        // The default branch ("main") is expected to pre-exist.
        ExecutionBranchManager::new(
            MemoryBranchStorage {
                branches: tokio::sync::RwLock::new(vec!["main".to_string()]),
            },
            "main",
        )
    }

    #[tokio::test]
    async fn create_and_list_branches() {
        let manager = make_manager();
        manager
            .create_branch("feature", Some("main"))
            .await
            .unwrap();

        let mut branches = manager.list_branches().await.unwrap();
        branches.sort();
        assert_eq!(branches, vec!["feature", "main"]);
    }

    #[tokio::test]
    async fn switch_branch_requires_existence() {
        let manager = make_manager();
        assert!(manager.switch_branch("missing").await.is_err());
    }

    #[tokio::test]
    async fn merge_branch_links_base_relationship() {
        let manager = make_manager();
        manager
            .create_branch("feature", Some("main"))
            .await
            .unwrap();

        manager.merge_branch("feature", "main").await.unwrap();

        let cached = manager.cache.get("main").unwrap();
        assert_eq!(cached.base_branch.as_deref(), Some("feature"));
    }

    #[tokio::test]
    async fn merge_self_rejected() {
        let manager = make_manager();
        let err = manager.merge_branch("main", "main").await.unwrap_err();
        assert!(matches!(err, CheckpointError::Branch(_)));
    }

    #[tokio::test]
    async fn merge_missing_source_rejected() {
        let manager = make_manager();
        let err = manager.merge_branch("nope", "main").await.unwrap_err();
        assert!(matches!(err, CheckpointError::Branch(_)));
    }

    #[tokio::test]
    async fn delete_branch_removes() {
        let manager = make_manager();
        manager.create_branch("temp", Some("main")).await.unwrap();
        manager.delete_branch("temp").await.unwrap();
        assert!(!manager
            .list_branches()
            .await
            .unwrap()
            .contains(&"temp".to_string()));
    }
}
