use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use wf_core::interruption::check_execution_interruption;
use wf_core::types::interruption::ExecutionInterruptionCheckResult;
use wf_execution_shared::hooks::HookRegistry;
use wf_execution_shared::types::execution_entity::ExecutionEntity;
use wf_llm::LlmGateway;
use wf_metrics::MetricsRegistry;
use wf_tools::registry::ToolRegistry;
use wf_types::agent_execution::{
    truncate_summary_preview, LlmCallRecord, LlmMessageSummary, LlmRequestSummary,
    LlmResponseSummary,
};
use wf_types::llm::{LlmRequest, MessageStreamEvent};
use wf_types::message::{LlmToolCall, Message, MessageContentValue, MessageRole};
use wf_types::tool::approval::ToolApprovalOptions;

use crate::agent_request::build_agent_request;
use crate::approval::ToolApprovalHandler;
use crate::coordinator::tool::ToolExecutionCoordinator;
use crate::entity::AgentLoopEntity;
use crate::error::{AgentError, AgentResult};
use crate::hook::AgentHookHandler;
use crate::stream::{AgentEventSink, AgentStreamEvent};

/// Publish the stream termination event (error vs abort) for the agent loop's
/// streaming LLM path. Consumer-layer publishing keeps wf-llm free of the
/// event bus dependency; the builders live in wf-llm.
fn publish_stream_termination(
    event_bus: Option<&wf_core::EventBus>,
    agent_loop_id: &str,
    profile_id: &str,
    aborted: bool,
    message: &str,
) {
    let Some(bus) = event_bus else { return };
    if aborted {
        let _ = bus.publish(wf_llm::build_llm_stream_aborted_event(
            agent_loop_id,
            Some(agent_loop_id),
            message,
            profile_id,
        ));
    } else {
        let _ = bus.publish(wf_llm::build_llm_stream_error_event(
            agent_loop_id,
            Some(agent_loop_id),
            message,
            profile_id,
        ));
    }
}

#[derive(Debug, Clone)]
pub struct IterationResult {
    pub should_continue: bool,
    pub content: Value,
    pub completion_data: Option<Value>,
    pub tool_call_count: u32,
}

/// Abstraction over a single agent iteration so the execution coordinator
/// can be driven by alternative iteration implementations (tests).
#[async_trait::async_trait]
pub trait IterationExecutor: Send + Sync {
    async fn execute_iteration(&self, entity: &AgentLoopEntity) -> AgentResult<IterationResult>;
}

#[async_trait::async_trait]
impl IterationExecutor for AgentIterationCoordinator {
    async fn execute_iteration(&self, entity: &AgentLoopEntity) -> AgentResult<IterationResult> {
        AgentIterationCoordinator::execute_iteration(self, entity).await
    }
}

/// How a single iteration talks to the LLM. Streaming is a transport-level
/// mode of the same iteration skeleton: deltas are forwarded to the event
/// sink while the final message is aggregated for tool call extraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IterationMode {
    Blocking,
    Streaming,
}

/// Default token warning threshold percentage of the configured limit.
pub const DEFAULT_TOKEN_WARNING_THRESHOLD: u32 = wf_llm::DEFAULT_TOKEN_WARNING_THRESHOLD;

/// Single iteration implementation shared by blocking and streaming runs.
pub struct AgentIterationCoordinator {
    gateway: Arc<LlmGateway>,
    tool_coordinator: ToolExecutionCoordinator,
    metrics: Option<Arc<MetricsRegistry>>,
    mode: IterationMode,
    event_sink: Option<AgentEventSink>,
    event_bus: Option<Arc<wf_core::EventBus>>,
    /// Shared hook receiver registry; hook points dispatch through it.
    hook_registry: Option<Arc<HookRegistry>>,
    token_warning_threshold: u32,
    /// Token usage tracking; disabled only by an explicit config switch.
    token_tracking_enabled: bool,
    /// Assembly-time rendered description for the `general` tool (resource
    /// template override); `None` keeps the builtin static description.
    general_description: Option<String>,
    /// Assembly-time pre-rendered discoverable-tool metadata block (resource
    /// template override); `None` falls back to built-in generation at
    /// request assembly time.
    discoverable_metadata_block: Option<String>,
}

impl AgentIterationCoordinator {
    pub fn new(
        gateway: Arc<LlmGateway>,
        tool_registry: Arc<ToolRegistry>,
        metrics: Option<Arc<MetricsRegistry>>,
    ) -> Self {
        let tool_coordinator =
            ToolExecutionCoordinator::new(tool_registry).with_metrics(metrics.clone());
        Self {
            gateway,
            tool_coordinator,
            metrics,
            mode: IterationMode::Blocking,
            event_sink: None,
            event_bus: None,
            hook_registry: None,
            token_warning_threshold: DEFAULT_TOKEN_WARNING_THRESHOLD,
            token_tracking_enabled: true,
            general_description: None,
            discoverable_metadata_block: None,
        }
    }

    /// Register the tool approval configuration passed down to the tool
    /// execution coordinator.
    pub fn with_approval(
        mut self,
        options: Option<ToolApprovalOptions>,
        handler: Option<Arc<dyn ToolApprovalHandler>>,
    ) -> Self {
        let registry = self.tool_coordinator.tool_registry().clone();
        self.tool_coordinator = ToolExecutionCoordinator::new(registry)
            .with_event_bus(self.event_bus.clone())
            .with_metrics(self.metrics.clone())
            .with_approval(options, handler);
        self
    }

    /// Gate tool visibility at execution time (blocks only intercept; the
    /// schema is assembled independently).
    pub fn with_visibility_store(
        mut self,
        store: Option<Arc<dyn crate::coordinator::tool::ToolVisibilityStore>>,
    ) -> Self {
        // Rebuilding the tool coordinator must preserve the approval
        // wiring applied by `with_approval`, otherwise every tool call is
        // auto-approved downstream.
        let registry = self.tool_coordinator.tool_registry().clone();
        let (approval_options, approval_handler) = self.tool_coordinator.approval_config();
        self.tool_coordinator = ToolExecutionCoordinator::new(registry)
            .with_event_bus(self.event_bus.clone())
            .with_metrics(self.metrics.clone())
            .with_approval(approval_options, approval_handler)
            .with_visibility_store(store);
        self
    }

