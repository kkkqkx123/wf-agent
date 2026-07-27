use async_trait::async_trait;

#[async_trait]
pub trait StateManager<TSnapshot>: Send + Sync {
    async fn cleanup(&mut self) -> Result<(), crate::error::ExecutionSharedError>;
    async fn create_snapshot(&self) -> Result<TSnapshot, crate::error::ExecutionSharedError>;
    async fn restore_from_snapshot(&mut self, snapshot: TSnapshot) -> Result<(), crate::error::ExecutionSharedError>;
    fn size(&self) -> usize;
    fn is_empty(&self) -> bool;
}
