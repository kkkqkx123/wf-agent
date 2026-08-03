use crate::error::CheckpointError;
use wf_types::checkpoint::{CheckpointContext, CheckpointTrigger, DeltaStorageConfig};

pub trait CheckpointCoordinator: Send + Sync {
    type Checkpoint: Send + Sync;
    type Entity: Send + Sync;
    type State: Send + Sync;

    fn prepare(
        &self,
        entity_id: &str,
        trigger: CheckpointTrigger,
    ) -> impl std::future::Future<Output = Result<CheckpointContext, CheckpointError>> + Send;

    fn build(
        &self,
        ctx: CheckpointContext,
        state: Self::State,
    ) -> impl std::future::Future<Output = Result<Self::Checkpoint, CheckpointError>> + Send;

    fn persist(
        &self,
        checkpoint: &Self::Checkpoint,
        entity_id: &str,
    ) -> impl std::future::Future<Output = Result<(), CheckpointError>> + Send;

    fn restore(
        &self,
        checkpoint_id: &str,
    ) -> impl std::future::Future<Output = Result<Self::Entity, CheckpointError>> + Send;

    fn delete(
        &self,
        checkpoint_id: &str,
    ) -> impl std::future::Future<Output = Result<bool, CheckpointError>> + Send;

    fn determine_type(
        &self,
        entity_id: &str,
        config: &DeltaStorageConfig,
    ) -> impl std::future::Future<
        Output = Result<wf_types::checkpoint::CheckpointType, CheckpointError>,
    > + Send;
}