    /// Switch the coordinator to streaming mode and attach the event sink
    /// deltas and tool lifecycle events are forwarded to.
    pub fn with_streaming(mut self, sink: AgentEventSink) -> Self {
        self.mode = IterationMode::Streaming;
        self.event_sink = Some(sink);
        self
    }

    /// Attach the event bus token usage and hook events are published to.
    /// Forwarded to the tool execution coordinator (tool-call hooks publish
    /// through it).
    pub fn with_event_bus(mut self, event_bus: Arc<wf_core::EventBus>) -> Self {
        self.event_bus = Some(event_bus.clone());
        self.tool_coordinator = self.tool_coordinator.with_event_bus(Some(event_bus));
        self.tool_coordinator = self
            .tool_coordinator
            .with_hook_registry(self.hook_registry.clone());
        self
    }

    /// Inject the shared hook receiver registry: every hook point dispatches
    /// through it (synchronous receiver notification + audit event).
    pub fn with_hook_registry(mut self, registry: Option<Arc<HookRegistry>>) -> Self {
        self.hook_registry = registry;
        self
    }

    /// Token warning threshold percentage of the configured limit.
    pub fn with_token_warning_threshold(mut self, threshold_percentage: u32) -> Self {
        self.token_warning_threshold = threshold_percentage;
        self
    }

    /// Switch token usage tracking off (usage recording and token events).
    /// Defaults to enabled; callers pass the resolved config value
    /// (`enable_token_tracking.unwrap_or(true)`).
    pub fn with_token_tracking_enabled(mut self, enabled: bool) -> Self {
        self.token_tracking_enabled = enabled;
        self
    }

    /// Override the `general` tool description with the assembly-time
    /// rendered text (resource template override).
    pub fn with_general_description(mut self, description: Option<String>) -> Self {
        self.general_description = description;
        self
    }

    /// Override the discoverable-tool metadata block with the assembly-time
    /// rendered text (resource template override); `None` falls back to
    /// built-in generation at request assembly time.
    pub fn with_discoverable_metadata_block(mut self, block: Option<String>) -> Self {
        self.discoverable_metadata_block = block;
        self
    }

    /// Inject the run's `general` tool invoker into the tool execution
    /// coordinator, so every execution context built afterwards carries it.
    /// Call once per run; the coordinator is rebuilt per run, so no
    /// unregister step exists.
    pub fn set_general_invoker(&self, entity: Arc<AgentLoopEntity>) {
        let context = Arc::new(crate::coordinator::tool::GeneralToolContext::new(
            self.tool_coordinator.execution_ctx(),
            entity.clone(),
            self.event_bus.clone(),
        ));
        self.tool_coordinator.set_general_invoker(context);
    }

    fn is_streaming(&self) -> bool {
        self.mode == IterationMode::Streaming
    }

    /// Read the conversation position at the current boundary: message count
    /// and ledger version, published as the turn anchor on iteration events.
    async fn conversation_anchor(&self, entity: &AgentLoopEntity) -> (usize, u64) {
        let conversation = entity.conversation().read().await;
        (
            conversation.messages().len(),
            conversation.conversation_version(),
        )
    }

