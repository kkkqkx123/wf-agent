use super::*;

#[tokio::test]
async fn context_trigger_sets_variable_on_live_execution() {
    use wf_types::trigger::{TriggerCondition, TriggerTemplate};

    let bus = Arc::new(EventBus::new(64));
    let registries = Arc::new(ResourceRegistries::new());
    let gateway = Arc::new(LlmGateway::new());
    let contexts = Arc::new(ExecutionContextRegistry::new());
    let listener =
        start_trigger_listener(bus.clone(), registries.clone(), gateway, contexts.clone());
    wait_for_listener(&bus, 1).await;

    // A SetVariable trigger template matching custom events.
    let ts = wf_common::now();
    let _ = wf_core::registry::MutableRegistry::register(
        &registries.trigger_templates,
        "on_flag".to_string(),
        Arc::new(TriggerTemplate {
            name: "on_flag".to_string(),
            description: Some("set a variable from an event".to_string()),
            condition: Some(TriggerCondition {
                event_type: "NODE_CUSTOM_EVENT".to_string(),
                event_name: Some("flag_raised".to_string()),
                condition: None,
                metadata: None,
                metadata_exists: None,
                execution_prefix: None,
            }),
            action: Some(TriggerAction::SetVariable {
                variable_name: "event_flag".to_string(),
                value: Value::Bool(true),
            }),
            enabled: Some(true),
            max_triggers: None,
            priority: Some(10),
            dispatch_mode: None,
            allow_multi_effect: None,
            effect_order: None,
            metadata: None,
            created_at: ts,
            updated_at: ts,
            create_checkpoint: None,
            checkpoint_description_template: None,
        }),
    );

    // A live workflow execution registered in the write-back registry.
    let execution_id = "exec-live-1".to_string();
    let variables = Arc::new(DashMap::new());
    contexts.register_workflow(execution_id.clone(), variables.clone());

    bus.publish(BaseEvent {
        id: wf_common::generate_id(),
        r#type: EventType::NodeCustomEvent,
        timestamp: wf_common::now(),
        event_name: Some("flag_raised".to_string()),
        workflow_id: Some(Id::from("wf-live-1".to_string())),
        execution_id: Some(execution_id.clone()),
        agent_loop_id: None,
        metadata: None,
    })
    .unwrap();

    // The action ran against the live execution's variable map.
    wait_until(|| variables.contains_key("event_flag")).await;
    assert_eq!(
        variables.get("event_flag").map(|v| v.value().clone()),
        Some(Value::Bool(true))
    );

    stop_trigger_listener(listener).await;
}

#[tokio::test]
async fn context_trigger_skips_events_without_live_context() {
    use wf_types::trigger::{TriggerCondition, TriggerTemplate};

    let bus = Arc::new(EventBus::new(64));
    let registries = Arc::new(ResourceRegistries::new());
    let gateway = Arc::new(LlmGateway::new());
    let contexts = Arc::new(ExecutionContextRegistry::new());
    let listener =
        start_trigger_listener(bus.clone(), registries.clone(), gateway, contexts.clone());
    wait_for_listener(&bus, 1).await;

    let ts = wf_common::now();
    let _ = wf_core::registry::MutableRegistry::register(
        &registries.trigger_templates,
        "on_flag_2".to_string(),
        Arc::new(TriggerTemplate {
            name: "on_flag_2".to_string(),
            description: None,
            condition: Some(TriggerCondition {
                event_type: "NODE_CUSTOM_EVENT".to_string(),
                event_name: Some("flag_raised".to_string()),
                condition: None,
                metadata: None,
                metadata_exists: None,
                execution_prefix: None,
            }),
            action: Some(TriggerAction::SetVariable {
                variable_name: "event_flag".to_string(),
                value: Value::Bool(true),
            }),
            enabled: Some(true),
            max_triggers: None,
            priority: Some(10),
            dispatch_mode: None,
            allow_multi_effect: None,
            effect_order: None,
            metadata: None,
            created_at: ts,
            updated_at: ts,
            create_checkpoint: None,
            checkpoint_description_template: None,
        }),
    );

    // No execution registered: the runner must skip (agent sessions are
    // not registered), never fail the listener loop.
    bus.publish(BaseEvent {
        id: wf_common::generate_id(),
        r#type: EventType::NodeCustomEvent,
        timestamp: wf_common::now(),
        event_name: Some("flag_raised".to_string()),
        workflow_id: None,
        execution_id: Some("no-such-execution".to_string()),
        agent_loop_id: None,
        metadata: None,
    })
    .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    stop_trigger_listener(listener).await;
}

#[tokio::test]
async fn listener_subscription_exists_synchronously_after_start() {
    let bus = Arc::new(EventBus::new(64));
    let registries = Arc::new(ResourceRegistries::new());
    let gateway = Arc::new(LlmGateway::new());
    let contexts = Arc::new(ExecutionContextRegistry::new());
    let listener =
        start_trigger_listener(bus.clone(), registries.clone(), gateway, contexts.clone());
    assert!(
        bus.total_receiver_count() >= 1,
        "subscription must exist synchronously after start (got {})",
        bus.total_receiver_count()
    );

    stop_trigger_listener(listener).await;
}
