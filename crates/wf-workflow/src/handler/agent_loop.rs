use async_trait::async_trait;
use futures::StreamExt;
use serde_json::Value;
use std::sync::Arc;

use wf_agent::checkpoint::AgentCheckpointStrategy;
use wf_agent::coordinator::lifecycle::AgentLoopCoordinator;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_llm::LlmGateway;
use wf_tools::callback::{AgentLoopConfig, AgentLoopInput, HookConfig};
use wf_tools::registry::ToolRegistry;

use wf_types::message::Message;
use wf_types::node::StaticNodeType;

use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::NodeHandler;
use crate::message_context;

fn parse_agent_hooks(agent_config: Option<&wf_types::agent::AgentConfig>) -> Vec<HookConfig> {
    agent_config
        .and_then(|c| c.hooks.as_ref())
        .map(|hooks| {
            hooks
                .iter()
                .map(|h| HookConfig {
                    hook_type: serde_json::to_string(&h.hook_type)
                        .map(|t| t.trim_matches('"').to_string())
                        .unwrap_or_else(|_| format!("{:?}", h.hook_type)),
                    condition: h.condition.clone(),
                    enabled: h.enabled.unwrap_or(true),
                    parallel: None,
                    continue_on_error: None,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Collect the initial conversation: inline `conversation` messages plus all
/// messages from the named contexts listed in `message_inputs`.
fn collect_initial_conversation(ctx: &NodeExecutionContext) -> Vec<Message> {
    let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);
    let mut conversation: Vec<Message> = Vec::new();

    if let Some(inline) = config.get("conversation").and_then(|v| v.as_array()) {
        for msg in inline {
            if let Ok(m) = serde_json::from_value(msg.clone()) {
                conversation.push(m);
            }
        }
    }

    if let Some(inputs) = config
        .get("message_inputs")
        .or_else(|| config.get("messageInputs"))
        .and_then(|v| v.as_array())
    {
        for entry in inputs {
            let source = entry
                .get("source_context_id")
                .or_else(|| entry.get("sourceContextId"))
                .and_then(|v| v.as_str());
            if let Some(source) = source {
                conversation.extend(message_context::get_context(&ctx.variables, source));
            }
        }
    }

    conversation
}

/// Export the final conversation to the target contexts declared in
/// `message_outputs`.
fn export_conversation(ctx: &NodeExecutionContext, conversation: &[Message]) {
    let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);
    if let Some(outputs) = config
        .get("message_outputs")
        .or_else(|| config.get("messageOutputs"))
        .and_then(|v| v.as_array())
    {
        for entry in outputs {
            let target = entry
                .get("target_context_id")
                .or_else(|| entry.get("targetContextId"))
                .and_then(|v| v.as_str());
            if let Some(target) = target {
                message_context::register_context(&ctx.variables, target, conversation.to_vec());
            }
        }
    }
}

pub struct AgentLoopHandler {
    gateway: Arc<LlmGateway>,
}

impl AgentLoopHandler {
    pub fn new(gateway: Arc<LlmGateway>) -> Self {
        Self { gateway }
    }
}

#[async_trait]
impl NodeHandler for AgentLoopHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::AgentLoop
    }

    async fn execute(&self, ctx: &mut NodeExecutionContext) -> WorkflowResult<NodeExecutionResult> {
        let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);

        // Canonical AgentLoopNodeConfig: either an inline AgentDefinition or
        // an agent_loop_id reference. Agent loop id resolution is not
        // supported without an agent registry, so a referenced loop must
        // ship an inline definition.
        let definition: Option<wf_types::agent::AgentDefinition> = config
            .get("inline_definition")
            .and_then(|v| serde_json::from_value(v.clone()).ok());
        let agent_config = definition.as_ref().and_then(|d| d.config.as_ref());

        if definition.is_none() {
            return Err(WorkflowError::Internal(
                "AGENT_LOOP node requires an inline_definition (or a resolvable agent_loop_id)"
                    .to_string(),
            ));
        }

        let model = agent_config
            .and_then(|c| c.profile_id.clone())
            .ok_or_else(|| {
                WorkflowError::OperationError(
                    "AGENT_LOOP node requires a profile_id in inline_definition.config".to_string(),
                )
            })?;
        let system_prompt = agent_config.and_then(|c| c.system_prompt.clone());
        let max_iterations = agent_config.and_then(|c| c.max_iterations).unwrap_or(10);
        let stream_enabled = agent_config.and_then(|c| c.stream).unwrap_or(false);
        let max_execution_time = config.get("execution_timeout").and_then(|v| v.as_u64());

        let tool_names: Vec<String> = agent_config
            .and_then(|c| c.available_tools.as_ref())
            .map(|tools| {
                tools
                    .available
                    .iter()
                    .chain(tools.initial.as_ref().into_iter().flatten())
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();

        let text = if let Value::String(s) = &ctx.input {
            s.clone()
        } else {
            ctx.input.to_string()
        };

        let message = if let Some(ref sp) = system_prompt {
            format!("{}\n\n{}", sp, text)
        } else {
            text
        };

        let tool_registry = ctx
            .tool_registry
            .clone()
            .unwrap_or_else(|| std::sync::Arc::new(ToolRegistry::new()));
        let mut coordinator = AgentLoopCoordinator::new(self.gateway.clone(), tool_registry);
        if let Some(ref bus) = ctx.event_bus {
            coordinator = coordinator.with_event_bus(bus.clone());
        }
        if let Some(checkpoint) = agent_config.and_then(|c| c.checkpoint.as_ref()) {
            if checkpoint.enabled {
                let interval = checkpoint.interval_iterations.unwrap_or(1);
                coordinator = coordinator.with_checkpoint_strategy(
                    AgentCheckpointStrategy::every_n_iterations(interval),
                );
            }
        }

        let loop_config = AgentLoopConfig {
            agent_id: ctx.node_id.clone(),
            model,
            available_tool_names: tool_names,
            hooks: parse_agent_hooks(agent_config),
            max_iterations: Some(max_iterations),
            max_execution_time,
            tool_call_format: agent_config
                .and_then(|c| c.tool_call_format.as_ref())
                .and_then(|format| wf_types::llm::ToolCallFormatConfig::from_format_str(format)),
        };

        let loop_input = AgentLoopInput {
            message,
            context: std::collections::HashMap::new(),
            conversation: collect_initial_conversation(ctx),
        };

        if stream_enabled {
            // Stream pass-through: forward deltas and lifecycle events to the
            // workflow event bus, aggregate the final result.
            let mut stream = coordinator.execute_stream(loop_config, loop_input).await;
            let mut final_result = Value::Null;
            let mut iterations = 0u32;
            let mut last_error: Option<String> = None;
            while let Some(event) = stream.next().await {
                if let Some(ref bus) = ctx.event_bus {
                    let event_type = match &event {
                        wf_agent::AgentStreamEvent::LlmDelta { .. } => {
                            wf_types::events::EventType::LlmStreamChunk
                        }
                        wf_agent::AgentStreamEvent::ToolStart { .. } => {
                            wf_types::events::EventType::AgentToolExecutionStarted
                        }
                        wf_agent::AgentStreamEvent::ToolEnd { .. } => {
                            wf_types::events::EventType::AgentToolExecutionCompleted
                        }
                        wf_agent::AgentStreamEvent::IterationStart { .. } => {
                            wf_types::events::EventType::AgentIterationStarted
                        }
                        wf_agent::AgentStreamEvent::IterationEnd { .. } => {
                            wf_types::events::EventType::AgentIterationCompleted
                        }
                        wf_agent::AgentStreamEvent::Completed { .. } => {
                            wf_types::events::EventType::AgentCompleted
                        }
                        wf_agent::AgentStreamEvent::Failed { .. } => {
                            wf_types::events::EventType::AgentFailed
                        }
                        wf_agent::AgentStreamEvent::Interrupted { .. } => {
                            wf_types::events::EventType::AgentCancelled
                        }
                    };
                    let bus_event = wf_types::events::BaseEvent {
                        id: wf_common::generate_id(),
                        r#type: event_type,
                        timestamp: wf_common::now(),
                        workflow_id: Some(ctx.execution_id.clone()),
                        execution_id: Some(ctx.execution_id.clone()),
                        agent_loop_id: Some(ctx.node_id.clone()),
                        metadata: serde_json::to_value(&event).ok().and_then(|v| {
                            v.as_object()
                                .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                        }),
                    };
                    let _ = bus.publish(bus_event);
                }
                match event {
                    wf_agent::AgentStreamEvent::Completed {
                        result,
                        iterations: it,
                    } => {
                        final_result = result;
                        iterations = it;
                    }
                    wf_agent::AgentStreamEvent::Failed { error } => {
                        last_error = Some(error);
                    }
                    _ => {}
                }
            }

            if let Some(error) = last_error {
                return Err(WorkflowError::AgentError(
                    wf_agent::AgentError::ExecutionError(error),
                ));
            }

            let mut metadata = std::collections::HashMap::new();
            metadata.insert(
                "iteration_count".to_string(),
                Value::Number(iterations.into()),
            );
            metadata.insert("node_id".to_string(), Value::String(ctx.node_id.clone()));
            return Ok(NodeExecutionResult {
                output: final_result,
                next_node_ids: Vec::new(),
                metadata,
            });
        }

        match coordinator.execute(loop_config, loop_input).await {
            Ok(output) => {
                export_conversation(ctx, &output.conversation);

                let mut metadata = std::collections::HashMap::new();
                metadata.insert(
                    "iteration_count".to_string(),
                    Value::Number(output.iterations.into()),
                );
                metadata.insert("node_id".to_string(), Value::String(ctx.node_id.clone()));
                metadata.insert(
                    "message_count".to_string(),
                    Value::Number(serde_json::Number::from(output.conversation.len() as u64)),
                );

                let final_content = output.result;

                Ok(NodeExecutionResult {
                    output: final_content,
                    next_node_ids: Vec::new(),
                    metadata,
                })
            }
            Err(e) => Err(WorkflowError::AgentError(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hooks_from_agent_config() {
        let agent_config = serde_json::from_value::<wf_types::agent::AgentConfig>(serde_json::json!({
            "profile_id": "mock",
            "hooks": [
                {"hook_type": "BEFORE_ITERATION", "event_name": "before_iteration", "enabled": true},
                {"hook_type": "AFTER_TOOL_CALL", "event_name": "after_tool_call", "enabled": false}
            ]
        }))
        .expect("canonical agent config should parse");
        let hooks = parse_agent_hooks(Some(&agent_config));
        assert_eq!(hooks.len(), 2);
        assert_eq!(hooks[0].hook_type, "BEFORE_ITERATION");
        assert!(hooks[0].enabled);
        assert!(!hooks[1].enabled);
    }

    #[test]
    fn no_hooks_without_agent_config() {
        assert!(parse_agent_hooks(None).is_empty());
    }

    #[test]
    fn collects_conversation_from_contexts() {
        let vars = std::sync::Arc::new(dashmap::DashMap::new());
        message_context::append_context(
            &vars,
            "chat",
            vec![Message {
                id: wf_types::Id::new(),
                role: wf_types::message::MessageRole::User,
                content: wf_types::message::MessageContentValue::Text("hi".to_string()),
                timestamp: wf_common::now(),
                tool_call_id: None,
                tool_name: None,
                tool_calls: None,
                thinking: None,
                metadata: None,
            }],
        );
        let ctx = NodeExecutionContext::new(
            wf_types::Id::new(),
            "agent".to_string(),
            StaticNodeType::AgentLoop,
            Value::Null,
            vars,
        )
        .with_node_config(serde_json::json!({
            "message_inputs": [{"source_context_id": "chat"}]
        }));
        let conversation = collect_initial_conversation(&ctx);
        assert_eq!(conversation.len(), 1);
    }
}