    pub async fn execute_iteration(
        &self,
        entity: &AgentLoopEntity,
    ) -> AgentResult<IterationResult> {
        let execution_id = entity.id().clone();

        AgentHookHandler::emit_agent_hooks(
            entity,
            "BEFORE_ITERATION",
            HashMap::new(),
            self.hook_registry.as_deref(),
            self.event_bus.as_deref(),
        )
        .await;

        entity.state.write().await.start_iteration();

        if self.is_streaming() {
            if let Some(ref sink) = self.event_sink {
                let iteration = entity.state.read().await.current_iteration();
                let (message_count, array_version) = self.conversation_anchor(entity).await;
                sink.emit(
                    entity.id(),
                    AgentStreamEvent::IterationStart {
                        iteration,
                        message_count,
                        array_version,
                    },
                )
                .await?;
            }
        }

        if let Some(result) = self.interrupted(entity, 0).await {
            return Ok(result);
        }

        AgentHookHandler::emit_agent_hooks(
            entity,
            "BEFORE_LLM_CALL",
            HashMap::new(),
            self.hook_registry.as_deref(),
            self.event_bus.as_deref(),
        )
        .await;

        let request = build_agent_request(
            entity,
            self.tool_coordinator.tool_registry(),
            self.is_streaming(),
            self.general_description.as_deref(),
            self.discoverable_metadata_block.as_deref(),
        )
        .await?;

        // Pre-request token budget check: the estimate is approximate, so this
        // is a warning only (never blocks the request); the provider's real
        // count takes precedence. Fires at most once per session.
        if self.token_tracking_enabled {
            if let Some(ref bus) = self.event_bus {
                let mut conversation = entity.conversation().write().await;
                let token_limit = conversation.token_limit();
                if token_limit > 0 {
                    let estimated = u64::from(wf_llm::estimate_request_tokens(&request));
                    if estimated > token_limit && conversation.consume_preflight_warning() {
                        let _ = bus.publish(wf_llm::build_token_usage_warning_event(
                            &execution_id,
                            Some(entity.id()),
                            estimated,
                            token_limit,
                            estimated as f64 / token_limit as f64 * 100.0,
                        ));
                    }
                }
            }
        }

        if let Some(ref metrics) = self.metrics {
            if let Some(format) = entity.tool_call_format() {
                metrics
                    .agent_loop()
                    .record_protocol_locked(&format.format.to_string());
            }
        }

        let (assistant_msg, llm_content, finish_reason, request_usage) = match self.mode {
            IterationMode::Blocking => {
                let started_at = wf_common::now();
                // Publish the request event before the gateway call.
                self.emit_llm_requested(entity, &request).await;
                let llm_result = match self
                    .gateway
                    .generate(&request, Some(entity.get_abort_signal()))
                    .await
                {
                    Ok(result) => result,
                    Err(e) if e.is_context_length_exceeded() => {
                        // The failed call stays on the audit trail.
                        self.persist_llm_call(
                            entity,
                            &request,
                            llm_call_record(
                                &request,
                                started_at,
                                None,
                                None,
                                0,
                                0,
                                Some(e.to_string()),
                            ),
                        )
                        .await;
                        // Safety-net path: the provider rejected the
                        // actual request; force a compression event over the
                        // real messages so the chain fires even though the
                        // estimate undercounted.
                        self.publish_forced_compression(entity, &request).await;
                        return Err(e.into());
                    }
                    Err(e) => {
                        // Record the failed call on the audit trail.
                        self.persist_llm_call(
                            entity,
                            &request,
                            llm_call_record(
                                &request,
                                started_at,
                                None,
                                None,
                                0,
                                0,
                                Some(e.to_string()),
                            ),
                        )
                        .await;
                        return Err(e.into());
                    }
                };
                let usage = llm_result.usage.as_ref().map(wf_llm::RequestUsage::from);
                // Record the completed call (request/response summaries).
                let content = text_of(&llm_result.message.content);
                let response_summary = build_response_summary(
                    &content,
                    llm_result.tool_calls.as_deref().unwrap_or_default(),
                    llm_result.finish_reason.clone(),
                );
                let model = if llm_result.model.is_empty() {
                    None
                } else {
                    Some(llm_result.model.clone())
                };
                self.persist_llm_call(
                    entity,
                    &request,
                    llm_call_record(
                        &request,
                        started_at,
                        model,
                        Some(response_summary),
                        usage.as_ref().map(|u| u.prompt_tokens).unwrap_or(0),
                        usage.as_ref().map(|u| u.completion_tokens).unwrap_or(0),
                        None,
                    ),
                )
                .await;
                (
                    llm_result.message.clone(),
                    llm_result.content.clone(),
                    llm_result.finish_reason.clone(),
                    usage,
                )
            }
            IterationMode::Streaming => self.stream_llm_call(entity, &request).await?,
        };

        // Record token usage into the conversation session (v2 dual-track
        // semantics): the decision track always accumulates the local request
        // estimate (warnings / limit / compression never depend on provider
        // usage); the cost track records the real usage — or an estimated
        // marker when the provider reported none — for accounting only.
        // Skipped entirely when token tracking is disabled by config.
        if self.token_tracking_enabled {
            let mut conversation = entity.conversation().write().await;
            let prompt_est = wf_llm::estimate_request_tokens(&request);
            let completion_est = wf_llm::estimate_tokens(&text_of(&assistant_msg.content)) as u32;
            if let Some(usage) = &request_usage {
                conversation.accumulate_stream_usage(usage);
                conversation.accumulate_estimated_usage(prompt_est, completion_est);
            } else {
                conversation.update_estimated_usage(prompt_est, completion_est);
            }
            conversation.finalize_current_request();

            // Emit token usage events (decision track only): a single-shot
            // warning at the threshold crossing, then limit exceeded once per
            // 50% tier band, then compression requested when the conversation
            // array's ledger estimate exceeds the limit.
            if let Some(ref bus) = self.event_bus {
                let tokens_used = conversation.estimated_total();
                let token_limit = conversation.token_limit();
                if token_limit > 0 {
                    if conversation.consume_token_warning(self.token_warning_threshold as f64) {
                        let percentage = conversation.usage_percentage().unwrap_or(0.0);
                        let _ = bus.publish(wf_llm::build_token_usage_warning_event(
                            &execution_id,
                            Some(entity.id()),
                            tokens_used,
                            token_limit,
                            percentage,
                        ));
                    }
                    if conversation.consume_limit_exceeded_tier().is_some() {
                        let _ = bus.publish(wf_llm::build_token_limit_exceeded_event(
                            &execution_id,
                            Some(entity.id()),
                            tokens_used,
                            token_limit,
                        ));
                    }
                    let estimated = conversation.estimated_conversation_tokens();
                    let version = conversation.conversation_version();
                    if estimated > token_limit && conversation.should_emit_compression(version) {
                        let message_count = conversation.messages().len();
                        let messages = conversation.messages().to_vec();
                        let request = wf_llm::ContextCompressionRequest {
                            target_context_id: wf_llm::CONVERSATION_CONTEXT_ID,
                            tokens_used: estimated,
                            token_limit,
                            message_count,
                            array_version: version,
                            forced: false,
                            messages: &messages,
                        };
                        // The event-bus copy stays the audit / persistence /
                        // user-rule channel; delivery is the synchronous hook
                        // dispatch (the compression service takes over here).
                        let _ = bus.publish(wf_llm::build_context_compression_requested_event(
                            &execution_id,
                            Some(entity.id()),
                            request.target_context_id,
                            request.tokens_used,
                            request.token_limit,
                            request.message_count,
                            request.array_version,
                            request.forced,
                            Some(request.messages),
                        ));
                        self.dispatch_compression(entity, &request).await;
                        conversation.mark_compression_emitted(version);
                    }
                }
            }
        }

        if let Some(result) = self.interrupted(entity, 0).await {
            return Ok(result);
        }

        let mut hook_data = HashMap::new();
        hook_data.insert(
            "llm_content".to_string(),
            llm_content
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null),
        );
        hook_data.insert(
            "finish_reason".to_string(),
            Value::String(finish_reason.unwrap_or_default()),
        );
        AgentHookHandler::emit_agent_hooks(
            entity,
            "AFTER_LLM_CALL",
            hook_data,
            self.hook_registry.as_deref(),
            self.event_bus.as_deref(),
        )
        .await;

