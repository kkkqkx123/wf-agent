use crate::error::CheckpointError;
use async_trait::async_trait;
use dashmap::DashMap;

#[async_trait]
pub trait BranchStorageAdapter: Send + Sync {
    async fn create_branch(&self, name: &str, base: Option<&str>) -> Result<(), CheckpointError>;
    async fn delete_branch(&self, name: &str) -> Result<(), CheckpointError>;
    async fn list_branches(&self) -> Result<Vec<String>, CheckpointError>;
    async fn branch_exists(&self, name: &str) -> Result<bool, CheckpointError>;
}

#[async_trait]
pub trait BranchManager: Send + Sync {
    async fn create_branch(&self, branch_name: &str, base_branch: Option<&str>) -> Result<(), CheckpointError>;
    async fn switch_branch(&self, branch_name: &str) -> Result<(), CheckpointError>;
    async fn merge_branch(&self, source: &str, target: &str) -> Result<(), CheckpointError>;
    async fn delete_branch(&self, branch_name: &str) -> Result<(), CheckpointError>;
    async fn list_branches(&self) -> Result<Vec<String>, CheckpointError>;
    async fn current_branch(&self) -> Result<String, CheckpointError>;
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

#[async_trait]
impl<S: BranchStorageAdapter> BranchManager for ExecutionBranchManager<S> {
    async fn create_branch(&self, branch_name: &str, base_branch: Option<&str>) -> Result<(), CheckpointError> {
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

    async fn merge_branch(&self, source: &str, target: &str) -> Result<(), CheckpointError> {
        let _ = (source, target);
        Err(CheckpointError::Branch(
            "merge not yet implemented".to_string(),
        ))
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
