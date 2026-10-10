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
