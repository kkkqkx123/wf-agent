use std::future::Future;

pub trait StateManager<TSnapshot>: Send + Sync {
    fn cleanup(&mut self) -> impl Future<Output = Result<(), crate::error::ExecutionSharedError>> + Send;
    fn create_snapshot(&self) -> impl Future<Output = Result<TSnapshot, crate::error::ExecutionSharedError>> + Send;
    fn restore_from_snapshot(&mut self, snapshot: TSnapshot) -> impl Future<Output = Result<(), crate::error::ExecutionSharedError>> + Send;
    fn size(&self) -> usize;
    fn is_empty(&self) -> bool;
}
