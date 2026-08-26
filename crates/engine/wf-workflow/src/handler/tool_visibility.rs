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
/// tools were added/removed. `unblock` is a formal activation: the tail
/// system announcement tells the model the tools are now callable directly
/// (the system prompt prefix stays unchanged, so the LLM prefix cache is
/// preserved). The wording is templateable via wf-resource
/// (`tool-visibility.activation` / `tool-visibility.block`); executions
/// without injected resource registries fall back to the built-in text
/// (identical wording, so deployments are indistinguishable).
fn build_visibility_message(
    regs: Option<&wf_resource::ResourceRegistries>,
    action: &str,
    tools: &[String],
) -> String {
    let list: Vec<String> = tools.iter().map(|t| format!("- {}", t)).collect();
    let fallback = match action {
        "block" => format!(
            "The following tools are now unavailable:\n{}",
            list.join("\n")
        ),
        "unblock" => format!(
            "[Tool Activation] The following tools are now available: {}.\n\
             You can call them directly or via the general tool.",
            tools.join(", ")
        ),
        _ => format!("Tool visibility changed ({}):\n{}", action, list.join("\n")),
    };
    let template_id = match action {
        "block" => wf_resource::BLOCK_TEMPLATE_ID,
        "unblock" => wf_resource::ACTIVATION_TEMPLATE_ID,
        _ => return fallback,
    };
    let mut vars = std::collections::HashMap::new();
    vars.insert(
        "tool_names".to_string(),
        tools
            .iter()
            .map(|t| format!("- {}", t))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    if action == "unblock" {
        vars.insert("tool_names".to_string(), tools.join(", "));
    }
    wf_resource::render_visibility_message(regs, template_id, &fallback, &vars)
}

fn emit_visibility_event(
    event_bus: Option<&EventBus>,
    ctx: &NodeExecutionContext,
    action: &str,
    tools: &[String],
) {
    let Some(bus) = event_bus else {
        tracing::debug!(
            execution_id = %ctx.execution_id,
            node_id = %ctx.node_id,
            "no event bus, skipping tool visibility event"
        );
        return;
    };
    bus.publish_logged(
        BaseEvent {
            id: wf_types::Id::new(),
            r#type: EventType::NodeCustomEvent,
            timestamp: wf_common::now(),
            workflow_id: Some(ctx.execution_id.clone()),
            execution_id: Some(ctx.execution_id.clone()),
            agent_loop_id: None,

            event_name: None,
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
        },
        &format!(
            "workflow={} tool-visibility={}",
            ctx.execution_id, ctx.node_id
        ),
    )
    .ok();
}

pub struct ToolVisibilityHandler;

#[async_trait]
impl NodeHandler for ToolVisibilityHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::ToolVisibility
    }

    async fn execute(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> wf_execution_shared::error::ExecutionSharedResult<NodeExecutionResult> {
        self.execute_inner(ctx).await.map_err(Into::into)
    }
}

impl ToolVisibilityHandler {
    async fn execute_inner(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> WorkflowResult<NodeExecutionResult> {
        let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);
        let action = config
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("block");
        let tools = config
            .get("tool_ids")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        if tools.is_empty() {
            return Err(WorkflowError::OperationError(
                "ToolVisibility node requires a 'tool_ids' list".to_string(),
            ));
        }

        match action {
            // block = runtime interception only: the visible schema is
            // unchanged (KV-cache friendly); blocked calls are rejected at
            // execution time by the visibility store.
            "block" => {
                for tool_name in &tools {
                    ctx.variables
                        .insert(format!("__tool_blocked_{}", tool_name), Value::Bool(true));
                }
            }
            // unblock = formal activation: clears the block marker, records
            // the activation so the next agent loop seeds the activated-tool
            // state (gated -> schema) and emits a tail system announcement.
            "unblock" => {
                for tool_name in &tools {
                    ctx.variables
                        .remove(&format!("__tool_blocked_{}", tool_name));
                    ctx.variables
                        .insert(format!("__tool_activated_{}", tool_name), Value::Bool(true));
                }
                if let Some(ref metrics) = ctx.metrics {
                    for tool_name in &tools {
                        metrics.tool().record_activation(tool_name);
                    }
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
        // observe it in the conversation (tail system message).
        let content = build_visibility_message(ctx.resource_registries.as_deref(), action, &tools);
        message_context::append_context(
            &ctx.variables,
            message_context::DEFAULT_CONTEXT_ID,
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
        let msg = build_visibility_message(
            None,
            "block",
            &["file_read".to_string(), "shell".to_string()],
        );
        assert!(msg.contains("file_read"));
        assert!(msg.contains("shell"));
        assert!(msg.contains("unavailable"));

        let unblock = build_visibility_message(None, "unblock", &["shell".to_string()]);
        assert!(unblock.contains("[Tool Activation]"));
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
            "tool_ids": ["file_read", "shell"]
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
    async fn unblock_removes_markers_and_activates() {
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
            "tool_ids": ["shell"]
        }));

        let result = ToolVisibilityHandler.execute(&mut ctx).await;
        assert!(result.is_ok());
        assert!(!vars.contains_key("__tool_blocked_shell"));
        assert_eq!(
            vars.get("__tool_activated_shell").unwrap().value(),
            &Value::Bool(true)
        );
    }

    #[tokio::test]
    async fn block_does_not_touch_activation_markers() {
        let vars = Arc::new(dashmap::DashMap::new());
        vars.insert("__tool_activated_shell".to_string(), Value::Bool(true));
        let mut ctx = NodeExecutionContext::new(
            wf_types::Id::new(),
            "tv3".to_string(),
            StaticNodeType::ToolVisibility,
            Value::Null,
            vars.clone(),
        )
        .with_node_config(serde_json::json!({
            "action": "block",
            "tool_ids": ["shell"]
        }));

        let result = ToolVisibilityHandler.execute(&mut ctx).await;
        assert!(result.is_ok());
        // Block intercepts at runtime only; activation is preserved.
        assert_eq!(
            vars.get("__tool_activated_shell").unwrap().value(),
            &Value::Bool(true)
        );
        assert_eq!(
            vars.get("__tool_blocked_shell").unwrap().value(),
            &Value::Bool(true)
        );
    }
}
