use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_llm::LlmGateway;
use wf_types::events::EventType;
use wf_types::llm::LlmRequest;
use wf_types::message::{LlmToolCall, Message, MessageContentValue};
use wf_types::node::StaticNodeType;

use crate::error::WorkflowResult;
use crate::handler::NodeHandler;
use crate::message_context;

pub mod config;
pub mod events;
pub mod messages;
pub mod stream;
pub mod token_budget;
pub mod tool_exec;

use config::parse_llm_node_config;
use events::emit_llm_event;
use messages::{build_messages, tool_result_message};
use stream::run_streaming_request;
use token_budget::{
    await_compression_settle, check_preflight_budget, emit_token_usage_events,
    record_non_stream_usage, setup_token_tracker,
};
use tool_exec::{call_llm, execute_tool_call, pending_queue_for, resolve_tools, LlmToolCallBatch};

pub struct LlmHandler {
    gateway: Arc<LlmGateway>,
    file_checkpoint: Option<wf_checkpoint::file::FileCheckpointManager>,
}

impl LlmHandler {
    pub fn new(gateway: Arc<LlmGateway>) -> Self {
        Self {
            gateway,
            file_checkpoint: None,
        }
    }

    pub fn with_file_checkpoint(
        mut self,
        manager: wf_checkpoint::file::FileCheckpointManager,
    ) -> Self {
        self.file_checkpoint = Some(manager);
        self
    }

    pub fn with_file_checkpoint_opt(
        mut self,
        manager: Option<wf_checkpoint::file::FileCheckpointManager>,
    ) -> Self {
        self.file_checkpoint = manager;
        self
    }
}

#[async_trait]
impl NodeHandler for LlmHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::Llm
    }

    async fn execute(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> wf_execution_shared::error::ExecutionSharedResult<NodeExecutionResult> {
        self.execute_inner(ctx).await.map_err(Into::into)
    }
}

impl LlmHandler {
    fn build_request(
        &self,
        ctx: &NodeExecutionContext,
        cfg: &config::LlmNodeConfig,
        node_config: &Value,
        messages: &[Message],
        tools: &[wf_types::tool::Tool],
    ) -> LlmRequest {
        LlmRequest {
            profile_id: cfg.profile_id.clone(),
            messages: messages.to_vec(),
            parameters: node_config.get("parameters").cloned(),
            generation: cfg.node_generation.clone(),
            tools: if tools.is_empty() {
                None
            } else {
                Some(tools.to_vec())
            },
            tool_call_protocol: cfg
                .tool_call_protocol
                .as_ref()
                .map(|config| config.format.clone()),
            locked_tool_call_protocol: cfg.tool_call_protocol.clone(),
            violation_policy: cfg.violation_policy.clone(),
            execution_id: Some(ctx.execution_id.to_string()),
            stream: None,
            dead_loop_detection: cfg.dead_loop_detection.clone(),
            protocol_auto_converted: None,
        }
    }

