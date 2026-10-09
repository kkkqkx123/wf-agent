use checkpoint_base::error::CheckpointError;

pub trait ExecutionPointerAdapter: Send + Sync {
    fn create_pointer(
        &self,
        name: &str,
        base: Option<&str>,
    ) -> impl std::future::Future<Output = Result<(), CheckpointError>> + Send;
    fn pointer_exists(
        &self,
        name: &str,
    ) -> impl std::future::Future<Output = Result<bool, CheckpointError>> + Send;
}
