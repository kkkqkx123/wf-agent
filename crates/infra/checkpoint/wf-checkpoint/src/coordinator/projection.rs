use crate::coordinator::events::publish_best_effort_failed;
use crate::coordinator::queue::{push_persistence_handle, PersistenceQueue};
use checkpoint_base::error::CheckpointError;
use checkpoint_file::event::CheckpointEventBus;
use checkpoint_file::file::FileCheckpointManager;

/// Synchronous best-effort file projection for the entity. Missing file
/// history yields `Ok` so the state checkpoint never fails. Success and
/// failure are logged with the state checkpoint id for correlation.
/// A successful projection records the state-to-file link so restore
/// resolves the exact file set instead of "latest".
pub async fn save_file_snapshot(
    file_checkpoint_manager: Option<&FileCheckpointManager>,
    checkpoint_id: &str,
    entity_id: &str,
) -> Result<(), CheckpointError> {
    let Some(manager) = file_checkpoint_manager else {
        return Ok(());
    };
    let manager_for_task = manager.clone();
    let entity_id_owned = entity_id.to_string();
    let outcome = tokio::task::spawn_blocking(move || {
        manager_for_task.create_latest_file_checkpoint(&entity_id_owned)
    })
    .await
    .map_err(|e| CheckpointError::Internal(format!("file projection task failed: {e}")))??;
    match outcome {
        Some(file_checkpoint) => {
            manager.record_state_file_link(checkpoint_id, &file_checkpoint.id)?;
            tracing::debug!(
                entity_id = %entity_id,
                checkpoint_id = %checkpoint_id,
                file_checkpoint_id = %file_checkpoint.id,
                "file projection correlated with state checkpoint"
            );
        }
        None => {
            tracing::debug!(
                entity_id = %entity_id,
                checkpoint_id = %checkpoint_id,
                "no file history for state checkpoint"
            );
        }
    }
    Ok(())
}

/// Defer the file projection to the background persistence queue. The
/// queue is bounded and shared by both coordinators.
pub async fn enqueue_persistence(
    queue: &PersistenceQueue,
    file_checkpoint_manager: Option<&FileCheckpointManager>,
    bus: Option<&CheckpointEventBus>,
    checkpoint_id: &str,
    entity_id: &str,
) {
    let checkpoint_id = checkpoint_id.to_string();
    let entity_id = entity_id.to_string();
    let file_manager = file_checkpoint_manager.cloned();
    let bus = bus.cloned();
    let bus_for_task = bus.clone();
    let checkpoint_id_for_task = checkpoint_id.clone();
    let entity_id_for_task = entity_id.clone();
    let metrics = file_checkpoint_manager.and_then(|m| m.checkpoint_metrics_for_observability());
    let handle = tokio::task::spawn_blocking(move || {
        if let Some(manager) = file_manager {
            match manager.create_latest_file_checkpoint(&entity_id_for_task) {
                Ok(Some(file_checkpoint)) => {
                    if let Err(err) =
                        manager.record_state_file_link(&checkpoint_id_for_task, &file_checkpoint.id)
                    {
                        tracing::warn!(
                            entity_id = %entity_id_for_task,
                            checkpoint_id = %checkpoint_id_for_task,
                            error = %err,
                            "deferred state-to-file link failed (best-effort)"
                        );
                    }
                    tracing::debug!(
                        entity_id = %entity_id_for_task,
                        checkpoint_id = %checkpoint_id_for_task,
                        file_checkpoint_id = %file_checkpoint.id,
                        "deferred file projection correlated with state checkpoint"
                    );
                }
                Ok(None) => {
                    tracing::debug!(
                        entity_id = %entity_id_for_task,
                        checkpoint_id = %checkpoint_id_for_task,
                        "deferred file projection found no history"
                    );
                }
                Err(err) => {
                    tracing::warn!(
                        entity_id = %entity_id_for_task,
                        checkpoint_id = %checkpoint_id_for_task,
                        error = %err,
                        "deferred file checkpoint creation failed (best-effort)"
                    );
                    publish_best_effort_failed(
                        bus_for_task.as_ref(),
                        Some(checkpoint_id_for_task.clone()),
                        &entity_id_for_task,
                        "async_projection",
                        &format!("deferred file checkpoint creation failed: {err}"),
                    );
                    if let Some(metrics) = metrics.as_ref() {
                        metrics.record_persistence_failure(&entity_id_for_task);
                    }
                }
            }
        }
    });
    let queue_metrics =
        file_checkpoint_manager.and_then(|m| m.checkpoint_metrics_for_observability());
    push_persistence_handle(
        queue,
        handle,
        bus.as_ref(),
        &entity_id,
        &checkpoint_id,
        queue_metrics.as_deref(),
    )
    .await;
}

/// Best-effort restore of the file set linked to a state checkpoint (falls
/// back to the latest file checkpoint for pre-link history). State restore
/// stands regardless of file history; a failure is logged for correlation.
pub fn restore_state_files(
    file_checkpoint_manager: Option<&FileCheckpointManager>,
    entity_id: &str,
    checkpoint_id: &str,
) {
    let Some(manager) = file_checkpoint_manager else {
        return;
    };
    if let Err(err) = manager.restore_state_files(entity_id, checkpoint_id) {
        tracing::warn!(
            checkpoint_id = %checkpoint_id,
            entity_id = %entity_id,
            error = %err,
            "file checkpoint restore failed for state checkpoint; state restore still stands"
        );
    }
}