        let has_tool_calls = assistant_msg
            .tool_calls
            .as_ref()
            .map(|c| !c.is_empty())
            .unwrap_or(false);
        entity
            .conversation()
            .write()
            .await
            .add_message(assistant_msg.clone());

        if !has_tool_calls {
            let content = text_of(&assistant_msg.content);
            return self.finish_iteration(entity, content, None, 0, false).await;
        }

        let tool_calls = assistant_msg.tool_calls.unwrap_or_default();
        let tool_messages = match self.mode {
            IterationMode::Blocking => {
                self.tool_coordinator
                    .execute_tool_calls(entity, &tool_calls)
                    .await?
            }
            IterationMode::Streaming => {
                self.execute_tool_calls_streaming(entity, &tool_calls)
                    .await?
            }
        };
        let tool_call_count = tool_calls.len() as u32;

        if let Some(ref metrics) = self.metrics {
            metrics
                .agent_loop()
                .record_tool_calls(tool_call_count as u64);
        }

        if let Some(result) = self.interrupted(entity, tool_call_count).await {
            return Ok(result);
        }

        let mut completion_data = None;
        for tc in &tool_calls {
            if tc.function.name == "attempt_completion" {
                completion_data = Some(Value::String(tc.function.arguments.clone()));
            }
        }

        for msg in &tool_messages {
            entity.conversation().write().await.add_message(msg.clone());
        }

