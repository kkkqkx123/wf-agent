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

#[cfg(test)]
mod tests {
    use super::*;
    use checkpoint_file::event::CheckpointEvent;
    use wf_metrics::collector::CollectorConfig;
    use wf_metrics::CheckpointMetricsCollector;

    fn collector() -> CheckpointMetricsCollector {
        CheckpointMetricsCollector::new(CollectorConfig::default())
    }

    #[tokio::test]
    async fn push_below_capacity_and_drain() {
        let queue: PersistenceQueue = Arc::new(Mutex::new(Vec::new()));
        let bus = CheckpointEventBus::new();
        let mut rx = bus.subscribe();

        for _ in 0..3 {
            let handle = tokio::spawn(async {});
            push_persistence_handle(&queue, handle, Some(&bus), "e1", "c1", None).await;
        }
        assert_eq!(queue.lock().await.len(), 3);

        drain_persistence_handles(&queue, Some(&bus), "e1", None).await;
        assert!(queue.lock().await.is_empty());
        // Below capacity nothing is skipped or fails.
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn push_at_capacity_drains_backlog_before_enqueue() {
        let queue: PersistenceQueue = Arc::new(Mutex::new(Vec::new()));
        let bus = CheckpointEventBus::new();
        let mut rx = bus.subscribe();
        let metrics = collector();

        for _ in 0..MAX_PERSISTENCE_QUEUE {
            let handle = tokio::spawn(async {});
            push_persistence_handle(&queue, handle, None, "e1", "c0", None).await;
        }
        assert_eq!(queue.lock().await.len(), MAX_PERSISTENCE_QUEUE);

        let handle = tokio::spawn(async {});
        push_persistence_handle(&queue, handle, Some(&bus), "e1", "c-new", Some(&metrics)).await;

        // The backlog was awaited; only the new handle stays queued.
        assert_eq!(queue.lock().await.len(), 1);

        // The skip is observable as a Skipped event, not a failure.
        let event = rx.recv().await.expect("skip event");
        match event {
            CheckpointEvent::Skipped { data, .. } => {
                assert_eq!(data.operation.as_deref(), Some("persistence_backlog"));
                assert_eq!(
                    data.reason.as_deref(),
                    Some("persistence queue full; awaiting backlog")
                );
                assert_eq!(data.checkpoint_id.as_deref(), Some("c-new"));
            }
            other => panic!("expected Skipped, got {other:?}"),
        }
        assert_eq!(metrics.usage_stats().persistence_backlog, 1);
    }

    #[tokio::test]
    async fn drain_reports_panicked_tasks() {
        let queue: PersistenceQueue = Arc::new(Mutex::new(Vec::new()));
        let bus = CheckpointEventBus::new();
        let mut rx = bus.subscribe();
        let metrics = collector();

        let good = tokio::spawn(async {});
        let bad = tokio::spawn(async { panic!("boom") });
        push_persistence_handle(&queue, good, None, "e1", "c1", None).await;
        push_persistence_handle(&queue, bad, None, "e1", "c2", None).await;

        drain_persistence_handles(&queue, Some(&bus), "e1", Some(&metrics)).await;
        assert!(queue.lock().await.is_empty());

        let event = rx.recv().await.expect("failure event");
        match event {
            CheckpointEvent::Failed { data, .. } => {
                assert_eq!(data.operation.as_deref(), Some("persistence_failure"));
                assert!(data.error.as_deref().expect("error text").contains("panicked"));
            }
            other => panic!("expected Failed, got {other:?}"),
        }
        assert_eq!(metrics.usage_stats().persistence_failures, 1);
    }

    #[tokio::test]
    async fn full_queue_backlog_panic_is_reported_as_failure() {
        let queue: PersistenceQueue = Arc::new(Mutex::new(Vec::new()));
        let bus = CheckpointEventBus::new();
        let mut rx = bus.subscribe();
        let metrics = collector();

        for i in 0..MAX_PERSISTENCE_QUEUE {
            let handle = if i == 0 {
                tokio::spawn(async { panic!("backlog boom") })
            } else {
                tokio::spawn(async {})
            };
            push_persistence_handle(&queue, handle, None, "e1", &format!("c{i}"), None).await;
        }

        push_persistence_handle(
            &queue,
            tokio::spawn(async {}),
            Some(&bus),
            "e1",
            "c-new",
            Some(&metrics),
        )
        .await;

        // The skip lands first; the backlog panic surfaces as a failure.
        let mut saw_skip = false;
        let mut saw_failure = false;
        for _ in 0..2 {
            match rx.recv().await.expect("event") {
                CheckpointEvent::Skipped { data, .. }
                    if data.operation.as_deref() == Some("persistence_backlog") =>
                {
                    saw_skip = true
                }
                CheckpointEvent::Failed { data, .. }
                    if data.operation.as_deref() == Some("persistence_failure") =>
                {
                    saw_failure = true
                }
                other => panic!("unexpected {other:?}"),
            }
        }
        assert!(saw_skip && saw_failure);
        assert_eq!(metrics.usage_stats().persistence_backlog, 1);
        assert_eq!(metrics.usage_stats().persistence_failures, 1);
        assert_eq!(queue.lock().await.len(), 1);
    }
}
