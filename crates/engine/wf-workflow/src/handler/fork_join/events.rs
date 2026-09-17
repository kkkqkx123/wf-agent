use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use wf_core::EventBus;
use wf_execution_shared::context::NodeExecutionContext;
use wf_types::events::{BaseEvent, EventType};

pub fn emit_fork_event(
    event_bus: Option<&Arc<EventBus>>,
    event_type: EventType,
    execution_id: &wf_types::Id,
    metadata: HashMap<String, Value>,
) {
    let Some(bus) = event_bus else {
        tracing::debug!(execution_id = %execution_id, ?event_type, "no event bus, skipping fork/join event");
        return;
    };
    let event = BaseEvent {
        id: wf_types::Id::new(),
        r#type: event_type,
        timestamp: wf_common::now(),
        workflow_id: None,
        execution_id: Some(execution_id.clone()),
        agent_loop_id: None,

        event_name: None,
        metadata: Some(metadata),
    };
    let context = format!("workflow={} fork-join-event", execution_id);
    bus.publish_logged(event, &context).ok();
}

pub fn publish_join_started(
    event_bus: Option<&Arc<EventBus>>,
    ctx: &NodeExecutionContext,
    strategy: &str,
) {
    let join_meta = HashMap::from([
        (
            "join_strategy".to_string(),
            Value::String(strategy.to_string()),
        ),
        ("node_id".to_string(), Value::String(ctx.node_id.clone())),
    ]);
    match event_bus {
        Some(bus) => {
            bus.publish_logged(
                BaseEvent {
                    id: wf_types::Id::new(),
                    r#type: EventType::WorkflowExecutionJoinStarted,
                    timestamp: wf_common::now(),
                    workflow_id: None,
                    execution_id: Some(ctx.execution_id.clone()),
                    agent_loop_id: None,

                    event_name: None,
                    metadata: Some(join_meta),
                },
                &format!("workflow={} join={}", ctx.execution_id, ctx.node_id),
            )
            .ok();
        }
        None => {
            tracing::debug!(
                execution_id = %ctx.execution_id,
                node_id = %ctx.node_id,
                "no event bus, skipping join event"
            );
        }
    }
}

pub fn publish_join_completed(
    event_bus: Option<&Arc<EventBus>>,
    ctx: &NodeExecutionContext,
    strategy: &str,
    success_count: usize,
    failed_count: usize,
    expected: usize,
) {
    match event_bus {
        Some(bus) => {
            bus.publish_logged(
                BaseEvent {
                    id: wf_types::Id::new(),
                    r#type: EventType::WorkflowExecutionJoinCompleted,
                    timestamp: wf_common::now(),
                    workflow_id: None,
                    execution_id: Some(ctx.execution_id.clone()),
                    agent_loop_id: None,

                    event_name: None,
                    metadata: Some(HashMap::from([
                        (
                            "join_strategy".to_string(),
                            Value::String(strategy.to_string()),
                        ),
                        (
                            "branch_count".to_string(),
                            Value::Number(serde_json::Number::from(success_count as u64)),
                        ),
                        (
                            "failed_branch_count".to_string(),
                            Value::Number(serde_json::Number::from(failed_count as u64)),
                        ),
                        (
                            "expected_branches".to_string(),
                            Value::Number(serde_json::Number::from(expected as u64)),
                        ),
                    ])),
                },
                &format!("workflow={} join={}", ctx.execution_id, ctx.node_id),
            )
            .ok();
        }
        None => {
            tracing::debug!(
                execution_id = %ctx.execution_id,
                node_id = %ctx.node_id,
                "no event bus, skipping join event"
            );
        }
    }
}
