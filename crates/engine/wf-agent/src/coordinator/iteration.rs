//! Single agent iteration skeleton: hook boundaries, request assembly,
//! LLM dispatch (blocking or streaming), tool execution and iteration
//! close. Peripheral concerns live in sibling submodules (`llm_call`,
//! `streaming`, `compression`, `checkpoint`, `outcome`).

mod checkpoint;
mod compression;
mod llm_call;
mod outcome;
mod streaming;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use wf_execution_shared::hooks::HookHandlerRegistry;
use wf_execution_shared::types::execution_entity::ExecutionEntity;
use wf_llm::LlmGateway;
use wf_metrics::MetricsRegistry;
use wf_tools::registry::ToolRegistry;
use wf_types::tool::approval::ToolApprovalOptions;

use crate::agent_request::build_agent_request;
use crate::approval::ToolApprovalHandler;
use crate::coordinator::tool::ToolExecutionCoordinator;
use crate::entity::AgentLoopEntity;
use crate::error::AgentResult;
use crate::hook::AgentHookEmitter;
use crate::stream::{AgentEventSink, AgentStreamEvent};

#[derive(Debug, Clone)]
pub struct IterationResult {
    pub should_continue: bool,
    pub content: Value,
    pub completion_data: Option<Value>,
    pub tool_call_count: u32,
    /// Terminal classification when this result ends the loop (`should_continue
    /// == false`); the content string stays a human-readable summary.
    pub finish_reason: wf_tools::callback::LoopFinishReason,
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
pub const DEFAULT_TOKEN_WARNING_THRESHOLD: u32 =
    wf_execution_shared::DEFAULT_TOKEN_WARNING_THRESHOLD;

/// Single iteration implementation shared by blocking and streaming runs.
pub struct AgentIterationCoordinator {
    gateway: Arc<LlmGateway>,
    tool_coordinator: ToolExecutionCoordinator,
    metrics: Option<Arc<MetricsRegistry>>,
    mode: IterationMode,
    event_sink: Option<AgentEventSink>,
    event_bus: Option<Arc<wf_core::EventBus>>,
    /// Shared hook receiver registry; hook points dispatch through it.
    hook_handler_registry: Option<Arc<HookHandlerRegistry>>,
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
    /// Strategy-gated checkpoint handle for intra-iteration boundaries
    /// (tool calls, compression signals, message-count backstop). Shared via
    /// `Arc` so the tool execution coordinator observes the same handle.
    /// `None` disables boundary checkpoints; the iteration-end checkpoint
    /// stays with the execution coordinator.
    checkpoint: Option<Arc<crate::checkpoint::AgentCheckpointIntegration>>,
    /// Message-count backstop: checkpoint every N appended messages.
    /// `None` disables (tool boundaries already cover most cases).
    message_interval: Option<u32>,
    /// Session message count at the last message-level checkpoint, so a
    /// batch of appends crossing several multiples fires exactly once.
    message_checkpoint_watermark: std::sync::Mutex<u64>,
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
            hook_handler_registry: None,
            token_warning_threshold: DEFAULT_TOKEN_WARNING_THRESHOLD,
            token_tracking_enabled: true,
            general_description: None,
            discoverable_metadata_block: None,
            checkpoint: None,
            message_interval: None,
            message_checkpoint_watermark: std::sync::Mutex::new(0),
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
        let file_observer = self.tool_coordinator.checkpoint_session_config();
        let agent_checkpoint = self.tool_coordinator.agent_checkpoint_config();
        self.tool_coordinator = ToolExecutionCoordinator::new(registry)
            .with_event_bus(self.event_bus.clone())
            .with_metrics(self.metrics.clone())
            .with_approval(options, handler)
            .with_checkpoint_session(file_observer)
            .with_agent_checkpoint(agent_checkpoint);
        self
    }

    /// Gate tool visibility at execution time (blocks only intercept; the
    /// schema is assembled independently).
    pub fn with_visibility_store(
        mut self,
        store: Option<Arc<dyn crate::coordinator::tool::ToolVisibilityStore>>,
    ) -> Self {
        // Rebuilding the tool coordinator must preserve the approval and
        // file-observer wiring applied earlier, otherwise every tool call is
        // auto-approved downstream or loses file attribution.
        let registry = self.tool_coordinator.tool_registry().clone();
        let (approval_options, approval_handler) = self.tool_coordinator.approval_config();
        let file_observer = self.tool_coordinator.checkpoint_session_config();
        let agent_checkpoint = self.tool_coordinator.agent_checkpoint_config();
        self.tool_coordinator = ToolExecutionCoordinator::new(registry)
            .with_event_bus(self.event_bus.clone())
            .with_metrics(self.metrics.clone())
            .with_approval(approval_options, approval_handler)
            .with_visibility_store(store)
            .with_checkpoint_session(file_observer)
            .with_agent_checkpoint(agent_checkpoint);
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
            .with_hook_handler_registry(self.hook_handler_registry.clone());
        self
    }

