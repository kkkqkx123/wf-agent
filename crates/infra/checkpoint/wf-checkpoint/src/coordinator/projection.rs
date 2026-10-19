use crate::coordinator::events::publish_best_effort_failed;
use crate::coordinator::queue::{push_persistence_handle, PersistenceQueue};
use crate::{is_file_channel_closed, ResolvedFileScope};
use checkpoint_base::error::CheckpointError;
use checkpoint_file::event::CheckpointEventBus;
use checkpoint_file::file::{FileCheckpoint, FileCheckpointManager};

/// Total file-projection attempts (one initial try plus retries) before the
/// best-effort snapshot is abandoned.
pub const FILE_PROJECTION_MAX_ATTEMPTS: u32 = 3;

/// Initial retry backoff; doubled after every failed attempt.
const FILE_PROJECTION_INITIAL_BACKOFF_MS: u64 = 100;

/// Project the entity's latest file checkpoint with bounded retries so a
/// transient git/SQLite failure does not leave a state checkpoint without a
/// linked file snapshot. Only errors are retried; a missing history (`None`)
/// is a normal outcome. Runs inside a blocking task.
fn create_latest_file_checkpoint_with_retry(
    manager: &FileCheckpointManager,
    entity_id: &str,
) -> Result<Option<FileCheckpoint>, CheckpointError> {
    let mut attempt = 1;
    let mut backoff_ms = FILE_PROJECTION_INITIAL_BACKOFF_MS;
    loop {
        match manager.create_latest_file_checkpoint(entity_id) {
            Ok(outcome) => return Ok(outcome),
            Err(err) if attempt < FILE_PROJECTION_MAX_ATTEMPTS => {
                tracing::warn!(
                    entity_id = %entity_id,
                    attempt,
                    max_attempts = FILE_PROJECTION_MAX_ATTEMPTS,
                    error = %err,
                    "file projection failed; retrying"
                );
                std::thread::sleep(std::time::Duration::from_millis(backoff_ms));
                attempt += 1;
                backoff_ms *= 2;
            }
            Err(err) => return Err(err),
        }
    }
}

/// Project the entity's latest file snapshot and link it to the state
/// checkpoint. Returns the file checkpoint id when one was created, or
/// `None` when the entity has no file history. Runs inside a blocking task;
/// shared by the synchronous and deferred paths so a change lands once.
fn project_and_link(
    manager: &FileCheckpointManager,
    checkpoint_id: &str,
    entity_id: &str,
) -> Result<Option<String>, CheckpointError> {
    let Some(file_checkpoint) = create_latest_file_checkpoint_with_retry(manager, entity_id)?
    else {
        tracing::debug!(
            entity_id = %entity_id,
            checkpoint_id = %checkpoint_id,
            "no file history for state checkpoint"
        );
        return Ok(None);
    };
    let file_checkpoint_id = file_checkpoint.id;
    record_state_file_link_with_retry(manager, checkpoint_id, &file_checkpoint_id)?;
    tracing::debug!(
        entity_id = %entity_id,
        checkpoint_id = %checkpoint_id,
        file_checkpoint_id = %file_checkpoint_id,
        "file projection correlated with state checkpoint"
    );
    Ok(Some(file_checkpoint_id))
}

/// True when the file projection must be skipped for the scope. Closed
/// channels skip entirely; an explicit empty checkpoint range also skips
/// because there is nothing to project and no link to record.
pub fn should_skip_file_projection(scope: Option<&ResolvedFileScope>) -> bool {
    match scope {
        None => false,
        Some(s) => {
            is_file_channel_closed(Some(s)) || (s.enabled && s.checkpoint_prefixes.is_empty())
        }
    }
}

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
    save_file_snapshot_scoped(file_checkpoint_manager, checkpoint_id, entity_id, None).await
}

pub async fn save_file_snapshot_scoped(
    file_checkpoint_manager: Option<&FileCheckpointManager>,
    checkpoint_id: &str,
    entity_id: &str,
    scope: Option<&ResolvedFileScope>,
) -> Result<(), CheckpointError> {
    if should_skip_file_projection(scope) {
        return Ok(());
    }
    let Some(manager) = file_checkpoint_manager else {
        return Ok(());
    };
    let manager_for_task = manager.clone();
    let entity_id_owned = entity_id.to_string();
    let checkpoint_id_owned = checkpoint_id.to_string();
    tokio::task::spawn_blocking(move || {
        project_and_link(&manager_for_task, &checkpoint_id_owned, &entity_id_owned)
    })
    .await
    .map_err(|e| CheckpointError::Internal(format!("file projection task failed: {e}")))??;
    Ok(())
}