        let content = text_of(&assistant_msg.content);
        let should_continue = completion_data.is_none();
        self.finish_iteration(
            entity,
            content,
            completion_data,
            tool_call_count,
            should_continue,
        )
        .await
    }

    /// Check for an interruption after an LLM call or tool execution; when
    /// interrupted the iteration is closed and a terminal result returned.
    async fn interrupted(
        &self,
        entity: &AgentLoopEntity,
        tool_call_count: u32,
    ) -> Option<IterationResult> {
        let interruption = check_execution_interruption(
            entity.interruption(),
            Some(entity.state.read().await.current_iteration()),
        );
        if matches!(interruption, ExecutionInterruptionCheckResult::Continue) {
            return None;
        }

        entity.state.write().await.end_iteration();
        if self.is_streaming() {
            if let Some(ref sink) = self.event_sink {
                let iteration = entity.state.read().await.current_iteration();
                let (message_count, array_version) = self.conversation_anchor(entity).await;
                let _ = sink
                    .emit(
                        entity.id(),
                        AgentStreamEvent::IterationEnd {
                            iteration,
                            message_count,
                            array_version,
                        },
                    )
                    .await;
            }
        }
        Some(IterationResult {
            should_continue: false,
            content: Value::String("Execution interrupted".to_string()),
            completion_data: None,
            tool_call_count,
        })
    }

    /// Close the iteration (state, stream events, AFTER_ITERATION hook) and
    /// assemble the result.
    async fn finish_iteration(
        &self,
        entity: &AgentLoopEntity,
        content: String,
        completion_data: Option<Value>,
        tool_call_count: u32,
        should_continue: bool,
    ) -> AgentResult<IterationResult> {
        entity
            .state
            .write()
            .await
            .end_iteration_with_content(Some(content.clone()));

        if self.is_streaming() {
            if let Some(ref sink) = self.event_sink {
                let iteration = entity.state.read().await.current_iteration();
                let (message_count, array_version) = self.conversation_anchor(entity).await;
                sink.emit(
                    entity.id(),
                    AgentStreamEvent::IterationEnd {
                        iteration,
                        message_count,
                        array_version,
                    },
                )
                .await?;
            }
        }

        AgentHookHandler::emit_agent_hooks(
            entity,
            "AFTER_ITERATION",
            HashMap::new(),
            self.hook_registry.as_deref(),
            self.event_bus.as_deref(),
        )
        .await;

        Ok(IterationResult {
            should_continue,
            content: Value::String(content),
            completion_data,
            tool_call_count,
        })
    }

    /// Safety-net path: emit a forced CONTEXT_COMPRESSION_REQUESTED over
    /// the actual request messages when the provider rejected them with a
    /// context-length-exceeded error.
    async fn publish_forced_compression(&self, entity: &AgentLoopEntity, request: &LlmRequest) {
        let Some(ref bus) = self.event_bus else {
            return;
        };
        let mut conversation = entity.conversation().write().await;
        let version = conversation.conversation_version();
        let token_limit = conversation.token_limit();
        let tokens_used = u64::from(wf_llm::estimate_request_tokens(request));
        let messages = request.messages.clone();
        let compression_request = wf_llm::ContextCompressionRequest {
            target_context_id: wf_llm::CONVERSATION_CONTEXT_ID,
            tokens_used,
            token_limit,
            message_count: request.messages.len(),
            array_version: version,
            forced: true,
            messages: &messages,
        };
        let _ = bus.publish(wf_llm::build_context_compression_requested_event(
            &entity.id().clone(),
            Some(entity.id()),
            compression_request.target_context_id,
            compression_request.tokens_used,
            compression_request.token_limit,
            compression_request.message_count,
            compression_request.array_version,
            compression_request.forced,
            Some(compression_request.messages),
        ));
        self.dispatch_compression(entity, &compression_request).await;
        conversation.mark_compression_emitted(version);
    }

    /// Dispatch the `CONTEXT_COMPRESSION_REQUESTED` engine signal: registered
    /// receivers (the compression service) are notified synchronously so the
    /// summary sub-workflow takes over immediately; the event-bus audit copy
    /// is published by the caller.
    async fn dispatch_compression(
        &self,
        entity: &AgentLoopEntity,
        request: &wf_llm::ContextCompressionRequest<'_>,
    ) {
        let Some(registry) = &self.hook_registry else {
            return;
        };
        use wf_execution_shared::hooks::{dispatch, HookContext};
        let mut data = wf_llm::compression_request_hook_data(request);
        // Agent-owned target: the agent conversation consumes the completed
        // event itself (no registry write-back).
        data.insert(
            "agent_loop_id".to_string(),
            Value::String(entity.id().to_string()),
        );
        dispatch(
            registry,
            &[],
            wf_llm::token_events::COMPRESSION_SIGNAL_HOOK_TYPE,
            &HookContext {
                execution_id: entity.id().clone(),
                hook_type: wf_llm::token_events::COMPRESSION_SIGNAL_HOOK_TYPE.to_string(),
                data,
            },
            self.event_bus.as_deref(),
        )
        .await;
    }

    /// Publish the LLM_REQUESTED event before the gateway call.
    async fn emit_llm_requested(&self, entity: &AgentLoopEntity, request: &LlmRequest) {
        let Some(ref bus) = self.event_bus else {
            return;
        };
        let _ = bus.publish(wf_llm::build_llm_requested_event(
            entity.id(),
            Some(entity.id()),
            &request.profile_id,
            request.messages.len(),
            request.tools.as_ref().map(|tools| tools.len()).unwrap_or(0),
        ));
    }

    /// Single collection point: append the LLM call to the
    /// entity state audit trail and publish the finished/failed event. The
    /// call record carries request/response summaries; the events give the
    /// online timeline the same visibility.
    async fn persist_llm_call(
        &self,
        entity: &AgentLoopEntity,
        request: &LlmRequest,
        call: LlmCallRecord,
    ) {
        entity.state.write().await.record_llm_call(call.clone());
        let Some(ref bus) = self.event_bus else {
            return;
        };
        if let Some(error) = &call.error {
            let _ = bus.publish(wf_llm::build_llm_failed_event(
                entity.id(),
                Some(entity.id()),
                error,
                &request.profile_id,
            ));
        } else {
            let _ = bus.publish(wf_llm::build_llm_responded_event(
                entity.id(),
                Some(entity.id()),
                &request.profile_id,
                call.model.as_deref(),
                call.prompt_tokens,
                call.completion_tokens,
            ));
        }
    }

    /// Streaming LLM call: forward deltas to the event sink while
    /// aggregating the final message for tool call extraction.
    ///
    /// The call is recorded on the entity state audit trail in every
    /// terminal outcome (success, stream error, abort, missing final
    /// message), so long streams leave a complete audit record.
    async fn stream_llm_call(
        &self,
        entity: &AgentLoopEntity,
        request: &LlmRequest,
    ) -> AgentResult<(
        Message,
        Option<String>,
        Option<String>,
        Option<wf_llm::RequestUsage>,
    )> {
        let started_at = wf_common::now();
        // Publish the request event before the stream opens.
        self.emit_llm_requested(entity, request).await;
        let mut stream = match self
            .gateway
            .generate_stream(request, Some(entity.get_abort_signal()))
            .await
        {
            Ok(stream) => stream,
            Err(e) => {
                self.persist_llm_call(
                    entity,
                    request,
                    llm_call_record(request, started_at, None, None, 0, 0, Some(e.to_string())),
                )
                .await;
                return Err(e.into());
            }
        };
        let mut final_message: Option<Message> = None;
        let mut request_usage: Option<wf_llm::RequestUsage> = None;
        let mut content_parts: Vec<String> = Vec::new();
        let mut failure: Option<wf_llm::LlmError> = None;

        loop {
            let Some(event) = stream.next().await else {
                break;
            };
            match event {
                Ok(MessageStreamEvent::Text(t)) => {
                    content_parts.push(t.text.clone());
                    if let Some(ref sink) = self.event_sink {
                        sink.emit_quiet(
                            entity.id(),
                            AgentStreamEvent::LlmDelta { content: t.text },
                        )
                        .await;
                    }
                }
                Ok(MessageStreamEvent::Stream(chunk)) => {
                    content_parts.push(chunk.content.clone());
                    if let Some(ref sink) = self.event_sink {
                        sink.emit_quiet(
                            entity.id(),
                            AgentStreamEvent::LlmDelta {
                                content: chunk.content,
                            },
                        )
                        .await;
                    }
                }
                Ok(MessageStreamEvent::ReasoningText(reasoning)) => {
                    content_parts.push(reasoning.reasoning.clone());
                    if let Some(ref sink) = self.event_sink {
                        sink.emit_quiet(
                            entity.id(),
                            AgentStreamEvent::LlmDelta {
                                content: reasoning.reasoning,
                            },
                        )
                        .await;
                    }
                }
                Ok(MessageStreamEvent::Message(msg)) => {
                    final_message = Some(msg.message);
                }
                Ok(MessageStreamEvent::FinalMessage(msg)) => {
                    let content = text_of(&msg.message.content);
                    final_message = Some(msg.message);
                    content_parts.push(content);
                    if let Some(usage) = msg.usage {
                        request_usage = Some(wf_llm::RequestUsage::from(&usage));
                    }
                }
                Ok(MessageStreamEvent::Usage(u)) => {
                    // Merge mid-stream usage deltas into the current request
                    let usage = wf_llm::RequestUsage::from(&u.usage);
                    match &mut request_usage {
                        Some(acc) => {
                            acc.merge_non_zero(&usage);
                        }
                        None => request_usage = Some(usage),
                    }
                }
                Ok(MessageStreamEvent::Error(e)) => {
                    publish_stream_termination(
                        self.event_bus.as_deref(),
                        entity.id(),
                        &request.profile_id,
                        false,
                        &e.error,
                    );
                    if wf_llm::LlmError::StreamError(e.error.clone()).is_context_length_exceeded() {
                        self.publish_forced_compression(entity, request).await;
                    }
                    failure = Some(wf_llm::error::LlmError::StreamError(e.error));
                    break;
                }
                Ok(MessageStreamEvent::Abort(a)) => {
                    publish_stream_termination(
                        self.event_bus.as_deref(),
                        entity.id(),
                        &request.profile_id,
                        true,
                        &a.reason,
                    );
                    failure = Some(wf_llm::error::LlmError::StreamError(a.reason));
                    break;
                }
                Ok(MessageStreamEvent::End(_))
                | Ok(MessageStreamEvent::Connect(_))
                | Ok(MessageStreamEvent::InputJson(_))
                | Ok(MessageStreamEvent::ToolCallDelta(_)) => {}
                Err(e) => {
                    publish_stream_termination(
                        self.event_bus.as_deref(),
                        entity.id(),
                        &request.profile_id,
                        wf_llm::is_stream_abort(&e),
                        &e.to_string(),
                    );
                    if e.is_context_length_exceeded() {
                        self.publish_forced_compression(entity, request).await;
                    }
                    failure = Some(e);
                    break;
                }
            }
        }

        if let Some(error) = failure {
            self.persist_llm_call(
                entity,
                request,
                llm_call_record(
                    request,
                    started_at,
                    None,
                    None,
                    0,
                    0,
                    Some(error.to_string()),
                ),
            )
            .await;
            return Err(AgentError::LlmError(error));
        }

        let Some(assistant_msg) = final_message else {
            let message = "stream ended without a final message".to_string();
            self.persist_llm_call(
                entity,
                request,
                llm_call_record(request, started_at, None, None, 0, 0, Some(message.clone())),
            )
            .await;
            return Err(AgentError::LlmError(wf_llm::error::LlmError::StreamError(
                message,
            )));
        };

        let content = text_of(&assistant_msg.content);
        let response_summary = build_response_summary(
            &content,
            assistant_msg.tool_calls.as_deref().unwrap_or_default(),
            None,
        );
        self.persist_llm_call(
            entity,
            request,
            llm_call_record(
                request,
                started_at,
                None,
                Some(response_summary),
                request_usage.as_ref().map(|u| u.prompt_tokens).unwrap_or(0),
                request_usage
                    .as_ref()
                    .map(|u| u.completion_tokens)
                    .unwrap_or(0),
                None,
            ),
        )
        .await;
        Ok((assistant_msg, Some(content), None, request_usage))
    }

    /// Streaming tool execution: run each call sequentially and forward
    /// ToolStart/ToolEnd lifecycle events. Every call passes the approval
    /// gate first (same pipeline as the sequential executor), so a denied
    /// call surfaces as a failed ToolEnd without executing.
    async fn execute_tool_calls_streaming(
        &self,
        entity: &AgentLoopEntity,
        tool_calls: &[LlmToolCall],
    ) -> AgentResult<Vec<Message>> {
        let mut tool_messages = Vec::with_capacity(tool_calls.len());
        for tc in tool_calls {
            if let Some(ref sink) = self.event_sink {
                sink.emit(
                    entity.id(),
                    AgentStreamEvent::ToolStart {
                        tool_call_id: tc.id.clone(),
                        tool_name: tc.function.name.clone(),
                    },
                )
                .await?;
            }

            let msg = match self
                .tool_coordinator
                .approve_single_for_stream(entity, tc)
                .await
            {
                Some(rejection) => rejection,
                None => {
                    self.tool_coordinator
                        .execute_single_tool_for_stream(entity, tc)
                        .await
                }
            };
            let result_text = text_of(&msg.content);

            if let Some(ref sink) = self.event_sink {
                sink.emit(
                    entity.id(),
                    AgentStreamEvent::ToolEnd {
                        tool_call_id: tc.id.clone(),
                        tool_name: tc.function.name.clone(),
                        success: !result_text.contains("\"error\""),
                        result: result_text.clone(),
                    },
                )
                .await?;
            }
            tool_messages.push(msg);
        }
        Ok(tool_messages)
    }
}

