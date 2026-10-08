use checkpoint_base::error::CheckpointError;

pub trait BranchStorageAdapter: Send + Sync {
    fn create_branch(
        &self,
        name: &str,
        base: Option<&str>,
    ) -> impl std::future::Future<Output = Result<(), CheckpointError>> + Send;
    fn branch_exists(
        &self,
        name: &str,
    ) -> impl std::future::Future<Output = Result<bool, CheckpointError>> + Send;
}