/// Record the state-to-file link with bounded retries. When every attempt
/// fails, a pending marker is persisted (best-effort) so startup compensation
/// can re-link, and the original error is returned for loud reporting.
pub fn record_state_file_link_with_retry(
    manager: &FileCheckpointManager,
    checkpoint_id: &str,
    file_commit_id: &str,
) -> Result<(), CheckpointError> {
    let mut attempt = 1;
    let mut backoff_ms = FILE_PROJECTION_INITIAL_BACKOFF_MS;
    loop {
        match manager.record_state_file_link(checkpoint_id, file_commit_id) {
            Ok(()) => return Ok(()),
            Err(err) if attempt < FILE_PROJECTION_MAX_ATTEMPTS => {
                tracing::warn!(
                    checkpoint_id = %checkpoint_id,
                    attempt,
                    max_attempts = FILE_PROJECTION_MAX_ATTEMPTS,
                    error = %err,
                    "state-file link write failed; retrying"
                );
                std::thread::sleep(std::time::Duration::from_millis(backoff_ms));
                attempt += 1;
                backoff_ms *= 2;
            }
            Err(err) => {
                if let Err(mark_err) =
                    manager.record_pending_state_file_link(checkpoint_id, file_commit_id)
                {
                    tracing::warn!(
                        checkpoint_id = %checkpoint_id,
                        error = %mark_err,
                        "failed to record pending state-file link for compensation"
                    );
                }
                return Err(err);
            }
        }
    }
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
    enqueue_persistence_scoped(
        queue,
        file_checkpoint_manager,
        bus,
        checkpoint_id,
        entity_id,
        None,
    )
    .await
}

pub async fn enqueue_persistence_scoped(
    queue: &PersistenceQueue,
    file_checkpoint_manager: Option<&FileCheckpointManager>,
    bus: Option<&CheckpointEventBus>,
    checkpoint_id: &str,
    entity_id: &str,
    scope: Option<&ResolvedFileScope>,
) {
    if should_skip_file_projection(scope) {
        return;
    }
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
            match project_and_link(&manager, &checkpoint_id_for_task, &entity_id_for_task) {
                Ok(_) => {}
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
    match manager.restore_state_files(entity_id, checkpoint_id) {
        Ok(Some(_)) => {}
        Ok(None) => {
            // No link means the state checkpoint was created without file
            // history (or its file projection failed). The state restore is
            // still valid; report at debug so the absence stays visible
            // without flagging the common "no file history" case as a fault.
            tracing::debug!(
                checkpoint_id = %checkpoint_id,
                entity_id = %entity_id,
                "state checkpoint has no linked file snapshot; file state not restored"
            );
        }
        Err(err) => {
            tracing::warn!(
                checkpoint_id = %checkpoint_id,
                entity_id = %entity_id,
                error = %err,
                "file checkpoint restore failed for state checkpoint; state restore still stands"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file::StateFileLinkStatus;

    fn scope(enabled: bool, checkpoint_prefixes: &[&str]) -> ResolvedFileScope {
        ResolvedFileScope {
            enabled,
            read_prefixes: vec![".".to_string()],
            write_prefixes: vec![".".to_string()],
            checkpoint_prefixes: checkpoint_prefixes.iter().map(|s| s.to_string()).collect(),
            workdir: None,
        }
    }

    #[test]
    fn should_skip_file_projection_rules() {
        // No scope: legacy global behavior, nothing to skip.
        assert!(!should_skip_file_projection(None));
        // A disabled channel skips entirely.
        assert!(should_skip_file_projection(Some(&scope(false, &["src"]))));
        // Enabled but no checkpoint prefix: nothing to project, no link.
        assert!(should_skip_file_projection(Some(&scope(true, &[]))));
        // Enabled with a checkpoint prefix: project.
        assert!(!should_skip_file_projection(Some(&scope(true, &["src"]))));
    }

    #[tokio::test]
    async fn save_file_snapshot_without_manager_or_history_is_ok() {
        // No file channel at all: Ok with nothing to record.
        assert!(save_file_snapshot(None, "c1", "e1").await.is_ok());

        // A channel without any file history for the entity: Ok, Missing.
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        save_file_snapshot(Some(&manager), "c1", "no-history")
            .await
            .unwrap();
        assert_eq!(
            manager.state_file_link_status("c1").unwrap(),
            StateFileLinkStatus::Missing
        );
    }

    #[tokio::test]
    async fn save_file_snapshot_records_link_when_history_exists() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        let actor = manager.actor_id_for("loop-1");
        let head = manager.apply_agent_edit(&actor, "a.txt", b"one\n").unwrap();

        save_file_snapshot(Some(&manager), "c1", "loop-1")
            .await
            .unwrap();
        assert_eq!(
            manager.state_file_link_status("c1").unwrap(),
            StateFileLinkStatus::Linked {
                file_commit_id: head
            }
        );
    }

    #[tokio::test]
    async fn save_file_snapshot_skipped_for_closed_channel() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        let actor = manager.actor_id_for("loop-1");
        manager.apply_agent_edit(&actor, "a.txt", b"one\n").unwrap();

        save_file_snapshot_scoped(
            Some(&manager),
            "c1",
            "loop-1",
            Some(&scope(false, &["src"])),
        )
        .await
        .unwrap();
        assert_eq!(
            manager.state_file_link_status("c1").unwrap(),
            StateFileLinkStatus::Missing
        );
    }

    #[test]
    fn record_state_file_link_with_retry_records_on_first_attempt() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        record_state_file_link_with_retry(&manager, "c1", "f1").unwrap();
        assert_eq!(
            manager.state_file_link_status("c1").unwrap(),
            StateFileLinkStatus::Linked {
                file_commit_id: "f1".to_string()
            }
        );
    }
}