fn text_of(content: &MessageContentValue) -> String {
    match content {
        MessageContentValue::Text(t) => t.clone(),
        MessageContentValue::Rich(_) => String::new(),
    }
}

fn role_str(role: &MessageRole) -> &'static str {
    match role {
        MessageRole::System => "system",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::Tool => "tool",
    }
}

/// Summarize one message (role + truncated content preview).
fn summarize_message(message: &Message, truncated: &mut bool) -> LlmMessageSummary {
    let (preview, preview_truncated) = truncate_summary_preview(&text_of(&message.content));
    *truncated |= preview_truncated;
    LlmMessageSummary {
        role: role_str(&message.role).to_string(),
        preview,
        truncated: preview_truncated.then_some(true),
    }
}

/// LLM request payload summary with truncation markers.
fn build_request_summary(request: &LlmRequest) -> LlmRequestSummary {
    let mut truncated = false;
    let first_message = request
        .messages
        .first()
        .map(|m| summarize_message(m, &mut truncated));
    let last_message = if request.messages.len() > 1 {
        request
            .messages
            .last()
            .map(|m| summarize_message(m, &mut truncated))
    } else {
        None
    };
    LlmRequestSummary {
        message_count: request.messages.len() as u32,
        first_message,
        last_message,
        tool_count: request
            .tools
            .as_ref()
            .map(|tools| tools.len() as u32)
            .unwrap_or(0),
        parameter_count: request
            .parameters
            .as_ref()
            .and_then(|p| p.as_object())
            .map(|params| params.len() as u32)
            .unwrap_or(0),
        truncated: truncated.then_some(true),
    }
}

/// LLM response summary with content preview truncation.
fn build_response_summary(
    content: &str,
    tool_calls: &[LlmToolCall],
    finish_reason: Option<String>,
) -> LlmResponseSummary {
    let (preview, truncated) = truncate_summary_preview(content);
    LlmResponseSummary {
        content_preview: if preview.is_empty() {
            None
        } else {
            Some(preview)
        },
        truncated: truncated.then_some(true),
        tool_call_count: tool_calls.len() as u32,
        finish_reason,
    }
}

