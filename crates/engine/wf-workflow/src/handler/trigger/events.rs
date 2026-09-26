//! Event-bus emission helpers for synchronous trigger actions.

use serde_json::Value;
use wf_types::events::{BaseEvent, EventType};

use crate::handler::trigger::context::TriggerContext;

pub(crate) async fn emit(ctx: &TriggerContext, event_type: EventType, message: &str) {
    emit_with_metadata(ctx, event_type, message, &[]).await;
}

pub(crate) async fn emit_with_metadata(
    ctx: &TriggerContext,
    event_type: EventType,
    message: &str,
    extra: &[(&str, Value)],
) {
    match &ctx.event_bus {
        Some(bus) => {
            let mut metadata = std::collections::HashMap::from([(
                "trigger_message".to_string(),
                Value::String(message.to_string()),
            )]);
            for (key, value) in extra {
                metadata.insert(key.to_string(), value.clone());
            }
            bus.publish_logged(
                BaseEvent {
                    id: wf_common::generate_id(),
                    r#type: event_type,
                    timestamp: wf_common::now(),
                    workflow_id: Some(ctx.workflow_id.clone()),
                    execution_id: Some(ctx.execution_id.clone()),
                    agent_loop_id: None,

                    event_name: None,
                    metadata: Some(metadata),
                },
                &format!("workflow={} trigger", ctx.execution_id),
            )
            .ok();
        }
        None => {
            tracing::debug!(
                execution_id = %ctx.execution_id,
                ?event_type,
                "no event bus, skipping trigger event"
            );
        }
    }
}
