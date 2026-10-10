use super::super::*;
use super::fixtures::*;
use checkpoint_file::event::CheckpointEvent;
use wf_storage::backend::StorageBackend;

#[tokio::test]
async fn best_effort_cleanup_skip_emits_queryable_skipped_event() {
    let storage = Arc::new(StorageBackend::new_memory());
    let sm = AgentCheckpointStateManager::new(storage);
    let bus = CheckpointEventBus::new();
    let mut rx = bus.subscribe();
    let coord = AgentCheckpointCoordinator::new(sm).with_event_bus(bus);

    let first = build_and_persist(&coord, "running", 1).await;
    let _second = build_and_persist(&coord, "running", 2).await;
    coord.state_manager().delete(&first.id).await.unwrap();
    let err = coord
        .merge_description_back(&first.id, "loop-1", "late note")
        .await
        .unwrap_err();
    assert!(matches!(err, CheckpointError::NotFound { .. }));
    let mut found = None;
    while let Ok(event) = rx.try_recv() {
        if let CheckpointEvent::Skipped { data, .. } = &event {
            if data.operation.as_deref() == Some("cleanup_skip") {
                found = Some(event);
                break;
            }
        }
    }
    let event = found.expect("cleanup skip emits Skipped event");
    match event {
        CheckpointEvent::Skipped { data, .. } => {
            assert_eq!(data.operation.as_deref(), Some("cleanup_skip"));
        }
        other => panic!("expected Skipped event, got {:?}", other),
    }
}

#[test]
fn best_effort_failure_factory_reuses_failed_shape_for_all_operations() {
    for operation in ["async_projection", "persistence_failure"] {
        let event = CheckpointEventBus::failed_with(
            Some("cp-1".to_string()),
            operation,
            format!("{operation} failed"),
            Some("loop-1".to_string()),
        );
        match event {
            CheckpointEvent::Failed { data, .. } => {
                assert_eq!(data.operation.as_deref(), Some(operation));
                assert_eq!(data.checkpoint_id.as_deref(), Some("cp-1"));
            }
            other => panic!("expected Failed event, got {:?}", other),
        }
    }
    for operation in ["persistence_backlog", "cleanup_skip"] {
        let event = CheckpointEventBus::skipped(
            operation,
            format!("{operation} skipped"),
            Some("cp-1".to_string()),
        );
        match event {
            CheckpointEvent::Skipped { data, .. } => {
                assert_eq!(data.operation.as_deref(), Some(operation));
                assert_eq!(data.checkpoint_id.as_deref(), Some("cp-1"));
            }
            other => panic!("expected Skipped event, got {:?}", other),
        }
    }
}

#[test]
fn alert_counters_increment_in_original_collector() {
    use wf_metrics::collector::CollectorConfig;
    use wf_metrics::CheckpointMetricsCollector;
    let collector = CheckpointMetricsCollector::new(CollectorConfig::default());
    collector.record_async_projection_failure("loop-1");
    collector.record_persistence_backlog("loop-1");
    collector.record_persistence_failure("loop-1");
    collector.record_cleanup_skip("loop-1");
    let stats = collector.usage_stats();
    assert_eq!(stats.async_projection_failures, 1);
    assert_eq!(stats.persistence_backlog, 1);
    assert_eq!(stats.persistence_failures, 1);
    assert_eq!(stats.cleanup_skips, 1);
}

#[test]
fn empty_trigger_set_and_content_default_unified() {
    use checkpoint_base::strategy::{CheckpointStrategy, StandardStrategy};
    use wf_types::checkpoint::UnifiedCheckpointPolicy;
    let empty = UnifiedCheckpointPolicy {
        enabled: true,
        triggers: vec![],
        content: None,
        retention: None,
        error_handling: None,
    };
    let strategy = StandardStrategy::from_policy(&empty);
    let ctx = wf_types::checkpoint::CheckpointContext {
        entity_type: "agent_loop".to_string(),
        entity_id: "loop-1".to_string(),
        trigger: Some(CheckpointTiming::AfterExecute),
        actor_id: None,
        attempt: None,
        retry_count: None,
        error: None,
        fallback_used: None,
        metadata: None,
    };
    assert!(!strategy.should_checkpoint(&CheckpointTiming::AfterExecute, &ctx));
    assert_eq!(strategy.content_config().include_state, Some(true));
    assert_eq!(strategy.content_config().include_history, Some(true));
}
