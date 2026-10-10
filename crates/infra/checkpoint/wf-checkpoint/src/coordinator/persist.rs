use crate::coordinator::base::{CheckpointBlob, CheckpointId};
use crate::coordinator::events::{publish_persist_failed, publish_persisted};
use checkpoint_base::error::CheckpointError;
use checkpoint_base::error_handling::CheckpointErrorHandler;
use checkpoint_file::event::CheckpointEventBus;
use checkpoint_state::state::CheckpointStateManager;

/// Persist one checkpoint row, publishing the outcome on the shared bus.
/// A failed write is routed through the checkpoint error handler: the default
/// handler surfaces the failure to the caller so a failed write never
/// masquerades as a saved checkpoint. Only an explicitly lenient handler
/// lets the execution continue without one.
pub async fn persist_checkpoint<M>(
    manager: &M,
    checkpoint: &M::Checkpoint,
    entity_type: &str,
    entity_id: &str,
    bus: Option<&CheckpointEventBus>,
    error_handler: &CheckpointErrorHandler,
) -> Result<(), CheckpointError>
where
    M: CheckpointStateManager,
    M::Checkpoint: CheckpointId + CheckpointBlob,
{
    let checkpoint_id = checkpoint.checkpoint_id().to_string();
    if let Err(err) = manager.save(checkpoint, entity_type, entity_id).await {
        publish_persist_failed(bus, Some(checkpoint_id.clone()), entity_id, &err);
        let context = error_handler.context("create", Some(checkpoint_id), None);
        let outcome = error_handler.decide(&context, &err);
        if outcome.should_rethrow {
            return Err(err);
        }
        return Ok(());
    }

    let description = checkpoint.blob_description();
    publish_persisted(bus, &checkpoint_id, entity_id, description);
    Ok(())
}

/// Delete one checkpoint row, publishing the deletion when the row existed.
pub async fn delete_checkpoint<M>(
    manager: &M,
    checkpoint_id: &str,
    bus: Option<&CheckpointEventBus>,
) -> Result<bool, CheckpointError>
where
    M: CheckpointStateManager,
{
    let deleted = manager.delete(checkpoint_id).await?;
    if deleted {
        if let Some(bus) = bus {
            bus.publish(CheckpointEventBus::deleted_with(
                checkpoint_id.to_string(),
                Some("delete".to_string()),
            ));
        }
    }
    Ok(deleted)
}
