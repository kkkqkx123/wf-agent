use crate::coordinator::events::{publish_best_effort_failed, publish_cleanup_skipped};
use checkpoint_file::event::CheckpointEventBus;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

/// Background persistence queue holding deferred file-projection tasks.
pub type PersistenceQueue = Arc<Mutex<Vec<JoinHandle<()>>>>;

/// Upper bound on deferred persistence queues shared by both coordinators.
pub const MAX_PERSISTENCE_QUEUE: usize = 128;

/// Push a background persistence handle, awaiting the backlog first when
/// the queue is full so memory stays bounded. Shared by both coordinators.
pub async fn push_persistence_handle(
    queue: &PersistenceQueue,
    handle: JoinHandle<()>,
    bus: Option<&CheckpointEventBus>,
    entity_id: &str,
    checkpoint_id: &str,
    metrics: Option<&wf_metrics::CheckpointMetricsCollector>,
) {
    let mut guard = queue.lock().await;
    if guard.len() >= MAX_PERSISTENCE_QUEUE {
        publish_cleanup_skipped(
            bus,
            Some(checkpoint_id.to_string()),
            entity_id,
            "persistence_backlog",
            "persistence queue full; awaiting backlog",
        );
        if let Some(metrics) = metrics {
            metrics.record_persistence_backlog(entity_id);
        }
        let backlog: Vec<_> = std::mem::take(&mut *guard);
        drop(guard);
        for task in backlog {
            if let Err(join_err) = task.await {
                tracing::warn!(error = %join_err, "persistence task panicked");
                publish_best_effort_failed(
                    bus,
                    Some(checkpoint_id.to_string()),
                    entity_id,
                    "persistence_failure",
                    &format!("persistence task panicked: {join_err}"),
                );
                if let Some(metrics) = metrics {
                    metrics.record_persistence_failure(entity_id);
                }
            }
        }
        guard = queue.lock().await;
    }
    guard.push(handle);
}

/// Drain all deferred persistence handles. Shared by both coordinators.
pub async fn drain_persistence_handles(
    queue: &PersistenceQueue,
    bus: Option<&CheckpointEventBus>,
    entity_id: &str,
    metrics: Option<&wf_metrics::CheckpointMetricsCollector>,
) {
    let handles: Vec<_> = {
        let mut guard = queue.lock().await;
        std::mem::take(&mut *guard)
    };
    for handle in handles {
        if let Err(join_err) = handle.await {
            tracing::warn!(error = %join_err, "persistence task panicked");
            publish_best_effort_failed(
                bus,
                None,
                entity_id,
                "persistence_failure",
                &format!("persistence task panicked: {join_err}"),
            );
            if let Some(metrics) = metrics {
                metrics.record_persistence_failure(entity_id);
            }
        }
    }
}
