use crate::error::CheckpointError;
use async_trait::async_trait;
use wf_types::checkpoint::{CheckpointContext, CheckpointTrigger, DeltaStorageConfig};

#[async_trait]
pub trait CheckpointCoordinator: Send + Sync {
    type Checkpoint: Send + Sync;
    type Entity: Send + Sync;
    type State: Send + Sync;

    async fn prepare(
        &self,
        entity_id: &str,
        trigger: CheckpointTrigger,
    ) -> Result<CheckpointContext, CheckpointError>;

    async fn build(
        &self,
        ctx: CheckpointContext,
        state: Self::State,
    ) -> Result<Self::Checkpoint, CheckpointError>;

    async fn persist(&self, checkpoint: &Self::Checkpoint) -> Result<(), CheckpointError>;

    async fn restore(&self, checkpoint_id: &str) -> Result<Self::Entity, CheckpointError>;

    async fn determine_type(
        &self,
        entity_id: &str,
        config: &DeltaStorageConfig,
    ) -> Result<wf_types::checkpoint::CheckpointType, CheckpointError>;
}
