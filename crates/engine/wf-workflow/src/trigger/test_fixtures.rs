//! Shared builders for the trigger unit tests.

use wf_types::events::{BaseEvent, EventType};
use wf_types::trigger::{TriggerCondition, TriggerTemplate};

/// Sub-workflow trigger template for `event_type` with a per-execution
/// `max_triggers` budget (`0` = absent = unbounded; zero is rejected at load
/// time and means no capacity at runtime, so fixtures never emit it).
pub(crate) fn event_template(name: &str, event_type: &str, max_triggers: u32) -> TriggerTemplate {
    TriggerTemplate {
        name: name.to_string(),
        description: None,
        condition: Some(TriggerCondition {
            event_type: event_type.to_string(),
            event_name: None,
            condition: None,
            metadata: None,
            metadata_exists: None,
            execution_prefix: None,
        }),
        action: Some(
            serde_json::from_value(serde_json::json!({
                "action_type": "execute_triggered_subworkflow",
                "triggered_workflow_id": "summary_flow",
            }))
            .expect("sub-workflow action fixture"),
        ),
        enabled: Some(true),
        max_triggers: if max_triggers == 0 {
            None
        } else {
            Some(max_triggers)
        },
        priority: None,
        dispatch_mode: None,
        allow_multi_effect: None,
        effect_order: None,
        metadata: None,
        created_at: 0,
        updated_at: 0,
        create_checkpoint: None,
        checkpoint_description_template: None,
    }
}

/// Cold-start (execution-creating) template keyed by `event_name`.
pub(crate) fn creation_template(
    name: &str,
    event_name: &str,
    max_triggers: u32,
) -> TriggerTemplate {
    let mut template = event_template(name, "NODE_CUSTOM_EVENT", max_triggers);
    template.condition.as_mut().expect("condition").event_name = Some(event_name.to_string());
    template.action = Some(
        serde_json::from_value(serde_json::json!({
            "action_type": "execute_workflow",
            "workflow_id": "nightly_flow",
        }))
        .expect("execution-creating action fixture"),
    );
    template
}

/// Template bound to `event_name` whose action needs the emitting execution.
pub(crate) fn scoped_custom_template(name: &str, event_name: &str) -> TriggerTemplate {
    let mut template = creation_template(name, event_name, 0);
    template.action = Some(
        serde_json::from_value(serde_json::json!({
            "action_type": "set_variable",
            "variable_name": "x",
            "value": 1,
        }))
        .expect("execution-scoped action fixture"),
    );
    template
}

pub(crate) fn base_event(event_type: EventType, execution_id: &str) -> BaseEvent {
    BaseEvent {
        id: wf_types::Id::new(),
        r#type: event_type,
        timestamp: wf_common::now(),
        workflow_id: None,
        execution_id: Some(execution_id.to_string()),
        agent_loop_id: None,
        event_name: None,
        metadata: None,
    }
}

/// Execution-less creation event (scheduler tick / webhook ingress) stamped
/// with the producer's `fire_id`.
pub(crate) fn execution_less_event(event_name: &str, fire_id: &str) -> BaseEvent {
    BaseEvent {
        id: wf_types::Id::new(),
        r#type: EventType::NodeCustomEvent,
        timestamp: wf_common::now(),
        workflow_id: None,
        execution_id: None,
        agent_loop_id: None,
        event_name: Some(event_name.to_string()),
        metadata: Some(std::collections::HashMap::from([
            ("fire_id".to_string(), serde_json::json!(fire_id)),
            ("source".to_string(), serde_json::json!("schedule")),
        ])),
    }
}

/// Template names in order, for readable assertions.
pub(crate) fn names(templates: &[TriggerTemplate]) -> Vec<&str> {
    templates.iter().map(|t| t.name.as_str()).collect()
}