    async fn execute_inner(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> WorkflowResult<NodeExecutionResult> {
        let node_config = ctx.node_config.as_ref().unwrap_or(&Value::Null).clone();
        let cfg = parse_llm_node_config(ctx)?;
        let profile = self.gateway.profile_registry().get(&cfg.profile_id);
        let context_budget = wf_execution_shared::context_budget_from_profile(
            profile.as_ref().and_then(|p| p.context_window_size),
            profile.as_ref().and_then(|p| p.metadata.as_ref()),
        );
        if context_budget == 0 {
            tracing::warn!(
                profile_id = %cfg.profile_id,
                "no context window for profile: compression and preflight checks disabled"
            );
        }
        setup_token_tracker(
            ctx,
            &cfg.exec_config,
            cfg.token_tracking_enabled,
            context_budget,
        )
        .await;

        let mut messages = build_messages(ctx)?;
        let tools = resolve_tools(ctx)?;

        let mut executed_tool_calls: Vec<Value> = Vec::new();
        let mut final_response: Option<wf_types::llm::LlmResult> = None;
        let mut aggregated_content: Option<String> = None;

        // Multi-round tool loop: keep calling the model while it emits tool
        // calls, feeding tool results back, up to max_interactions
        // generations. Each response executes at most
        // max_tool_calls_per_request tool calls; excess calls fail with an
        // error result so the next generation sees what was skipped. When
        // the interaction budget is spent with tools just executed, the
        // node returns the collected tool record instead of failing: the
        // structured output carries every call, so nothing is truncated
        // silently.
        let mut stopped_with_tools = false;
        for _round in 0..cfg.max_interactions {
            if cfg.token_tracking_enabled {
                await_compression_settle(ctx).await;
            }
            let request = self.build_request(ctx, &cfg, &node_config, &messages, &tools);

            check_preflight_budget(ctx, &self.gateway, &request, cfg.token_tracking_enabled).await;

            if cfg.stream_enabled {
                let outcome = run_streaming_request(
                    ctx,
                    &self.gateway,
                    &request,
                    &cfg.profile_id,
                    cfg.token_tracking_enabled,
                    cfg.token_warning_threshold,
                )
                .await?;
                final_response = outcome.final_response;
                aggregated_content = outcome.aggregated_content;
                break;
            }

            let response = call_llm(ctx, &self.gateway, &request).await?;
            record_non_stream_usage(ctx, &request, &response, cfg.token_tracking_enabled).await;
            if cfg.token_tracking_enabled {
                emit_token_usage_events(ctx, cfg.token_warning_threshold, &request).await;
            }
            let has_tool_calls = response
                .tool_calls
                .as_ref()
                .is_some_and(|calls| !calls.is_empty());

            messages.push(response.message.clone());
            final_response = Some(response.clone());
            aggregated_content = response.content.clone();

            if !has_tool_calls {
                break;
            }

            let calls = response.tool_calls.unwrap_or_default();
            let run_now: &[LlmToolCall] = match cfg.max_tools_per_response {
                Some(cap) => {
                    let at = cap.min(calls.len() as u64) as usize;
                    &calls[..at]
                }
                None => &calls,
            };
            let skipped = &calls[run_now.len()..];
            let mut any_result = false;
            // One approval batch per response, mirroring the agent gate:
            // parallel calls share the batch id and see each other in the
            // pending queue instead of approving in isolation. Skipped
            // calls never reach approval or execution; they only record a
            // failure below.
            let pending_queue = pending_queue_for(run_now, ctx.tool_registry.as_ref());
            let batch_id = if run_now.len() > 1 {
                Some(wf_common::generate_id())
            } else {
                None
            };
            for (idx, call) in run_now.iter().enumerate() {
                let batch = batch_id.as_ref().map(|batch_id| LlmToolCallBatch {
                    batch_id: batch_id.clone(),
                    index: idx as u32,
                    total: run_now.len() as u32,
                    pending_queue: pending_queue.clone(),
                });
                let result_msg =
                    execute_tool_call(ctx, call, self.file_checkpoint.as_ref(), batch.as_ref())
                        .await;
                let is_error = result_msg
                    .metadata
                    .as_ref()
                    .and_then(|m| m.get("is_error"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                executed_tool_calls.push(serde_json::json!({
                    "id": call.id,
                    "name": call.function.name,
                    "success": !is_error,
                }));
                messages.push(result_msg);
                any_result = true;
            }
            if let Some(cap) = cfg.max_tools_per_response {
                for call in skipped {
                    let reason = format!(
                        "Tool \"{}\" was not executed: the response exceeds max_tool_calls_per_request ({}); only the first {} calls ran",
                        call.function.name, cap, cap,
                    );
                    messages.push(tool_result_message(
                        &call.id,
                        &call.function.name,
                        reason,
                        true,
                    ));
                    executed_tool_calls.push(serde_json::json!({
                        "id": call.id,
                        "name": call.function.name,
                        "success": false,
                        "skipped": true,
                    }));
                    any_result = true;
                }
            }
            if !any_result {
                break;
            }
            // The interaction budget covers model generations: when no
            // generation remains, the tools just executed are returned
            // instead of fed back. The structured output below carries
            // every call, so stopping here truncates nothing silently.
            if _round + 1 >= cfg.max_interactions {
                stopped_with_tools = true;
                break;
            }
        }

        let output = if stopped_with_tools {
            serde_json::json!({
                "content": aggregated_content.clone().unwrap_or_default(),
                "tool_calls": executed_tool_calls.clone(),
            })
        } else {
            aggregated_content
                .as_deref()
                .filter(|s| !s.is_empty())
                .map(|s| Value::String(s.to_string()))
                .unwrap_or(Value::Null)
        };

        let mut metadata = HashMap::new();
        if let Some(response) = &final_response {
            if !response.model.is_empty() {
                metadata.insert("model".to_string(), Value::String(response.model.clone()));
            }
            if let Some(finish_reason) = &response.finish_reason {
                metadata.insert(
                    "finish_reason".to_string(),
                    Value::String(finish_reason.clone()),
                );
            }
            if let Some(usage) = &response.usage {
                metadata.insert(
                    "prompt_tokens".to_string(),
                    Value::Number(usage.prompt_tokens.into()),
                );
                metadata.insert(
                    "completion_tokens".to_string(),
                    Value::Number(usage.completion_tokens.into()),
                );
            }
        }
        let executed_tool_count = executed_tool_calls.len();
        if !executed_tool_calls.is_empty() {
            metadata.insert("tool_calls".to_string(), Value::Array(executed_tool_calls));
        }
        metadata.insert("stream".to_string(), Value::Bool(cfg.stream_enabled));
        if stopped_with_tools {
            metadata.insert("rounds_exhausted".to_string(), Value::Bool(true));
        }

        // Write the assistant response to the configured output context
        // (the read context is left untouched so the compression chain can
        // replace it as a unit).
        if let (Some(response), Some(content)) = (&final_response, &aggregated_content) {
            if !content.is_empty() {
                let mut out_msg = response.message.clone();
                if out_msg.content == MessageContentValue::Text(String::new()) {
                    out_msg.content = MessageContentValue::Text(content.clone());
                }
                let output_context_id = node_config
                    .get("output_context")
                    .or_else(|| node_config.get("outputContext"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                if let Some(id) = output_context_id {
                    message_context::append_context(&ctx.variables, &id, vec![out_msg]);
                }
            }
        }

        let mut completion_meta = HashMap::from([
            (
                "event".to_string(),
                Value::String("llm_node_completed".to_string()),
            ),
            (
                "tool_call_count".to_string(),
                Value::Number(serde_json::Number::from(executed_tool_count as u64)),
            ),
        ]);
        if stopped_with_tools {
            completion_meta.insert("rounds_exhausted".to_string(), Value::Bool(true));
        }
        emit_llm_event(
            ctx.event_bus.as_deref(),
            EventType::NodeCustomEvent,
            ctx,
            completion_meta,
        );

        Ok(NodeExecutionResult {
            output,
            next_node_ids: Vec::new(),
            metadata,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::WorkflowError;

    #[test]
    fn handler_rejects_zero_interaction_budget() {
        let vars = std::sync::Arc::new(dashmap::DashMap::new());
        let ctx = NodeExecutionContext::new(
            wf_types::Id::new(),
            "llm1".to_string(),
            StaticNodeType::Llm,
            Value::Null,
            vars,
        )
        .with_node_config(serde_json::json!({
            "profile_id": "p",
            "max_interactions": 0,
        }));
        let err = parse_llm_node_config(&ctx).unwrap_err();
        assert!(err.to_string().contains("max_interactions"));
    }

    #[test]
    fn handler_requires_profile_id() {
        let vars = std::sync::Arc::new(dashmap::DashMap::new());
        let ctx = NodeExecutionContext::new(
            wf_types::Id::new(),
            "llm1".to_string(),
            StaticNodeType::Llm,
            Value::Null,
            vars,
        )
        .with_node_config(serde_json::json!({}));
        let err = parse_llm_node_config(&ctx).unwrap_err();
        assert!(err.to_string().contains("profile_id"));
        assert!(matches!(err, WorkflowError::OperationError(_)));
    }
}
