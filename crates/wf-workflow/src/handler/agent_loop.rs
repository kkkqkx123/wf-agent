use async_trait::async_trait;
use futures::StreamExt;
use serde_json::Value;
use wf_agent::checkpoint::AgentCheckpointStrategy;
use wf_agent::coordinator::lifecycle::AgentLoopCoordinator;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_llm::{ClientFactory, LlmWrapper};
use wf_tools::callback::{AgentLoopConfig, AgentLoopInput, HookConfig};
use wf_tools::registry::ToolRegistry;
use wf_types::message::Message;
use wf_types::node::StaticNodeType;
use wf_types::tool::approval::ToolApprovalOptions;

use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::NodeHandler;
use crate::message_context;

fn parse_hooks(config: &Value) -> Vec<HookConfig> {
    config
        .get("hooks")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|h| serde_json::from_value(h.clone()).ok())
                .collect()
        })
        .unwrap_or_default()
}

fn parse_approval_options(config: &Value) -> Option<ToolApprovalOptions> {
    let approval = config.get("approval")?;
    if approval.get("enabled").and_then(|v| v.as_bool()) == Some(false) {
        return None;
    }
    serde_json::from_value(approval.clone()).ok()
}

fn parse_checkpoint_strategy(config: &Value) -> Option<AgentCheckpointStrategy> {
    let checkpoint = config.get("checkpoint")?;
    if checkpoint.get("enabled").and_then(|v| v.as_bool()) == Some(false) {
        return None;
    }
    let interval = checkpoint
        .get("interval")
        .and_then(|v| v.as_u64())
        .unwrap_or(1);
    Some(AgentCheckpointStrategy::every_n_iterations(interval as u32))
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
    factory: Option<ClientFactory>,
}

impl AgentLoopHandler {
    pub fn new() -> Self {
        Self { factory: None }
    }

    /// Inject a client factory (e.g. one holding mock clients in tests).
    /// `None` keeps the default behavior: a fresh factory is created per call.
    pub fn with_factory(factory: ClientFactory) -> Self {
        Self {
            factory: Some(factory),
        }
    }

    fn get_factory(&self) -> ClientFactory {
        self.factory.clone().unwrap_or_default()
    }
}

impl Default for AgentLoopHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl NodeHandler for AgentLoopHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::AgentLoop
    }

    async fn execute(&self, ctx: &mut NodeExecutionContext) -> WorkflowResult<NodeExecutionResult> {
        let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);

        let max_iterations = config
            .get("max_iterations")
            .and_then(|v| v.as_u64())
            .unwrap_or(10) as u32;

        let max_execution_time = config.get("max_execution_time").and_then(|v| v.as_u64());

        let model = config
            .get("model")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let system_prompt = config
            .get("system_prompt")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let stream_enabled = config
            .get("stream")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let tool_names: Vec<String> = config
            .get("available_tools")
            .or_else(|| config.get("available_tool_names"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
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

        let mut llm_wrapper = LlmWrapper::with_factory(self.get_factory());
        if let Some(metrics) = &ctx.metrics {
            llm_wrapper = llm_wrapper.with_token_metrics(metrics.token().as_ref().clone());
        }
        let tool_registry = ctx
            .tool_registry
            .clone()
            .unwrap_or_else(|| std::sync::Arc::new(ToolRegistry::new()));

        let mut coordinator =
            AgentLoopCoordinator::new(std::sync::Arc::new(llm_wrapper), tool_registry);
        if let Some(ref bus) = ctx.event_bus {
            coordinator = coordinator.with_event_bus(bus.clone());
        }
        if let Some(max_pause) = config.get("max_pause_duration").and_then(|v| v.as_u64()) {
            coordinator = coordinator.with_max_pause_duration(max_pause);
        }
        if let Some(options) = parse_approval_options(config) {
            coordinator = coordinator.with_approval_options(options);
        }
        if let Some(strategy) = parse_checkpoint_strategy(config) {
            coordinator = coordinator.with_checkpoint_strategy(strategy);
        }

        let loop_config = AgentLoopConfig {
            agent_id: ctx.node_id.clone(),
            model,
            available_tool_names: tool_names,
            hooks: parse_hooks(config),
            max_iterations: Some(max_iterations),
            max_execution_time,
            tool_call_format: config
                .get("tool_call_format")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
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
    fn parses_hooks_from_config() {
        let config = serde_json::json!({
            "hooks": [
                {"hook_type": "BEFORE_EXECUTE", "enabled": true},
                {"hook_type": "AFTER_AGENT", "enabled": false, "parallel": false}
            ]
        });
        let hooks = parse_hooks(&config);
        assert_eq!(hooks.len(), 2);
        assert_eq!(hooks[0].hook_type, "BEFORE_EXECUTE");
        assert!(hooks[0].enabled);
        assert!(!hooks[1].enabled);
    }

    #[test]
    fn approval_disabled_yields_no_options() {
        let config = serde_json::json!({"approval": {"enabled": false}});
        assert!(parse_approval_options(&config).is_none());
    }

    #[test]
    fn approval_options_parsed() {
        let config = serde_json::json!({
            "approval": {
                "auto_approval_enabled": true,
                "auto_approve_patterns": ["file_read"]
            }
        });
        let options = parse_approval_options(&config).expect("options should parse");
        assert_eq!(options.auto_approval_enabled, Some(true));
        assert_eq!(
            options.auto_approve_patterns,
            Some(vec!["file_read".to_string()])
        );
    }

    #[test]
    fn checkpoint_strategy_config() {
        assert!(
            parse_checkpoint_strategy(&serde_json::json!({"checkpoint": {"enabled": false}}))
                .is_none()
        );
        assert!(
            parse_checkpoint_strategy(&serde_json::json!({"checkpoint": {"enabled": true}}))
                .is_some()
        );
        assert!(
            parse_checkpoint_strategy(&serde_json::json!({"checkpoint": {"interval": 3}}))
                .is_some()
        );
        assert!(parse_checkpoint_strategy(&serde_json::json!({})).is_none());
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
