use async_trait::async_trait;
use serde_json::Value;
use wf_core::EventBus;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_types::events::{BaseEvent, EventType};
use wf_types::node::StaticNodeType;

use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::NodeHandler;

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
            // unblock = formal activation: clears the block marker and records
            // the activation so the next agent loop seeds the activated-tool
            // state (gated -> schema). The loop-entry merged notice carries
            // the model-visible announcement; no per-node message is
            // written so stale announcements cannot accumulate.
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

        // No per-node message is written: the next AGENT_LOOP entry
        // recomputes a fresh merged notice from the current variables and
        // target exposure after stripping stale announcements. Old
        // `tool_visibility` messages remain stripped on import for
        // backward compatibility but are never produced here.
        emit_visibility_event(ctx.event_bus.as_deref(), ctx, action, &tools);

        let mut metadata = std::collections::HashMap::new();
        {
            use wf_execution_shared::context::node_metadata as keys;
            metadata.insert(keys::ACTION.to_string(), Value::String(action.to_string()));
            metadata.insert(
                keys::TOOLS.to_string(),
                Value::Array(tools.iter().map(|t| Value::String(t.clone())).collect()),
            );
        }
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

        // No per-node announcement is written: the loop-entry merged notice
        // owns the model-visible announcement. Only the variable markers
        // persist here.
        let context =
            crate::message_context::get_context(&vars, crate::message_context::DEFAULT_CONTEXT_ID);
        assert!(
            context.is_empty(),
            "tool visibility writes no message context; variables carry the state"
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
