use async_trait::async_trait;

use wf_types::Id;

use crate::error::ExecutionSharedError;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ExecutionStatus {
    Created,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
    Stopped,
    Timeout,
}

#[async_trait]
pub trait IExecutionEntity: Send + Sync {
    fn id(&self) -> &Id;
    fn status(&self) -> ExecutionStatus;
    fn is_running(&self) -> bool;
    fn is_paused(&self) -> bool;
    fn is_completed(&self) -> bool;
    fn is_failed(&self) -> bool;
    fn is_cancelled(&self) -> bool;

    async fn pause(&self) -> Result<(), ExecutionSharedError>;
    async fn resume(&self) -> Result<(), ExecutionSharedError>;
    async fn stop(&self) -> Result<(), ExecutionSharedError>;
    async fn abort(&self);

    fn get_abort_signal(&self) -> tokio_util::sync::CancellationToken;
    fn get_hierarchy_depth(&self) -> u32;
    fn get_root_execution_id(&self) -> Option<Id>;
}