    /// Inject the shared hook receiver registry: every hook point dispatches
    /// through it (synchronous receiver notification + audit event).
    pub fn with_hook_handler_registry(
        mut self,
        registry: Option<Arc<HookHandlerRegistry>>,
    ) -> Self {
        self.hook_handler_registry = registry;
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

    /// Attach the strategy-gated checkpoint handle used for intra-iteration
    /// boundaries (tool calls, compression signals, message backstop) and
    /// hook `create_checkpoint` opt-ins. The same handle is shared with the
    /// tool execution coordinator so tool-call hook points observe it.
    pub fn with_checkpoint(
        mut self,
        checkpoint: Option<crate::checkpoint::AgentCheckpointIntegration>,
    ) -> Self {
        let shared = checkpoint.map(Arc::new);
        self.tool_coordinator.set_agent_checkpoint(shared.clone());
        self.checkpoint = shared;
        self
    }

    /// Checkpoint every N appended conversation messages (`None` disables).
    pub fn with_message_interval(mut self, interval: Option<u32>) -> Self {
        self.message_interval = interval.filter(|n| *n > 0);
        self
    }

    /// Inject the file-content observer (agent actor partition) into the
    /// tool execution coordinator. Independent from execution-state
    /// snapshots: file changes land in the actor partition, checkpoints
    /// snapshot the execution record.
    pub fn with_checkpoint_session(
        mut self,
        session: Option<wf_checkpoint::CheckpointSession>,
    ) -> Self {
        self.tool_coordinator = self.tool_coordinator.with_checkpoint_session(session);
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

    pub async fn execute_iteration(
        &self,
        entity: &AgentLoopEntity,
    ) -> AgentResult<IterationResult> {
        let execution_id = entity.id().clone();

        AgentHookEmitter::fire_agent_point_with_checkpoint(
            entity,
            "BEFORE_ITERATION",
            HashMap::new(),
            self.hook_handler_registry.as_deref(),
            self.event_bus.as_deref(),
            self.checkpoint.as_deref(),
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

        AgentHookEmitter::fire_agent_point_with_checkpoint(
            entity,
            "BEFORE_LLM_CALL",
            HashMap::new(),
            self.hook_handler_registry.as_deref(),
            self.event_bus.as_deref(),
            self.checkpoint.as_deref(),
        )
        .await;

        // Blocking backpressure: when a compression run is in flight for
        // the current conversation version, wait (bounded by the settle
        // budget) for the write-back to land so this iteration assembles
        // the request from the compressed view. A compression failure or a
        // settle timeout parks the loop in the externally perceivable
        // paused state for handling; abort cancels the wait.
        if self.token_tracking_enabled {
            if let Some(result) = self.settle_compression_flight(entity).await? {
                return Ok(result);
            }
        }

        let request = build_agent_request(
            entity,
            self.tool_coordinator.tool_registry(),
            self.is_streaming(),
            self.general_description.as_deref(),
            self.discoverable_metadata_block.as_deref(),
        )
        .await?;

        // Pre-request context budget check: the estimate is approximate,
        // so this is a warning only (never blocks the request); the
        // provider's real count takes precedence. Compares the single
        // request estimate against the model-window context budget.
        // Near the threshold the provider count-tokens API refines the
        // estimate, mirroring the workflow preflight. The conversation
        // lock is never held across the network call.
        // Fires at most once per session until a compression write-back
        // re-arms it.
        if self.token_tracking_enabled {
            if let Some(ref bus) = self.event_bus {
                let context_limit = entity.conversation().read().await.context_limit();
                if context_limit > 0 {
                    let mut estimated = u64::from(wf_llm::estimate_request_tokens(&request));
                    if estimated as f64 > context_limit as f64 * 0.8 {
                        if let Ok(count) = self
                            .gateway
                            .count_tokens(&request, Some(entity.get_abort_signal()))
                            .await
                        {
                            estimated = u64::from(count.input_tokens);
                        }
                    }
                    if estimated > context_limit {
                        let mut conversation = entity.conversation().write().await;
                        if conversation.consume_preflight_warning() {
                            let _ =
                                bus.publish(wf_execution_shared::build_token_usage_warning_event(
                                    &execution_id,
                                    Some(entity.id()),
                                    estimated,
                                    context_limit,
                                    estimated as f64 / context_limit as f64 * 100.0,
                                ));
                        }
                    }
                }
            }
        }

        if let Some(ref metrics) = self.metrics {
            if let Some(format) = entity.tool_call_protocol() {
                metrics
                    .agent_loop()
                    .record_protocol_locked(&format.format.to_string());
            }
        }

        let (assistant_msg, llm_content, finish_reason, request_usage) = match self.mode {
            IterationMode::Blocking => self.blocking_llm_call(entity, &request).await?,
            IterationMode::Streaming => self.stream_llm_call(entity, &request).await?,
        };

        self.record_usage_and_compression(entity, &request, &assistant_msg, &request_usage)
            .await;

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
        AgentHookEmitter::fire_agent_point_with_checkpoint(
            entity,
            "AFTER_LLM_CALL",
            hook_data,
            self.hook_handler_registry.as_deref(),
            self.event_bus.as_deref(),
            self.checkpoint.as_deref(),
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
        if has_tool_calls {
            // The assistant message carries the finalized tool-call
            // arguments: snapshot the pre-tool moment.
            self.boundary_checkpoint(entity, wf_types::checkpoint::CheckpointTiming::ToolBefore)
                .await;
        }
        self.maybe_message_checkpoint(entity).await;

        if !has_tool_calls {
            let content = llm_call::text_of(&assistant_msg.content);
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
        // Tool results (success or failure) are in the session: snapshot
        // the post-tool moment, then run the message-count backstop.
        self.boundary_checkpoint(entity, wf_types::checkpoint::CheckpointTiming::ToolAfter)
            .await;
        self.maybe_message_checkpoint(entity).await;

        let content = llm_call::text_of(&assistant_msg.content);
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
}
