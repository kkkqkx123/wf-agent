use super::super::*;
use checkpoint_file::event::CheckpointEvent;

#[tokio::test]
async fn failed_event_factory_carries_correlation_fields() {
    let bus = CheckpointEventBus::new();
    let mut rx = bus.subscribe();

    bus.publish(CheckpointEventBus::failed_with(
        Some("cp-1".to_string()),
        "create",
        "persist failed: boom",
        Some("exec-1".to_string()),
    ));

    let event = rx.try_recv().unwrap();
    match event {
        CheckpointEvent::Failed { data, .. } => {
            assert_eq!(data.checkpoint_id.as_deref(), Some("cp-1"));
            assert_eq!(data.operation.as_deref(), Some("create"));
            assert_eq!(data.error.as_deref(), Some("persist failed: boom"));
            assert_eq!(data.execution_id.as_deref(), Some("exec-1"));
        }
        other => panic!("expected Failed event, got {:?}", other),
    }
}

#[test]
fn full_projection_type_and_empty_trigger_unified() {
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
    let ctx = CheckpointContext {
        entity_type: "workflow_execution".to_string(),
        entity_id: "exec-1".to_string(),
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
}
