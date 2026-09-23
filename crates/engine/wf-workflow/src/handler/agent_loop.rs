use async_trait::async_trait;
use std::sync::Arc;

use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_llm::LlmGateway;
use wf_tools::callback::{AgentLoopConfig, AgentLoopInput};
use wf_types::node::StaticNodeType;

use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::NodeHandler;

pub(crate) mod conversation;
pub(crate) mod coordinator;
pub(crate) mod settings;
pub(crate) mod stream;

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

    async fn execute(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> wf_execution_shared::error::ExecutionSharedResult<NodeExecutionResult> {
        self.execute_inner(ctx).await.map_err(Into::into)
    }
}

impl AgentLoopHandler {
    /// Orchestrate a single `AGENT_LOOP` execution: load static settings,
    /// assemble the stable system header and the volatile tail through the
    /// shared prompt module, normalize the inbound conversation, build the
    /// coordinator and loop config, then run the blocking or streaming path.
    /// The header seeds a leading system message once so the request prefix
    /// stays cacheable; volatile data travels as a separate marked user
    /// message and the user task stays pure user input.
    async fn execute_inner(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> WorkflowResult<NodeExecutionResult> {
        let settings = settings::load_settings(ctx, &self.gateway)?;
        let agent_config = settings.agent_config();

        let env = wf_execution_shared::agent_prompt::PromptEnvironment::new(
            ctx.resource_registries.as_deref(),
            ctx.tool_registry.as_deref(),
            ctx.metrics.as_deref(),
        );
        let assembled = wf_execution_shared::agent_prompt::assemble_agent_prompt(
            agent_config,
            &*ctx,
            &env,
            &settings.available_tool_names,
        );

        let artifacts = wf_execution_shared::agent_prompt::build_exposure_artifacts(
            &env,
            settings.tool_call_protocol.as_ref(),
            &settings.available_tool_names,
            &settings.initial_tool_names,
            &settings.discoverable_tool_names,
            &settings.hidden_tool_names,
            settings.enable_general_tool,
        );

        let coordinator = coordinator::build_coordinator(self.gateway.clone(), ctx, agent_config);

        // Loop-boundary history normalization: upstream archives carry
        // upstream bucket shapes; rewrite once to this loop's target
        // exposure (including tools activated by prior TOOL_VISIBILITY
        // nodes) so the new schema and the replayed history agree. The
        // rewritten history is self-consistent (call/result ids remapped
        // together), so no id-map sidecar is needed downstream.
        let mut initial_conversation = conversation::normalize_conversation_for_target(
            conversation::collect_initial_conversation(ctx),
            &conversation::TargetExposure {
                registry: ctx.tool_registry.as_deref(),
                available: &settings.available_tool_names,
                initial: &settings.initial_tool_names,
                discoverable: &settings.discoverable_tool_names,
                hidden: &settings.hidden_tool_names,
                enable_general_tool: settings.enable_general_tool,
                activated: &settings.activated_tool_names,
            },
        );

        let message = settings.input_text.clone();
        wf_execution_shared::agent_prompt::apply_assembled_prompt(
            &mut initial_conversation,
            &assembled,
            wf_execution_shared::agent_prompt::DynamicTailBearing::SeparateUserMessage,
        );

        let loop_config = AgentLoopConfig {
            agent_id: ctx.node_id.clone(),
            model: settings.model,
            available_tool_names: settings.available_tool_names,
            initial_tool_names: settings.initial_tool_names,
            discoverable_tool_names: settings.discoverable_tool_names,
            enable_general_tool: settings.enable_general_tool,
            activated_tool_names: settings.activated_tool_names,
            hidden_tool_names: settings.hidden_tool_names,
            hooks: settings.hooks,
            max_iterations: Some(settings.max_iterations),
            max_execution_time: settings.max_execution_time,
            tool_call_protocol: settings.tool_call_protocol,
            token_limit: settings.token_limit,
            token_warning_threshold: settings.token_warning_threshold,
            enable_token_tracking: settings.enable_token_tracking,
            checkpoint_message_interval: settings.checkpoint_message_interval,
            general_description: artifacts.general_description,
            discoverable_metadata_block: artifacts.discoverable_metadata_block,
            // Turn-level projection stays off here: the one-time boundary
            // normalization above covers the loop-entry shapes, and per-turn
            // rewrites would churn the KV-cache-friendly request prefix.
            history_normalization: false,
        };

        let loop_input = AgentLoopInput {
            message,
            context: std::collections::HashMap::new(),
            conversation: initial_conversation,
        };

        if settings.stream_enabled {
            let stream = coordinator.execute_stream(loop_config, loop_input).await;
            return stream::run_streaming(stream, ctx).await;
        }

        match coordinator.execute(loop_config, loop_input).await {
            Ok(output) => {
                conversation::export_conversation(ctx, &output.conversation);

                let mut metadata = std::collections::HashMap::new();
                metadata.insert(
                    "iteration_count".to_string(),
                    serde_json::Value::Number(output.iterations.into()),
                );
                metadata.insert(
                    "node_id".to_string(),
                    serde_json::Value::String(ctx.node_id.clone()),
                );
                metadata.insert(
                    "message_count".to_string(),
                    serde_json::Value::Number(serde_json::Number::from(
                        output.conversation.len() as u64
                    )),
                );

                Ok(NodeExecutionResult {
                    output: output.result,
                    next_node_ids: Vec::new(),
                    metadata,
                })
            }
            Err(e) => Err(WorkflowError::AgentError(e)),
        }
    }
}
