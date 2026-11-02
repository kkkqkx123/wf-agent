use checkpoint_file::event::CheckpointEventBus;

/// Shared persist event publishing so agent and workflow report identically.
pub fn publish_persisted(
    bus: Option<&CheckpointEventBus>,
    checkpoint_id: &str,
    entity_id: &str,
    description: Option<&str>,
) {
    if let Some(bus) = bus {
        bus.publish(CheckpointEventBus::created_with(
            checkpoint_id.to_string(),
            Some(entity_id.to_string()),
            description.map(String::from),
        ));
    }
}

/// Shared persist-failure event publishing.
pub fn publish_persist_failed(
    bus: Option<&CheckpointEventBus>,
    checkpoint_id: Option<String>,
    entity_id: &str,
    err: &checkpoint_base::error::CheckpointError,
) {
    if let Some(bus) = bus {
        bus.publish(CheckpointEventBus::failed_with(
            checkpoint_id,
            "create",
            format!("persist failed: {}", err),
            Some(entity_id.to_string()),
        ));
    }
}

/// Shared best-effort failure publishing reusing the Failed shape so
/// async projection and persistence failures stay queryable. Expected races
/// (cleanup contention, duplicate merge-back) must use `publish_cleanup_skipped`
/// instead so failure dashboards stay clean.
pub fn publish_best_effort_failed(
    bus: Option<&CheckpointEventBus>,
    checkpoint_id: Option<String>,
    entity_id: &str,
    operation: &str,
    err: &str,
) {
    if let Some(bus) = bus {
        bus.publish(CheckpointEventBus::failed_with(
            checkpoint_id,
            operation,
            err,
            Some(entity_id.to_string()),
        ));
    }
}

/// Publish an expected skip (cleanup race, duplicate merge-back, queue
/// backlog wait) as a `Skipped` event instead of `Failed`.
pub fn publish_cleanup_skipped(
    bus: Option<&CheckpointEventBus>,
    checkpoint_id: Option<String>,
    entity_id: &str,
    operation: &str,
    reason: &str,
) {
    if let Some(bus) = bus {
        bus.publish(CheckpointEventBus::skipped(
            operation,
            reason,
            checkpoint_id,
        ));
        let _ = entity_id;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use checkpoint_file::event::CheckpointEvent;

    #[test]
    fn missing_bus_is_a_silent_noop() {
        publish_persisted(None, "cp-1", "loop-1", Some("note"));
        publish_persist_failed(
            None,
            Some("cp-1".to_string()),
            "loop-1",
            &checkpoint_base::error::CheckpointError::Coordinator("boom".to_string()),
        );
        publish_best_effort_failed(None, None, "loop-1", "projection", "boom");
        publish_cleanup_skipped(None, Some("cp-1".to_string()), "loop-1", "cleanup", "race");
    }

    #[test]
    fn persisted_event_carries_ids_and_description() {
        let bus = CheckpointEventBus::new();
        let mut rx = bus.subscribe();
        publish_persisted(Some(&bus), "cp-1", "loop-1", Some("note"));
        match rx.try_recv().expect("created event is published") {
            CheckpointEvent::Created { data, .. } => {
                assert_eq!(data.checkpoint_id.as_deref(), Some("cp-1"));
                assert_eq!(data.execution_id.as_deref(), Some("loop-1"));
                assert_eq!(data.description.as_deref(), Some("note"));
            }
            other => panic!("expected Created, got {other:?}"),
        }
    }

    #[test]
    fn failed_and_skipped_events_stay_distinct() {
        let bus = CheckpointEventBus::new();
        let mut rx = bus.subscribe();
        publish_persist_failed(
            Some(&bus),
            Some("cp-1".to_string()),
            "loop-1",
            &checkpoint_base::error::CheckpointError::Coordinator("boom".to_string()),
        );
        publish_cleanup_skipped(
            Some(&bus),
            Some("cp-2".to_string()),
            "loop-1",
            "cleanup",
            "lost race",
        );
        let first = rx.try_recv().expect("failed event");
        let second = rx.try_recv().expect("skipped event");
        match first {
            CheckpointEvent::Failed { data, .. } => {
                assert_eq!(data.operation.as_deref(), Some("create"));
                assert!(data
                    .error
                    .as_deref()
                    .is_some_and(|error| error.contains("boom")));
            }
            other => panic!("expected Failed, got {other:?}"),
        }
        match second {
            CheckpointEvent::Skipped { data, .. } => {
                assert_eq!(data.operation.as_deref(), Some("cleanup"));
                assert_eq!(data.checkpoint_id.as_deref(), Some("cp-2"));
            }
            other => panic!("expected Skipped, got {other:?}"),
        }
    }
}