/// Assemble one LLM call audit record. `seq` is assigned by the
/// state (`AgentLoopState::record_llm_call`).
fn llm_call_record(
    request: &LlmRequest,
    started_at: i64,
    model: Option<String>,
    response_summary: Option<LlmResponseSummary>,
    prompt_tokens: u32,
    completion_tokens: u32,
    error: Option<String>,
) -> LlmCallRecord {
    let completed_at = wf_common::now();
    LlmCallRecord {
        seq: 0,
        profile_id: request.profile_id.clone(),
        model,
        request_summary: Some(build_request_summary(request)),
        response_summary,
        prompt_tokens,
        completion_tokens,
        started_at,
        completed_at: Some(completed_at),
        duration_ms: completed_at - started_at,
        error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use tokio::sync::mpsc;
    use wf_llm::mock::MockLlmClient;
    use wf_types::message::{LlmFunctionCall, LlmToolCall};
    use wf_types::Id;

    fn tool_call_message(tool_call_id: &str) -> Message {
        Message {
            id: Id::from(wf_common::generate_id()),
            role: wf_types::message::MessageRole::Assistant,
            content: MessageContentValue::Text("using tool".to_string()),
            timestamp: wf_common::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: Some(vec![LlmToolCall {
                id: tool_call_id.to_string(),
                r#type: "function".to_string(),
                function: LlmFunctionCall {
                    name: "mock_write".to_string(),
                    arguments: "{}".to_string(),
                },
            }]),
            thinking: None,
            metadata: None,
        }
    }

    fn text_message(content: &str) -> Message {
        Message {
            id: Id::from(wf_common::generate_id()),
            role: wf_types::message::MessageRole::Assistant,
            content: MessageContentValue::Text(content.to_string()),
            timestamp: wf_common::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        }
    }

    fn mock_tool_registry(
        executed: &Arc<std::sync::atomic::AtomicU32>,
    ) -> Arc<wf_tools::registry::ToolRegistry> {
        use std::sync::atomic::Ordering;
        let registry = Arc::new(wf_tools::registry::ToolRegistry::new());
        let handler: wf_tools::executor::stateless::StatelessHandler = {
            let executed = executed.clone();
            Arc::new(
                move |_p: &Value, _c: &wf_tools::executor::trait_def::ToolExecutionContext| {
                    executed.fetch_add(1, Ordering::SeqCst);
                    Ok(Value::from("stream-tool-ok"))
                },
            )
        };
        registry.register_tool(wf_types::tool::Tool {
            id: "tool-1".to_string(),
            name: "mock_write".to_string(),
            description: "mock".to_string(),
            tool_type: wf_types::tool::ToolType::Stateless,
            parameters: None,
            metadata: None,
            config: None,
            enabled: Some(true),
            strict: None,
            default_timeout_ms: Some(5000),
        });
        registry.register_stateless_handler("tool-1", handler);
        registry
    }

    fn stream_gateway(mock: Arc<MockLlmClient>) -> Arc<LlmGateway> {
        let gateway = LlmGateway::new();
        gateway.register_mock("mock", mock);
        Arc::new(gateway)
    }

    async fn run_streaming(
        gateway: Arc<LlmGateway>,
        registry: Arc<wf_tools::registry::ToolRegistry>,
        entity: &AgentLoopEntity,
    ) -> (
        AgentResult<IterationResult>,
        mpsc::Receiver<AgentStreamEvent>,
    ) {
        let (tx, rx) = mpsc::channel(32);
        let coordinator = AgentIterationCoordinator::new(gateway, registry, None)
            .with_streaming(AgentEventSink::new(tx, None));
        (coordinator.execute_iteration(entity).await, rx)
    }

    #[tokio::test]
    async fn test_stream_events_text_only() {
        let executed = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let registry = mock_tool_registry(&executed);
        let mock = Arc::new(MockLlmClient::new());
        mock.script_stream(vec![
            MessageStreamEvent::Text(wf_types::llm::MessageStreamText {
                text: "hello ".to_string(),
                snapshot: String::new(),
            }),
            MessageStreamEvent::Text(wf_types::llm::MessageStreamText {
                text: "world".to_string(),
                snapshot: String::new(),
            }),
            MessageStreamEvent::FinalMessage(wf_types::llm::MessageStreamFinal {
                message: text_message("hello world"),
                usage: None,
                stream_stats: None,
            }),
            MessageStreamEvent::End(wf_types::llm::MessageStreamEnd {}),
        ]);
        let entity = AgentLoopEntity::new(Id::from("agent-stream-1".to_string()))
            .with_model("mock".to_string());
        entity.state.write().await.start().unwrap();

        let (result, mut rx) = run_streaming(stream_gateway(mock), registry, &entity).await;
        let result = result.expect("stream iteration must succeed");

        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }
        assert_eq!(events.len(), 4); // IterationStart + 2 deltas + IterationEnd
                                     // The iteration boundary carries the conversation turn anchor
                                     // (message_count / array_version) for trigger conditions and
                                     // nested-agent input slicing.
        match &events[0] {
            AgentStreamEvent::IterationStart {
                iteration,
                message_count: 0,
                array_version: 0,
                ..
            } => assert_eq!(*iteration, 1),
            other => panic!("expected IterationStart, got {:?}", other),
        }
        assert!(
            matches!(&events[1], AgentStreamEvent::LlmDelta { content } if content == "hello ")
        );
        assert!(matches!(&events[2], AgentStreamEvent::LlmDelta { content } if content == "world"));
        // The iteration appended one assistant message: the end boundary
        // anchor reflects the conversation after the iteration.
        match &events[3] {
            AgentStreamEvent::IterationEnd {
                iteration: 1,
                message_count: 1,
                array_version,
                ..
            } => assert!(*array_version > 0),
            other => panic!("expected IterationEnd, got {:?}", other),
        }

        // No tool calls -> complete immediately with content.
        assert!(!result.should_continue);
        assert_eq!(result.content, Value::String("hello world".to_string()));
        assert_eq!(executed.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_stream_events_with_tool_call() {
        let executed = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let registry = mock_tool_registry(&executed);
        let mock = Arc::new(MockLlmClient::new());
        mock.script_stream(vec![
            MessageStreamEvent::Text(wf_types::llm::MessageStreamText {
                text: "using tool".to_string(),
                snapshot: String::new(),
            }),
            MessageStreamEvent::FinalMessage(wf_types::llm::MessageStreamFinal {
                message: tool_call_message("tc-9"),
                usage: None,
                stream_stats: None,
            }),
            MessageStreamEvent::End(wf_types::llm::MessageStreamEnd {}),
        ]);
        let entity = AgentLoopEntity::new(Id::from("agent-stream-2".to_string()))
            .with_model("mock".to_string());
        entity.state.write().await.start().unwrap();

        let (result, mut rx) = run_streaming(stream_gateway(mock), registry, &entity).await;
        let result = result.expect("stream iteration with tool must succeed");

        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }
        let tool_start = events
            .iter()
            .find(|e| matches!(e, AgentStreamEvent::ToolStart { .. }));
        let tool_end = events
            .iter()
            .find(|e| matches!(e, AgentStreamEvent::ToolEnd { .. }));
        assert!(tool_start.is_some(), "ToolStart missing: {:?}", events);
        assert!(tool_end.is_some(), "ToolEnd missing: {:?}", events);
        if let Some(AgentStreamEvent::ToolEnd {
            success, result: r, ..
        }) = tool_end
        {
            assert!(*success);
            assert!(r.contains("stream-tool-ok"), "unexpected result: {}", r);
        }
        assert_eq!(executed.load(std::sync::atomic::Ordering::SeqCst), 1);
        // Tool call was mock_write (not attempt_completion) -> keep looping.
        assert!(result.should_continue);
        assert_eq!(result.tool_call_count, 1);
        // Tool message added to conversation (assistant + tool).
        let messages = entity.conversation().read().await.messages().to_vec();
        assert_eq!(messages.len(), 2);
    }

    #[tokio::test]
    async fn test_stream_error_propagates() {
        let mock = Arc::new(MockLlmClient::new());
        mock.script_error(wf_llm::error::LlmError::StreamError(
            "upstream exploded".to_string(),
        ));
        let registry = Arc::new(wf_tools::registry::ToolRegistry::new());
        let entity = AgentLoopEntity::new(Id::from("agent-stream-3".to_string()))
            .with_model("mock".to_string());
        entity.state.write().await.start().unwrap();

        let (result, _rx) = run_streaming(stream_gateway(mock), registry, &entity).await;
        let err = result.expect_err("stream error must fail the iteration");
        assert!(err.to_string().contains("upstream exploded"));
    }

    async fn run_streaming_with_bus(
        gateway: Arc<LlmGateway>,
        registry: Arc<wf_tools::registry::ToolRegistry>,
        entity: &AgentLoopEntity,
        bus: Arc<wf_core::EventBus>,
    ) -> AgentResult<IterationResult> {
        let (tx, _rx) = mpsc::channel(32);
        let coordinator = AgentIterationCoordinator::new(gateway, registry, None)
            .with_streaming(AgentEventSink::new(tx, None))
            .with_event_bus(bus);
        coordinator.execute_iteration(entity).await
    }

    fn drain_until_type(
        sub: &mut wf_core::event::Subscription,
        event_type: wf_types::events::EventType,
    ) -> Option<wf_types::events::BaseEvent> {
        for _ in 0..16 {
            match sub.try_recv() {
                Ok(event) if event.r#type == event_type => return Some(event),
                Ok(_) => {}
                Err(_) => break,
            }
        }
        None
    }

    #[tokio::test]
    async fn test_stream_error_event_published_to_bus() {
        let mock = Arc::new(MockLlmClient::new());
        mock.script_stream(vec![MessageStreamEvent::Error(
            wf_types::llm::MessageStreamError {
                error: "HTTP 500 boom".to_string(),
            },
        )]);
        let registry = Arc::new(wf_tools::registry::ToolRegistry::new());
        let entity = AgentLoopEntity::new(Id::from("agent-stream-err".to_string()))
            .with_model("mock".to_string());
        entity.state.write().await.start().unwrap();

        let bus = Arc::new(wf_core::EventBus::new(64));
        let mut sub = bus.subscribe();

        let result =
            run_streaming_with_bus(stream_gateway(mock), registry, &entity, bus.clone()).await;
        assert!(result.is_err(), "stream error must fail the iteration");

        let event = drain_until_type(&mut sub, wf_types::events::EventType::LlmStreamError)
            .expect("LlmStreamError must be published");
        assert_eq!(event.execution_id.as_deref(), Some("agent-stream-err"));
        assert_eq!(event.agent_loop_id.as_deref(), Some("agent-stream-err"));
        let meta = event.metadata.unwrap();
        assert_eq!(meta["error"], serde_json::json!("HTTP 500 boom"));
        assert_eq!(meta["profile_id"], serde_json::json!("mock"));
    }

    #[tokio::test]
    async fn test_stream_abort_event_published_to_bus() {
        let mock = Arc::new(MockLlmClient::new());
        mock.script_stream(vec![MessageStreamEvent::Abort(
            wf_types::llm::MessageStreamAbort {
                reason: "dead loop detected".to_string(),
            },
        )]);
        let registry = Arc::new(wf_tools::registry::ToolRegistry::new());
        let entity = AgentLoopEntity::new(Id::from("agent-stream-abort".to_string()))
            .with_model("mock".to_string());
        entity.state.write().await.start().unwrap();

        let bus = Arc::new(wf_core::EventBus::new(64));
        let mut sub = bus.subscribe();

        let result =
            run_streaming_with_bus(stream_gateway(mock), registry, &entity, bus.clone()).await;
        assert!(result.is_err(), "stream abort must fail the iteration");

        let event = drain_until_type(&mut sub, wf_types::events::EventType::LlmStreamAborted)
            .expect("LlmStreamAborted must be published");
        let meta = event.metadata.unwrap();
        assert_eq!(meta["reason"], serde_json::json!("dead loop detected"));
        assert_eq!(meta["profile_id"], serde_json::json!("mock"));
    }
}
