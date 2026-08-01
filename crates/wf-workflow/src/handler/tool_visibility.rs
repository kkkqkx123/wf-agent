use async_trait::async_trait;
use serde_json::Value;
use wf_core::EventBus;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_types::events::{BaseEvent, EventType};
use wf_types::message::{Message, MessageContentValue, MessageRole};
use wf_types::node::StaticNodeType;

use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::NodeHandler;
use crate::message_context;

/// Build a lightweight tool visibility notification message describing which
/// tools were added/removed (aligned with the TS tool-visibility-message-builder).
fn build_visibility_message(action: &str, tools: &[String]) -> String {
    let list: Vec<String> = tools.iter().map(|t| format!("- {}", t)).collect();
    match action {
        "block" => format!(
            "The following tools are now unavailable:\n{}",
            list.join("\n")
        ),
        "unblock" => format!(
            "The following tools are now available:\n{}",
            list.join("\n")
        ),
        _ => format!("Tool visibility changed ({}):\n{}", action, list.join("\n")),
    }
}

fn emit_visibility_event(
    event_bus: Option<&EventBus>,
    ctx: &NodeExecutionContext,
    action: &str,
    tools: &[String],
) {
    let Some(bus) = event_bus else { return };
    let _ = bus.publish(BaseEvent {
        id: wf_types::Id::new(),
        r#type: EventType::NodeCustomEvent,
        timestamp: wf_common::now(),
        workflow_id: Some(ctx.execution_id.clone()),
        execution_id: Some(ctx.execution_id.clone()),
        agent_loop_id: None,
        metadata: Some(std::collections::HashMap::from([
            (
                "event".to_string(),
                Value::String("tool_visibility_changed".to_string()),
            ),
            ("action".to_string(), Value::String(action.to_string())),
            (
                "tools".to_string(),
                Value::Array(tools.iter().map(|t| Value::String(t.clone())).collect()),
            ),
        ])),
    });
}

pub struct ToolVisibilityHandler;

#[async_trait]
impl NodeHandler for ToolVisibilityHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::ToolVisibility
    }

    async fn execute(&self, ctx: &mut NodeExecutionContext) -> WorkflowResult<NodeExecutionResult> {
        let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);
        let action = config
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("block");
        let tools = config
            .get("tools")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        if tools.is_empty() {
            return Err(WorkflowError::OperationError(
                "ToolVisibility node requires 'tools' list".to_string(),
            ));
        }

        match action {
            "block" => {
                for tool_name in &tools {
                    ctx.variables
                        .insert(format!("__tool_blocked_{}", tool_name), Value::Bool(true));
                }
            }
            "unblock" => {
                for tool_name in &tools {
                    ctx.variables
                        .remove(&format!("__tool_blocked_{}", tool_name));
                }
            }
            _ => {
                return Err(WorkflowError::OperationError(format!(
                    "Invalid ToolVisibility action: {}. Expected 'block' or 'unblock'",
                    action
                )));
            }
        }

        // Record the change in the message context so agents/LLM nodes can
        // observe it in the conversation.
        let context_id = config
            .get("context_id")
            .or_else(|| config.get("contextId"))
            .and_then(|v| v.as_str())
            .unwrap_or(message_context::DEFAULT_CONTEXT_ID);
        let content = build_visibility_message(action, &tools);
        message_context::append_context(
            &ctx.variables,
            context_id,
            vec![Message {
                id: wf_types::Id::new(),
                role: MessageRole::System,
                content: MessageContentValue::Text(content),
                timestamp: wf_common::now(),
                tool_call_id: None,
                tool_name: None,
                tool_calls: None,
                thinking: None,
                metadata: Some(std::collections::HashMap::from([(
                    "type".to_string(),
                    Value::String("tool_visibility".to_string()),
                )])),
            }],
        );

        emit_visibility_event(ctx.event_bus.as_deref(), ctx, action, &tools);

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("action".to_string(), Value::String(action.to_string()));
        metadata.insert(
            "tools".to_string(),
            Value::Array(tools.iter().map(|t| Value::String(t.clone())).collect()),
        );
        metadata.insert(
            "context_id".to_string(),
            Value::String(context_id.to_string()),
        );

        Ok(NodeExecutionResult {
            output: ctx.input.clone(),
            next_node_ids: Vec::new(),
            metadata,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn visibility_message_content() {
        let msg =
            build_visibility_message("block", &["file_read".to_string(), "shell".to_string()]);
        assert!(msg.contains("file_read"));
        assert!(msg.contains("shell"));
        assert!(msg.contains("unavailable"));

        let unblock = build_visibility_message("unblock", &["shell".to_string()]);
        assert!(unblock.contains("now available"));
        assert!(unblock.contains("shell"));
    }

    #[tokio::test]
    async fn visibility_change_recorded_in_context() {
        let vars = Arc::new(dashmap::DashMap::new());
        let mut ctx = NodeExecutionContext::new(
            wf_types::Id::new(),
            "tv1".to_string(),
            StaticNodeType::ToolVisibility,
            Value::Null,
            vars.clone(),
        )
        .with_node_config(serde_json::json!({
            "action": "block",
            "tools": ["file_read", "shell"]
        }));

        let result = ToolVisibilityHandler.execute(&mut ctx).await;
        assert!(result.is_ok(), "handler should run: {:?}", result.err());

        assert_eq!(
            vars.get("__tool_blocked_file_read").unwrap().value(),
            &Value::Bool(true)
        );
        assert_eq!(
            vars.get("__tool_blocked_shell").unwrap().value(),
            &Value::Bool(true)
        );

        let context =
            crate::message_context::get_context(&vars, crate::message_context::DEFAULT_CONTEXT_ID);
        assert_eq!(context.len(), 1, "visibility message must be queryable");
        assert_eq!(context[0].role, MessageRole::System);
        let content = match &context[0].content {
            MessageContentValue::Text(t) => t.clone(),
            _ => String::new(),
        };
        assert!(content.contains("file_read"));
        assert!(content.contains("shell"));
        assert!(content.contains("unavailable"));

        let metadata = context[0].metadata.as_ref().expect("metadata");
        assert_eq!(
            metadata.get("type"),
            Some(&Value::String("tool_visibility".to_string()))
        );
    }

    #[tokio::test]
    async fn unblock_removes_markers() {
        let vars = Arc::new(dashmap::DashMap::new());
        vars.insert("__tool_blocked_shell".to_string(), Value::Bool(true));
        let mut ctx = NodeExecutionContext::new(
            wf_types::Id::new(),
            "tv2".to_string(),
            StaticNodeType::ToolVisibility,
            Value::Null,
            vars.clone(),
        )
        .with_node_config(serde_json::json!({
            "action": "unblock",
            "tools": ["shell"]
        }));

        let result = ToolVisibilityHandler.execute(&mut ctx).await;
        assert!(result.is_ok());
        assert!(!vars.contains_key("__tool_blocked_shell"));
    }
}
