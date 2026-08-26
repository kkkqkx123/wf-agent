use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use wf_common::retry::RetryBudget;
use wf_execution_shared::hooks::types::BaseHookContext;
use wf_execution_shared::hooks::HookRegistry;
use wf_execution_shared::types::execution_entity::ExecutionEntity;
use wf_metrics::MetricsRegistry;
use wf_tools::approval::{ApprovalDecision, ToolApprovalCoordinator};
use wf_tools::failure_protection::ToolFailureProtectionState;
use wf_tools::registry::ToolRegistry;
use wf_types::interaction::tool_approval::{PendingToolCallInfo, ToolApprovalRequestData};
use wf_types::message::{LlmToolCall, Message, MessageContentValue, MessageRole};
use wf_types::tool::approval::ToolApprovalOptions;
use wf_types::tool::file_permission::FilePermissionSettings;
use wf_types::tool::ToolRiskLevel;
use wf_types::tool::{CheckpointTiming, ToolExecutionOptions};

use crate::approval::{RejectionMessageBuilder, ToolApprovalHandler, ToolApprovalRequest};
use crate::entity::AgentLoopEntity;
use crate::error::AgentResult;
use crate::hook::AgentHookHandler;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ToolExecutionMode {
    #[default]
    Sequential,
    Parallel,
}

/// Per-tool-call decision produced by the approval engine.
#[derive(Debug, Clone)]
enum ApprovalOutcome {
    Execute { edited_parameters: Option<Value> },
    Rejected { reason: String },
}

/// Phase of a single tool execution reported through the progress channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolProgressStatus {
    Started,
    Completed,
    Failed,
    Cancelled,
}

/// A progress event emitted for a single tool call.
#[derive(Debug, Clone)]
pub struct ToolProgressEvent {
    pub tool_call_id: String,
    pub status: ToolProgressStatus,
    pub partial: Option<Value>,
}

/// Optional visibility gate applied before tool execution. Used to hide
/// tools from an execution without removing them from the registry.
#[async_trait]
pub trait ToolVisibilityStore: Send + Sync {
    async fn is_tool_visible(&self, execution_id: &str, tool_name: &str) -> bool;
}

/// Optional checkpoint creation callback invoked around tool executions that
/// opt in via `ToolMetadata::create_checkpoint`.
#[async_trait]
pub trait ToolCheckpointHandler: Send + Sync {
    async fn create_checkpoint(&self, execution_id: &str, reason: &str) -> AgentResult<()>;
}

/// Per-task outcome produced by the parallel execution path.
enum TaskOutcome {
    Ok(Message),
    Failed(String),
}

/// Immutable execution context shared by sequential and parallel tool runs.
#[derive(Clone)]
pub(crate) struct ToolRunCtx {
    pub(crate) registry: Arc<ToolRegistry>,
    pub(crate) metrics: Option<Arc<MetricsRegistry>>,
    pub(crate) progress_tx: Option<tokio::sync::mpsc::Sender<ToolProgressEvent>>,
    pub(crate) checkpoint_handler: Option<Arc<dyn ToolCheckpointHandler>>,
    pub(crate) failure_protection: Option<Arc<ToolFailureProtectionState>>,
    pub(crate) visibility_store: Option<Arc<dyn ToolVisibilityStore>>,
    pub(crate) general_invoker: Option<Arc<dyn wf_tools::general::GeneralToolInvoker>>,
    pub(crate) retry_budget: Option<Arc<RetryBudget>>,
}

pub struct ToolExecutionCoordinator {
    tool_registry: Arc<ToolRegistry>,
    event_bus: Option<Arc<wf_core::EventBus>>,
    /// Shared hook receiver registry; hook points dispatch through it.
    hook_registry: Option<Arc<HookRegistry>>,
    mode: ToolExecutionMode,
    metrics: Option<Arc<MetricsRegistry>>,
    approval_options: Option<ToolApprovalOptions>,
    approval_handler: Option<Arc<dyn ToolApprovalHandler>>,
    rejection_builder: RejectionMessageBuilder,
    progress_tx: Option<tokio::sync::mpsc::Sender<ToolProgressEvent>>,
    cancellation: Option<CancellationToken>,
    cancel_on_failure: bool,
    visibility_store: Option<Arc<dyn ToolVisibilityStore>>,
    checkpoint_handler: Option<Arc<dyn ToolCheckpointHandler>>,
    failure_protection: Option<Arc<ToolFailureProtectionState>>,
    /// Per-run `general` tool invoker. Injected once when the run starts
    /// (set after the coordinator is assembled); carried into every
    /// execution context snapshot so the builtin `general` handler resolves
    /// its invoker from the context instead of global per-execution state.
    general_invoker: Arc<std::sync::Mutex<Option<Arc<dyn wf_tools::general::GeneralToolInvoker>>>>,
    retry_budget: Option<Arc<RetryBudget>>,
}

impl ToolExecutionCoordinator {
    pub fn new(tool_registry: Arc<ToolRegistry>) -> Self {
        Self {
            tool_registry,
            event_bus: None,
            hook_registry: None,
            mode: ToolExecutionMode::default(),
            metrics: None,
            approval_options: None,
            approval_handler: None,
            rejection_builder: RejectionMessageBuilder::new(),
            progress_tx: None,
            cancellation: None,
            cancel_on_failure: false,
            visibility_store: None,
            checkpoint_handler: None,
            failure_protection: None,
            general_invoker: Arc::new(std::sync::Mutex::new(None)),
            retry_budget: None,
        }
    }

    /// Attach the event bus tool-call hook events are published to.
    pub fn with_event_bus(mut self, event_bus: Option<Arc<wf_core::EventBus>>) -> Self {
        self.event_bus = event_bus;
        self
    }

    /// Inject the shared hook receiver registry; tool-call hooks dispatch
    /// through it (synchronous receiver notification + audit event).
    pub fn with_hook_registry(mut self, registry: Option<Arc<HookRegistry>>) -> Self {
        self.hook_registry = registry;
        self
    }

    pub fn with_mode(mut self, mode: ToolExecutionMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn with_metrics(mut self, metrics: Option<Arc<MetricsRegistry>>) -> Self {
        self.metrics = metrics;
        self
    }

    /// Stream tool progress events (started / completed / failed / cancelled)
    /// into the given channel. Default: no progress reporting.
    pub fn with_progress_tx(
        mut self,
        progress_tx: Option<tokio::sync::mpsc::Sender<ToolProgressEvent>>,
    ) -> Self {
        self.progress_tx = progress_tx;
        self
    }

    /// Merge an external cancellation token with the entity abort signal. All
    /// tool executions observe it; parallel mode additionally aborts the whole
    /// batch on cancellation.
    pub fn with_cancellation(mut self, cancellation: Option<CancellationToken>) -> Self {
        self.cancellation = cancellation;
        self
    }

    /// In parallel mode, abort the whole batch when any tool call fails.
    /// Default: `false` (independent tool execution, matching current
    /// behavior).
    pub fn with_cancel_on_failure(mut self, enabled: bool) -> Self {
        self.cancel_on_failure = enabled;
        self
    }

    /// Gate tool visibility before execution. Invisible tools produce an
    /// error message instead of executing. Default: all tools visible.
    pub fn with_visibility_store(mut self, store: Option<Arc<dyn ToolVisibilityStore>>) -> Self {
        self.visibility_store = store;
        self
    }

    /// Enable checkpoint creation around tools whose metadata opts in via
    /// `create_checkpoint`. Default: no checkpoints.
    pub fn with_checkpoint_handler(
        mut self,
        handler: Option<Arc<dyn ToolCheckpointHandler>>,
    ) -> Self {
        self.checkpoint_handler = handler;
        self
    }

    /// Enable failure protection: tools are blocked after a configurable
    /// number of consecutive failures, and successes reset the counter.
    /// Default: disabled.
    pub fn with_failure_protection(
        mut self,
        state: Option<Arc<ToolFailureProtectionState>>,
    ) -> Self {
        self.failure_protection = state;
        self
    }

    /// Enforce a shared retry budget on tool failure retries: each failed
    /// attempt consumes the budget; when exhausted retries are abandoned and
    /// the failure reason is returned. Default: no budget (fail on first
    /// error, matching the historical single-attempt behavior).
    pub fn with_retry_budget(mut self, budget: Option<Arc<RetryBudget>>) -> Self {
        self.retry_budget = budget;
        self
    }

    /// Inject the run's `general` tool invoker (once per run, before
    /// execution starts). The invoker is carried into every execution
    /// context snapshot built afterwards.
    pub fn set_general_invoker(&self, invoker: Arc<dyn wf_tools::general::GeneralToolInvoker>) {
        *wf_common::lock::lock_ok(self.general_invoker.lock()) = Some(invoker);
    }

    /// Register tool approval configuration. Without a handler and without
    /// explicit options every tool call is auto-approved by default.
    pub fn with_approval(
        mut self,
        options: Option<ToolApprovalOptions>,
        handler: Option<Arc<dyn ToolApprovalHandler>>,
    ) -> Self {
        self.approval_options = options;
        self.approval_handler = handler;
        self
    }

    /// Current approval wiring (options + handler); lets callers rebuild
    /// the coordinator without silently dropping the approval contract.
    pub fn approval_config(
        &self,
    ) -> (
        Option<ToolApprovalOptions>,
        Option<Arc<dyn ToolApprovalHandler>>,
    ) {
        (self.approval_options.clone(), self.approval_handler.clone())
    }

    pub fn with_rejection_builder(mut self, builder: RejectionMessageBuilder) -> Self {
        self.rejection_builder = builder;
        self
    }

    pub fn tool_registry(&self) -> &Arc<ToolRegistry> {
        &self.tool_registry
    }

    /// The runtime visibility gate used for execution and per-turn assembly.
    pub fn visibility_store(&self) -> Option<Arc<dyn ToolVisibilityStore>> {
        self.visibility_store.clone()
    }

    /// Snapshot the immutable execution context shared by sequential and
    /// parallel tool runs.
    fn run_ctx(&self) -> ToolRunCtx {
        ToolRunCtx {
            registry: self.tool_registry.clone(),
            metrics: self.metrics.clone(),
            progress_tx: self.progress_tx.clone(),
            checkpoint_handler: self.checkpoint_handler.clone(),
            failure_protection: self.failure_protection.clone(),
            visibility_store: self.visibility_store.clone(),
            general_invoker: wf_common::lock::lock_ok(self.general_invoker.lock()).clone(),
            retry_budget: self.retry_budget.clone(),
        }
    }

    /// The immutable execution context, exposed for the `general` tool
    /// invoker so inner invocations share the exact same pipeline.
    pub(crate) fn execution_ctx(&self) -> ToolRunCtx {
        self.run_ctx()
    }

    pub async fn execute_tool_calls(
        &self,
        entity: &AgentLoopEntity,
        tool_calls: &[LlmToolCall],
    ) -> AgentResult<Vec<Message>> {
        match self.mode {
            ToolExecutionMode::Sequential => self.execute_sequential(entity, tool_calls).await,
            ToolExecutionMode::Parallel => self.execute_parallel(entity, tool_calls).await,
        }
    }

    /// Run the approval engine for a batch of tool calls. Produces one
    /// outcome per tool call, in order.
    async fn approve_tool_calls(
        &self,
        entity: &AgentLoopEntity,
        tool_calls: &[LlmToolCall],
    ) -> Vec<ApprovalOutcome> {
        // Fast path: no handler and no options -> auto-approve everything.
        if self.approval_handler.is_none() && self.approval_options.is_none() {
            return tool_calls
                .iter()
                .map(|_| ApprovalOutcome::Execute {
                    edited_parameters: None,
                })
                .collect();
        }

        let requests: Vec<ToolApprovalRequestData> = tool_calls
            .iter()
            .map(|tc| ToolApprovalRequestData {
                tool_call_id: tc.id.clone(),
                tool_name: tc.function.name.clone(),
                tool_description: None,
                parameters: serde_json::from_str(&tc.function.arguments).unwrap_or(Value::Null),
                risk_level: Self::risk_level_of(&self.tool_registry, &tc.function.name),
                pending_queue: None,
                batch_id: None,
                tool_index: None,
                total_tools: None,
                timeout: None,
                security_preset: None,
            })
            .collect();

        // When a handler is registered it controls the policy; without
        // explicit options fall back to ask-everything for the handler.
        let options = self
            .approval_options
            .clone()
            .unwrap_or_else(|| ToolApprovalOptions {
                auto_approval_enabled: Some(self.approval_handler.is_none()),
                security_preset: None,
                risk_threshold: None,
                auto_approve_patterns: None,
                categories: None,
                workspace_boundary: None,
                file_permissions: Some(FilePermissionSettings::default_rules()),
                command: None,
                mcp: None,
                network: None,
                interaction: None,
                allow_write_protected: None,
            });

        let coordinator = ToolApprovalCoordinator::new(options);
        let decisions = coordinator.evaluate(&requests);
        let batch = coordinator.process_batch(requests);

        // Policy denials are final: they must never be escalated into a
        // human approval request, so drop them from the pending set before
        // the interaction loop runs.
        let asks: Vec<usize> = batch
            .pending
            .iter()
            .copied()
            .filter(|idx| !matches!(decisions[*idx], ApprovalDecision::Deny(_)))
            .collect();

        let mut outcomes: Vec<ApprovalOutcome> = decisions
            .iter()
            .enumerate()
            .map(|(idx, decision)| match decision {
                ApprovalDecision::Deny(reason) => ApprovalOutcome::Rejected {
                    reason: reason.clone(),
                },
                _ => ApprovalOutcome::Rejected {
                    reason: format!("internal: unclassified (tool call {idx})"),
                },
            })
            .collect();

        for idx in &batch.auto_approved {
            outcomes[*idx] = ApprovalOutcome::Execute {
                edited_parameters: None,
            };
        }

        for idx in &asks {
            let tc = &tool_calls[*idx];
            let outcome = match self.approval_handler.as_ref() {
                Some(handler) => {
                    let interaction_id = format!(
                        "approval-{}-{}",
                        wf_common::now(),
                        tc.id
                    );
                    let request = ToolApprovalRequest {
                        tool_call_id: tc.id.clone(),
                        tool_name: tc.function.name.clone(),
                        arguments: serde_json::from_str(&tc.function.arguments)
                            .unwrap_or(Value::Null),
                        interaction_id,
                        batch_id: Some(batch.batch_id.clone()),
                        tool_index: Some(*idx as u32),
                        total_tools: Some(tool_calls.len() as u32),
                        pending_queue: Some(
                            asks.iter()
                                .map(|p| PendingToolCallInfo {
                                    id: tool_calls[*p].id.clone(),
                                    name: tool_calls[*p].function.name.clone(),
                                    arguments: Some(
                                        serde_json::from_str(&tool_calls[*p].function.arguments)
                                            .unwrap_or(Value::Null),
                                    ),
                                    risk_level: None,
                                })
                                .collect(),
                        ),
                    };

                    // Approval waits must not consume the wall-clock budget.
                    let _guard = entity.timeout_manager().pause_handle();
                    let result = handler.request_approval(&request).await;
                    if result.approved {
                        ApprovalOutcome::Execute {
                            edited_parameters: result.edited_parameters,
                        }
                    } else {
                        ApprovalOutcome::Rejected {
                            reason: result
                                .rejection_reason
                                .unwrap_or_else(|| "Rejected by user".to_string()),
                        }
                    }
                }
                None => ApprovalOutcome::Rejected {
                    reason: format!(
                        "No approval handler configured. Tool \"{}\" requires manual approval but no handler is registered.",
                        tc.function.name
                    ),
                },
            };
            outcomes[*idx] = outcome;
        }

        outcomes
    }

    fn build_rejection_message(&self, tc: &LlmToolCall, reason: &str) -> Message {
        Message {
            id: wf_types::Id::new(),
            role: MessageRole::Tool,
            content: MessageContentValue::Text(serde_json::json!({
                "error": self.rejection_builder.build_rejection_message(&tc.function.name, Some(reason))
            })
            .to_string()),
            timestamp: wf_common::now(),
            tool_call_id: Some(tc.id.clone()),
            tool_name: Some(tc.function.name.clone()),
            tool_calls: None,
            thinking: None,
            metadata: None,
        }
    }

    fn risk_level_of(registry: &ToolRegistry, name: &str) -> Option<String> {
        registry
            .list_tools()
            .into_iter()
            .find(|t| t.name == name)
            .and_then(|t| t.metadata)
            .and_then(|m| m.risk_level)
            .map(|level| match level {
                ToolRiskLevel::ReadOnly => "read_only",
                ToolRiskLevel::Write => "write",
                ToolRiskLevel::Execute => "execute",
                ToolRiskLevel::Mcp => "mcp",
                ToolRiskLevel::Network => "network",
                ToolRiskLevel::System => "system",
                ToolRiskLevel::Interaction => "interaction",
            })
            .map(String::from)
    }

    /// Approval gate for the streaming tool path: approve one tool call
    /// through the same batch pipeline as the sequential executor. Returns
    /// the rejection message when the call is denied, `None` when it may
    /// execute.
    pub async fn approve_single_for_stream(
        &self,
        entity: &AgentLoopEntity,
        tc: &LlmToolCall,
    ) -> Option<Message> {
        let outcomes = self
            .approve_tool_calls(entity, std::slice::from_ref(tc))
            .await;
        match outcomes.first() {
            Some(ApprovalOutcome::Rejected { reason }) => {
                Some(self.build_rejection_message(tc, reason))
            }
            _ => None,
        }
    }

    async fn execute_sequential(
        &self,
        entity: &AgentLoopEntity,
        tool_calls: &[LlmToolCall],
    ) -> AgentResult<Vec<Message>> {
        let outcomes = self.approve_tool_calls(entity, tool_calls).await;
        let mut messages = Vec::with_capacity(tool_calls.len());

        for (idx, tc) in tool_calls.iter().enumerate() {
            let outcome = &outcomes[idx];
            match outcome {
                ApprovalOutcome::Rejected { reason } => {
                    AgentHookHandler::emit_agent_hooks(
                        entity,
                        "BEFORE_TOOL_CALL",
                        Self::build_hook_data(tc),
                        self.hook_registry.as_deref(),
                        self.event_bus.as_deref(),
                    )
                    .await;
                    let msg = self.build_rejection_message(tc, reason);
                    let mut hook_data = Self::build_hook_data(tc);
                    hook_data.insert("error".to_string(), Value::String(reason.clone()));
                    AgentHookHandler::emit_agent_hooks(
                        entity,
                        "AFTER_TOOL_CALL",
                        hook_data,
                        self.hook_registry.as_deref(),
                        self.event_bus.as_deref(),
                    )
                    .await;
                    messages.push(msg);
                    continue;
                }
                ApprovalOutcome::Execute { edited_parameters } => {
                    let mut tc = tc.clone();
                    if let Some(edited) = edited_parameters {
                        tc.function.arguments =
                            serde_json::to_string(edited).unwrap_or(tc.function.arguments);
                    }
                    AgentHookHandler::emit_agent_hooks(
                        entity,
                        "BEFORE_TOOL_CALL",
                        Self::build_hook_data(&tc),
                        self.hook_registry.as_deref(),
                        self.event_bus.as_deref(),
                    )
                    .await;

                    let msg = self.execute_single_tool(entity, &tc).await?;

                    AgentHookHandler::emit_agent_hooks(
                        entity,
                        "AFTER_TOOL_CALL",
                        Self::build_hook_data(&tc),
                        self.hook_registry.as_deref(),
                        self.event_bus.as_deref(),
                    )
                    .await;

                    messages.push(msg);
                }
            }
        }

        Ok(messages)
    }

    async fn execute_parallel(
        &self,
        entity: &AgentLoopEntity,
        tool_calls: &[LlmToolCall],
    ) -> AgentResult<Vec<Message>> {
        let outcomes = self.approve_tool_calls(entity, tool_calls).await;
        let mut messages: Vec<Option<Message>> = vec![None; tool_calls.len()];
        let run_ctx = self.run_ctx();
        let batch_cancellation = self.batch_cancellation(entity);

        let mut set = tokio::task::JoinSet::new();
        for (idx, tc) in tool_calls.iter().enumerate() {
            match &outcomes[idx] {
                ApprovalOutcome::Rejected { reason } => {
                    messages[idx] = Some(self.build_rejection_message(tc, reason));
                }
                ApprovalOutcome::Execute { edited_parameters } => {
                    let mut tool_call = tc.clone();
                    if let Some(edited) = edited_parameters {
                        tool_call.function.arguments =
                            serde_json::to_string(edited).unwrap_or(tool_call.function.arguments);
                    }
                    let run_ctx = run_ctx.clone();
                    let event_bus = self.event_bus.clone();
                    let hook_registry = self.hook_registry.clone();
                    let entity_state = entity.state.clone();
                    let entity_hooks = entity.hooks().to_vec();
                    let entity_id = entity.id().clone();
                    let task_cancellation = batch_cancellation.child_token();

                    set.spawn(async move {
                        let hook_data = Self::build_hook_data(&tool_call);
                        let hook_ctx = BaseHookContext {
                            execution_id: entity_id.clone(),
                            data: hook_data.clone(),
                        };
                        let hook_ctx = wf_execution_shared::hooks::HookContext::from(&hook_ctx);

                        AgentHookHandler::emit_hooks(
                            &entity_hooks,
                            "BEFORE_TOOL_CALL",
                            &hook_ctx,
                            hook_registry.as_deref(),
                            event_bus.as_deref(),
                        )
                        .await;

                        let result = tokio::select! {
                            res = Self::run_tool(
                                &run_ctx,
                                &tool_call,
                                &entity_id,
                                &entity_state,
                            ) => res,
                            _ = task_cancellation.cancelled() => Err(
                                "Tool execution was cancelled".to_string()
                            ),
                        };

                        AgentHookHandler::emit_hooks(
                            &entity_hooks,
                            "AFTER_TOOL_CALL",
                            &hook_ctx,
                            hook_registry.as_deref(),
                            event_bus.as_deref(),
                        )
                        .await;

                        match result {
                            Ok(msg) => (idx, TaskOutcome::Ok(msg)),
                            Err(reason) => (idx, TaskOutcome::Failed(reason)),
                        }
                    });
                }
            }
        }

        let mut aborted = false;
        while let Some(joined) = set.join_next().await {
            match joined {
                Ok((idx, outcome)) => match outcome {
                    TaskOutcome::Ok(msg) => {
                        messages[idx] = Some(msg);
                    }
                    TaskOutcome::Failed(reason) => {
                        messages[idx] = Some(Self::error_message(&reason, None, None));
                        if self.cancel_on_failure {
                            set.abort_all();
                            aborted = true;
                        }
                    }
                },
                Err(e) if e.is_cancelled() => {
                    // Task aborted as part of a batch cancellation.
                    aborted = true;
                }
                Err(e) => {
                    // Task panicked. Its slot is filled with a generic error
                    // below; the concrete panic is logged.
                    tracing::error!(error = %e, "parallel tool task panicked");
                    aborted = true;
                }
            }
        }

        // Fill slots for tasks that were aborted / panicked before producing a
        // result. Rejected tools already filled their slots above.
        if aborted {
            for (idx, slot) in messages.iter_mut().enumerate() {
                if slot.is_none() {
                    let tc = &tool_calls[idx];
                    *slot = Some(Self::error_message(
                        "Tool execution did not complete (batch aborted, cancelled or panicked)",
                        Some(&tc.id),
                        Some(&tc.function.name),
                    ));
                }
            }
        }

        Ok(messages.into_iter().flatten().collect())
    }

    /// Single-tool execution used by the streaming driver; execution errors
    /// surface as tool error messages rather than failures.
    pub async fn execute_single_tool_for_stream(
        &self,
        entity: &AgentLoopEntity,
        tc: &LlmToolCall,
    ) -> Message {
        self.execute_single_tool(entity, tc)
            .await
            .unwrap_or_else(|e| {
                Self::error_message(&e.to_string(), Some(&tc.id), Some(&tc.function.name))
            })
    }

    fn resolve_timeout(registry: &ToolRegistry, tool_name: &str) -> u64 {
        if let Some(tool) = registry.list_tools().iter().find(|t| t.name == tool_name) {
            if let Some(ms) = tool.default_timeout_ms {
                return ms;
            }
            if let Some(config) = &tool.config {
                if let Some(ms) = config.get("timeout").and_then(|v| v.as_u64()) {
                    return ms;
                }
            }
        }
        120_000
    }

    fn tool_execution_deadline(timeout_ms: u64) -> Duration {
        let safety_margin = 30_000;
        Duration::from_millis(timeout_ms + safety_margin)
    }

    async fn execute_single_tool(
        &self,
        entity: &AgentLoopEntity,
        tc: &LlmToolCall,
    ) -> AgentResult<Message> {
        let ctx = self.run_ctx();
        Ok(Self::run_tool(&ctx, tc, entity.id(), &entity.state)
            .await
            .unwrap_or_else(|reason| {
                Self::error_message(&reason, Some(&tc.id), Some(&tc.function.name))
            }))
    }

    /// Shared single-tool execution core used by the sequential and parallel
    /// paths and by the `general` tool invoker. Errors are returned as
    /// `Err(reason)` so callers can decide how to surface them (tool error
    /// message, batch abort, etc.).
    pub(crate) async fn run_tool(
        ctx: &ToolRunCtx,
        tc: &LlmToolCall,
        entity_id: &str,
        entity_state: &tokio::sync::RwLock<crate::state::AgentLoopState>,
    ) -> Result<Message, String> {
        let params: Value = serde_json::from_str(&tc.function.arguments).unwrap_or(Value::Null);
        let tool_name = tc.function.name.clone();

        // replay idempotency: a tool call id that already produced a
        // result (e.g. replayed from a checkpoint taken mid-iteration) is
        // served from the cached result instead of executing the tool again.
        {
            let state = entity_state.read().await;
            if let Some(cached) = state.completed_tool_result(&tc.id) {
                let msg = Message {
                    id: wf_types::Id::new(),
                    role: MessageRole::Tool,
                    content: MessageContentValue::Text(cached.to_string()),
                    timestamp: wf_common::now(),
                    tool_call_id: Some(tc.id.clone()),
                    tool_name: Some(tool_name),
                    tool_calls: None,
                    thinking: None,
                    metadata: None,
                };
                return Ok(msg);
            }
        }
        entity_state.write().await.begin_tool_call(&tc.id);

        let tool_id = Self::find_tool_id_by_name(&ctx.registry, &tool_name);
        let timeout_ms = Self::resolve_timeout(&ctx.registry, &tool_name);
        let parameter_size = json_size(&params);

        // Visibility gate.
        if let Some(ref store) = ctx.visibility_store {
            if !store.is_tool_visible(entity_id, &tool_name).await {
                entity_state.write().await.finish_tool_call(&tc.id, None);
                return Err(format!(
                    "Tool '{}' is not visible in this execution",
                    tool_name
                ));
            }
        }

        if let Some(ref metrics) = ctx.metrics {
            metrics.tool().record_tool_call_start(&tool_name, entity_id);
        }
        Self::emit_progress(&ctx.progress_tx, &tc.id, ToolProgressStatus::Started, None);

        let Some(tid) = tool_id else {
            entity_state.write().await.finish_tool_call(&tc.id, None);
            if let Some(ref metrics) = ctx.metrics {
                metrics
                    .tool()
                    .record_tool_call_error(&tool_name, entity_id, "not_found");
            }
            Self::emit_progress(&ctx.progress_tx, &tc.id, ToolProgressStatus::Failed, None);
            return Err(format!("Tool not found: {}", tool_name));
        };

        // Failure protection gate.
        if let Some(ref fp) = ctx.failure_protection {
            let check = fp.can_execute(&tool_name);
            if !check.allowed {
                let reason = check.reason.unwrap_or_else(|| {
                    format!("Tool '{}' is blocked due to repeated failures", tool_name)
                });
                entity_state.write().await.finish_tool_call(&tc.id, None);
                Self::emit_progress(&ctx.progress_tx, &tc.id, ToolProgressStatus::Failed, None);
                return Err(reason);
            }
        }

        // Checkpoint before execution.
        let checkpoint_timing = ctx
            .registry
            .get_tool(&tid)
            .and_then(|t| t.metadata)
            .and_then(|m| m.create_checkpoint);
        let before = matches!(
            checkpoint_timing,
            Some(CheckpointTiming::Before) | Some(CheckpointTiming::Both)
        );
        let after = matches!(
            checkpoint_timing,
            Some(CheckpointTiming::After) | Some(CheckpointTiming::Both)
        );
        if before {
            if let Some(ref handler) = ctx.checkpoint_handler {
                if let Err(e) = handler
                    .create_checkpoint(entity_id, &format!("before tool '{}'", tool_name))
                    .await
                {
                    entity_state.write().await.finish_tool_call(&tc.id, None);
                    Self::emit_progress(&ctx.progress_tx, &tc.id, ToolProgressStatus::Failed, None);
                    return Err(format!(
                        "Checkpoint failed before tool '{}': {}",
                        tool_name, e
                    ));
                }
            }
        }

        let tool_ctx = {
            let mut tool_ctx =
                wf_tools::executor::trait_def::ToolExecutionContext::new(entity_id.into());
            if let Some(invoker) = &ctx.general_invoker {
                tool_ctx = tool_ctx.with_general_invoker(invoker.clone());
            }
            tool_ctx
        };
        let options = ToolExecutionOptions {
            timeout: Some(timeout_ms),
            retries: None,
            retry_delay: None,
            exponential_backoff: None,
        };

        let mut duration_ms;
        let result = loop {
            let start = wf_common::now();
            let attempt = tokio::time::timeout(
                Self::tool_execution_deadline(timeout_ms),
                ctx.registry
                    .execute_tool(&tid, &params, &options, &tool_ctx),
            )
            .await;
            duration_ms = (wf_common::now() - start) as f64;

            if matches!(&attempt, Ok(Ok(r)) if r.success) {
                break attempt;
            }
            // Budget-gated retry: each failed attempt consumes the shared
            // retry budget; when exhausted the failure is reported as-is.
            let Some(budget) = ctx.retry_budget.as_ref() else {
                break attempt;
            };
            let check = budget.consume_retry(0, None, duration_ms as u64);
            if !check.allowed {
                break attempt;
            }
            tracing::debug!(tool = %tool_name, "retrying tool call under retry budget");
        };
        let success = matches!(&result, Ok(Ok(r)) if r.success);

        // Audit payload for the persisted tool-call record: arguments as
        // passed, result payload on success, raw error otherwise.
        let call_result: Option<Value> = match &result {
            Ok(Ok(r)) if r.success => r.result.clone(),
            _ => None,
        };
        let call_error: Option<String> = match &result {
            Ok(Ok(r)) if !r.success => r
                .error
                .clone()
                .or_else(|| Some(format!("Tool '{}' reported failure", tool_name))),
            Ok(Err(e)) => Some(e.to_string()),
            Err(_) => Some(format!(
                "Tool '{}' timed out after {}ms",
                tool_name, timeout_ms
            )),
            _ => None,
        };

        entity_state.write().await.record_tool_call_with_details(
            &tool_name,
            duration_ms as i64,
            success,
            params.clone(),
            call_result,
            call_error,
            Some(tc.id.clone()),
        );

        if let Some(ref metrics) = ctx.metrics {
            match &result {
                Ok(Ok(tool_result)) if tool_result.success => {
                    metrics.tool().record_tool_call_complete(
                        &tool_name,
                        entity_id,
                        true,
                        duration_ms,
                        parameter_size,
                        json_size(tool_result.result.as_ref().unwrap_or(&Value::Null)),
                    );
                }
                Ok(Ok(_)) => {
                    metrics.tool().record_tool_call_complete(
                        &tool_name,
                        entity_id,
                        false,
                        duration_ms,
                        parameter_size,
                        0,
                    );
                    metrics.tool().record_tool_call_error(
                        &tool_name,
                        entity_id,
                        "execution_failed",
                    );
                    tracing::warn!(tool = %tool_name, "tool call reported failure");
                }
                Ok(Err(e)) => {
                    metrics.tool().record_tool_call_complete(
                        &tool_name,
                        entity_id,
                        false,
                        duration_ms,
                        parameter_size,
                        0,
                    );
                    metrics.tool().record_tool_call_error(
                        &tool_name,
                        entity_id,
                        "execution_failed",
                    );
                    tracing::warn!(tool = %tool_name, error = %e, "tool call failed");
                }
                Err(_) => {
                    metrics.tool().record_tool_call_complete(
                        &tool_name,
                        entity_id,
                        false,
                        duration_ms,
                        parameter_size,
                        0,
                    );
                    metrics
                        .tool()
                        .record_tool_call_error(&tool_name, entity_id, "timeout");
                    tracing::warn!(tool = %tool_name, "tool call timed out after {}ms", timeout_ms);
                }
            }
        }

        match result {
            Ok(Ok(tool_result)) if tool_result.success => {
                // cache the successful result as the replay idempotency
                // key before the marker is cleared.
                entity_state
                    .write()
                    .await
                    .finish_tool_call(&tc.id, tool_result.result.clone());
                if let Some(ref fp) = ctx.failure_protection {
                    fp.record_success(&tool_name);
                }
                if after {
                    if let Some(ref handler) = ctx.checkpoint_handler {
                        if let Err(e) = handler
                            .create_checkpoint(entity_id, &format!("after tool '{}'", tool_name))
                            .await
                        {
                            Self::emit_progress(
                                &ctx.progress_tx,
                                &tc.id,
                                ToolProgressStatus::Failed,
                                None,
                            );
                            return Err(format!(
                                "Checkpoint failed after tool '{}': {}",
                                tool_name, e
                            ));
                        }
                    }
                }
                let msg = Message {
                    id: wf_types::Id::new(),
                    role: MessageRole::Tool,
                    content: MessageContentValue::Text(
                        tool_result
                            .result
                            .as_ref()
                            .map(|v| v.to_string())
                            .unwrap_or_default(),
                    ),
                    timestamp: wf_common::now(),
                    tool_call_id: Some(tc.id.clone()),
                    tool_name: Some(tc.function.name.clone()),
                    tool_calls: None,
                    thinking: None,
                    metadata: None,
                };
                Self::emit_progress(
                    &ctx.progress_tx,
                    &tc.id,
                    ToolProgressStatus::Completed,
                    tool_result.result.clone(),
                );
                Ok(msg)
            }
            Ok(Ok(tool_result)) => {
                // Tool reported failure through its result payload.
                let reason = tool_result
                    .error
                    .clone()
                    .unwrap_or_else(|| format!("Tool '{}' reported failure", tool_name));
                entity_state.write().await.finish_tool_call(&tc.id, None);
                if let Some(ref fp) = ctx.failure_protection {
                    fp.record_failure(&tool_name, reason.clone());
                }
                Self::emit_progress(&ctx.progress_tx, &tc.id, ToolProgressStatus::Failed, None);
                Err(reason)
            }
            Ok(Err(e)) => {
                entity_state.write().await.finish_tool_call(&tc.id, None);
                if let Some(ref fp) = ctx.failure_protection {
                    fp.record_failure(&tool_name, e.to_string());
                }
                Self::emit_progress(&ctx.progress_tx, &tc.id, ToolProgressStatus::Failed, None);
                Err(e.to_string())
            }
            Err(_) => {
                entity_state.write().await.finish_tool_call(&tc.id, None);
                if let Some(ref fp) = ctx.failure_protection {
                    fp.record_failure(&tool_name, format!("timeout after {}ms", timeout_ms));
                }
                Self::emit_progress(&ctx.progress_tx, &tc.id, ToolProgressStatus::Failed, None);
                Err(format!(
                    "Tool '{}' timed out after {}ms",
                    tool_name, timeout_ms
                ))
            }
        }
    }

    /// Combine the entity abort signal with an optional external cancellation
    /// token. In parallel mode every task observes a child of this token.
    fn batch_cancellation(&self, entity: &AgentLoopEntity) -> CancellationToken {
        let entity_token = entity.get_abort_signal();
        match &self.cancellation {
            None => entity_token,
            Some(external) => {
                let batch = CancellationToken::new();
                let batch_clone = batch.clone();
                let external_clone = external.clone();
                tokio::spawn(async move {
                    tokio::select! {
                        _ = entity_token.cancelled() => batch_clone.cancel(),
                        _ = external_clone.cancelled() => batch_clone.cancel(),
                    }
                });
                batch
            }
        }
    }

    fn emit_progress(
        tx: &Option<tokio::sync::mpsc::Sender<ToolProgressEvent>>,
        tool_call_id: &str,
        status: ToolProgressStatus,
        partial: Option<Value>,
    ) {
        if let Some(tx) = tx {
            let _ = tx.try_send(ToolProgressEvent {
                tool_call_id: tool_call_id.to_string(),
                status,
                partial,
            });
        }
    }

    fn error_message(error: &str, tool_call_id: Option<&str>, tool_name: Option<&str>) -> Message {
        Message {
            id: wf_types::Id::new(),
            role: MessageRole::Tool,
            content: MessageContentValue::Text(serde_json::json!({"error": error}).to_string()),
            timestamp: wf_common::now(),
            tool_call_id: tool_call_id.map(String::from),
            tool_name: tool_name.map(String::from),
            tool_calls: None,
            thinking: None,
            metadata: None,
        }
    }

    fn build_hook_data(tc: &LlmToolCall) -> HashMap<String, Value> {
        let mut data = HashMap::new();
        data.insert("tool_call_id".to_string(), Value::String(tc.id.clone()));
        data.insert(
            "tool_name".to_string(),
            Value::String(tc.function.name.clone()),
        );
        data.insert(
            "tool_arguments".to_string(),
            Value::String(tc.function.arguments.clone()),
        );
        data
    }

    fn find_tool_id_by_name(registry: &ToolRegistry, name: &str) -> Option<String> {
        registry
            .list_tools()
            .into_iter()
            .find(|t| t.name == name)
            .map(|t| t.id)
    }
}

/// Per-run context backing the `general` tool.
///
/// Holds no reference to the coordinator (no Arc cycle): it snapshots the
/// immutable execution context and the run entity, then routes every inner
/// invocation through the shared [`ToolExecutionCoordinator::run_tool`]
/// pipeline so all controls (visibility, approval, checkpoint, failure
/// protection, timeout) apply exactly as to direct calls.
pub struct GeneralToolContext {
    ctx: ToolRunCtx,
    entity: Arc<AgentLoopEntity>,
    event_bus: Option<Arc<wf_core::EventBus>>,
}

impl GeneralToolContext {
    pub(crate) fn new(
        ctx: ToolRunCtx,
        entity: Arc<AgentLoopEntity>,
        event_bus: Option<Arc<wf_core::EventBus>>,
    ) -> Self {
        Self {
            ctx,
            entity,
            event_bus,
        }
    }

    /// Validate the inner tool against the run's exposure state using the
    /// same resolution the schema assembly consumes (single decision
    /// source): hidden tools are always rejected, gated (not yet activated)
    /// tools are rejected, and anything outside the resolved visible or
    /// discoverable buckets is rejected. The `general` tool itself cannot be
    /// invoked through `general` (recursion guard). Runtime blocks are
    /// enforced by the shared pipeline.
    async fn check_inner_tool_allowed(&self, tool_name: &str) -> Result<(), String> {
        if tool_name == wf_tools::general::GENERAL_TOOL_NAME {
            return Err(format!(
                "Tool '{}' cannot be invoked through the general tool; call the target tool directly",
                tool_name
            ));
        }
        // The hidden blocklist is the strongest exposure layer (mirrors
        // `effective_exposure`): a hidden tool is rejected even when it is
        // outside the available pool, which the pooled resolution below
        // filters out entirely.
        if self
            .entity
            .hidden_tool_names()
            .contains(&tool_name.to_string())
        {
            return Err(format!(
                "Tool '{}' is not callable in this execution",
                tool_name
            ));
        }
        let activated_tools = {
            let state = self.entity.state.read().await;
            state.tool_discovery().activated_tools.clone()
        };
        let resolution = wf_tools::resolve_tool_exposure(wf_tools::ExposureInput {
            registry: self.ctx.registry.as_ref(),
            available_names: self.entity.available_tool_names(),
            initial_names: self.entity.initial_tool_names(),
            discoverable_names: self.entity.discoverable_tool_names(),
            hidden_names: self.entity.hidden_tool_names(),
            enable_general_tool: self.entity.enable_general_tool(),
            activated_tools: &activated_tools,
            exposure_overrides: &self.entity.exposure_overrides().iter().cloned().collect(),
        });
        if resolution.hidden.iter().any(|t| t.name == tool_name) {
            return Err(format!(
                "Tool '{}' is not callable in this execution",
                tool_name
            ));
        }
        if resolution.gated.iter().any(|t| t.name == tool_name) {
            return Err(format!(
                "Tool '{}' is not activated yet; wait until it is explicitly enabled",
                tool_name
            ));
        }
        if !wf_tools::is_tool_callable(&resolution, tool_name) {
            return Err(format!(
                "Tool '{}' is not in the available tool set",
                tool_name
            ));
        }
        Ok(())
    }

    /// Execute one inner invocation through the shared pipeline.
    async fn invoke_inner(
        &self,
        call: &wf_types::message::LlmToolCall,
    ) -> wf_tools::ToolResult<serde_json::Value> {
        use wf_tools::error::ToolError;

        let tool_name = call.function.name.clone();
        self.check_inner_tool_allowed(&tool_name)
            .await
            .map_err(ToolError::ValidationFailed)?;

        let started = wf_common::now();
        let is_first_discovery = {
            let mut state = self.entity.state.write().await;
            state
                .tool_discovery_mut()
                .record_general_discovery(&tool_name)
        };

        let msg = ToolExecutionCoordinator::run_tool(
            &self.ctx,
            call,
            self.entity.id(),
            &self.entity.state,
        )
        .await
        .map_err(ToolError::ExecutionError)?;
        let duration_ms = (wf_common::now() - started) as f64;

        if let Some(ref metrics) = self.ctx.metrics {
            let success = !matches!(&msg.content, wf_types::message::MessageContentValue::Text(t) if t.contains("\"error\""));
            metrics
                .tool()
                .record_general_invoke(&tool_name, success, duration_ms);
            if is_first_discovery {
                metrics.tool().record_discovery(&tool_name, "general");
            }
        }

        if is_first_discovery {
            self.emit_discovery_event(&tool_name, "general");
        }

        let content = match &msg.content {
            wf_types::message::MessageContentValue::Text(t) => t.clone(),
            wf_types::message::MessageContentValue::Rich(_) => String::new(),
        };
        // Return the inner tool's native result shape when it was JSON.
        Ok(serde_json::from_str(&content).unwrap_or(serde_json::Value::String(content)))
    }

    fn emit_discovery_event(&self, tool_name: &str, method: &str) {
        let Some(bus) = self.event_bus.as_ref() else {
            return;
        };
        let _ = bus.publish(wf_types::events::BaseEvent {
            id: wf_types::Id::new(),
            r#type: wf_types::events::EventType::NodeCustomEvent,
            timestamp: wf_common::now(),
            workflow_id: None,
            execution_id: Some(self.entity.id().clone()),
            agent_loop_id: Some(self.entity.id().clone()),
            event_name: None,
            metadata: Some(std::collections::HashMap::from([
                (
                    "event".to_string(),
                    serde_json::Value::String("tool_discovery_state_changed".to_string()),
                ),
                (
                    "tool".to_string(),
                    serde_json::Value::String(tool_name.to_string()),
                ),
                (
                    "method".to_string(),
                    serde_json::Value::String(method.to_string()),
                ),
            ])),
        });
    }
}

#[async_trait::async_trait]
impl wf_tools::general::GeneralToolInvoker for GeneralToolContext {
    async fn invoke_request(&self, request: &str) -> wf_tools::ToolResult<serde_json::Value> {
        let calls = wf_llm::tool_call_parser::parse_invoke_json_calls(request);
        if calls.is_empty() {
            return Err(wf_tools::general::build_format_error());
        }

        let mut results = Vec::with_capacity(calls.len());
        for call in &calls {
            results.push(self.invoke_inner(call).await?);
        }
        if results.len() == 1 {
            Ok(results.pop().expect("len checked above"))
        } else {
            Ok(serde_json::Value::Array(results))
        }
    }
}

/// Serialized size of a value in bytes, used for tool parameter/result metrics.
fn json_size(value: &Value) -> u64 {
    serde_json::to_string(value)
        .map(|s| s.len() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use wf_types::tool::Tool;
    use wf_types::Id;

    fn mock_tool_registry(executed: &Arc<AtomicU32>) -> Arc<ToolRegistry> {
        let registry = Arc::new(ToolRegistry::new());
        let handler: wf_tools::executor::stateless::StatelessHandler = {
            let executed = executed.clone();
            Arc::new(
                move |_params: &Value,
                      _ctx: &wf_tools::executor::trait_def::ToolExecutionContext| {
                    executed.fetch_add(1, Ordering::SeqCst);
                    Ok(Value::from("tool-result-ok"))
                },
            )
        };
        registry.register_tool(Tool {
            id: "tool-1".to_string(),
            name: "mock_write".to_string(),
            description: "mock tool".to_string(),
            tool_type: wf_types::tool::ToolType::Stateless,
            parameters: None,
            metadata: Some(wf_types::tool::ToolMetadata {
                category: Some("mock".to_string()),
                tags: None,
                documentation_url: None,
                custom_fields: None,
                risk_level: Some(ToolRiskLevel::Write),
                auto_approvable: None,
                create_checkpoint: None,
                exposure: None,
            }),
            config: None,
            enabled: Some(true),
            strict: None,
            default_timeout_ms: Some(5000),
        });
        registry.register_stateless_handler("tool-1", handler);
        registry
    }

    fn make_tool_call(id: &str, name: &str) -> LlmToolCall {
        LlmToolCall {
            id: id.to_string(),
            r#type: "function".to_string(),
            function: wf_types::message::LlmFunctionCall {
                name: name.to_string(),
                arguments: "{}".to_string(),
            },
        }
    }

    fn text_of(msg: &Message) -> String {
        match &msg.content {
            MessageContentValue::Text(t) => t.clone(),
            MessageContentValue::Rich(_) => String::new(),
        }
    }

    fn make_entity() -> AgentLoopEntity {
        AgentLoopEntity::new(Id::from("agent-approval-1".to_string()))
    }

    #[tokio::test]
    async fn test_no_handler_auto_approves_and_executes() {
        let executed = Arc::new(AtomicU32::new(0));
        let registry = mock_tool_registry(&executed);
        let coordinator = ToolExecutionCoordinator::new(registry);
        let entity = make_entity();

        let messages = coordinator
            .execute_tool_calls(&entity, &[make_tool_call("tc-1", "mock_write")])
            .await
            .expect("tool execution must succeed");

        assert_eq!(messages.len(), 1);
        assert_eq!(executed.load(Ordering::SeqCst), 1);
        assert!(text_of(&messages[0]).contains("tool-result-ok"));
    }

    struct RejectingHandler {
        reason: String,
    }

    #[async_trait::async_trait]
    impl crate::approval::ToolApprovalHandler for RejectingHandler {
        async fn request_approval(
            &self,
            request: &crate::approval::ToolApprovalRequest,
        ) -> crate::approval::ToolApprovalResult {
            crate::approval::ToolApprovalResult::rejected(
                request.tool_call_id.clone(),
                self.reason.clone(),
            )
        }
    }

    struct ApprovingHandler;

    #[async_trait::async_trait]
    impl crate::approval::ToolApprovalHandler for ApprovingHandler {
        async fn request_approval(
            &self,
            request: &crate::approval::ToolApprovalRequest,
        ) -> crate::approval::ToolApprovalResult {
            crate::approval::ToolApprovalResult::approved(request.tool_call_id.clone())
        }
    }

    #[tokio::test]
    async fn test_rejecting_handler_blocks_tool_and_produces_message() {
        let executed = Arc::new(AtomicU32::new(0));
        let registry = mock_tool_registry(&executed);
        let coordinator = ToolExecutionCoordinator::new(registry).with_approval(
            None,
            Some(Arc::new(RejectingHandler {
                reason: "too risky".to_string(),
            })),
        );
        let entity = make_entity();

        let messages = coordinator
            .execute_tool_calls(&entity, &[make_tool_call("tc-1", "mock_write")])
            .await
            .expect("rejection must not fail the loop");

        // Tool never executed; a rejection tool message is produced.
        assert_eq!(executed.load(Ordering::SeqCst), 0);
        assert_eq!(messages.len(), 1);
        let content = text_of(&messages[0]);
        assert!(content.contains("too risky"));
        assert!(content.contains("mock_write"));
    }

    #[tokio::test]
    async fn test_approving_handler_allows_execution() {
        let executed = Arc::new(AtomicU32::new(0));
        let registry = mock_tool_registry(&executed);
        let coordinator = ToolExecutionCoordinator::new(registry)
            .with_approval(None, Some(Arc::new(ApprovingHandler)));
        let entity = make_entity();

        let messages = coordinator
            .execute_tool_calls(&entity, &[make_tool_call("tc-1", "mock_write")])
            .await
            .expect("approved tool must execute");

        assert_eq!(executed.load(Ordering::SeqCst), 1);
        assert_eq!(messages.len(), 1);
        assert!(text_of(&messages[0]).contains("tool-result-ok"));
    }

    #[tokio::test]
    async fn test_replayed_tool_call_id_is_served_from_cache() {
        let executed = Arc::new(AtomicU32::new(0));
        let registry = mock_tool_registry(&executed);
        let coordinator = ToolExecutionCoordinator::new(registry);
        let entity = make_entity();

        // First execution of the call id runs the tool and caches the result.
        let first = coordinator
            .execute_tool_calls(&entity, &[make_tool_call("tc-replay", "mock_write")])
            .await
            .expect("first execution succeeds");
        assert_eq!(executed.load(Ordering::SeqCst), 1);
        assert!(text_of(&first[0]).contains("tool-result-ok"));

        // A replayed call with the same id (crash/restore scenario) must NOT
        // re-execute the tool: the cached result is returned instead.
        let second = coordinator
            .execute_tool_calls(&entity, &[make_tool_call("tc-replay", "mock_write")])
            .await
            .expect("replay succeeds");
        assert_eq!(
            executed.load(Ordering::SeqCst),
            1,
            "tool must not run twice for the same call id"
        );
        assert!(text_of(&second[0]).contains("tool-result-ok"));
    }

    #[tokio::test]
    async fn test_in_flight_marker_cleared_after_execution() {
        let executed = Arc::new(AtomicU32::new(0));
        let registry = mock_tool_registry(&executed);
        let coordinator = ToolExecutionCoordinator::new(registry);
        let entity = make_entity();
        entity.state.write().await.start_iteration();

        coordinator
            .execute_tool_calls(&entity, &[make_tool_call("tc-live", "mock_write")])
            .await
            .unwrap();

        let state = entity.state.read().await;
        assert!(
            state.pending_tool_calls().is_empty(),
            "no call remains in flight after execution"
        );
        assert!(state.has_completed_tool_call("tc-live"));
        let recorded = state.iteration_history()[0]
            .tool_calls
            .iter()
            .find(|t| t.tool_call_id.as_deref() == Some("tc-live"));
        assert!(recorded.is_some(), "tool call id recorded in audit trail");
    }

    #[tokio::test]
    async fn test_parallel_approval_no_crosstalk() {
        let executed = Arc::new(AtomicU32::new(0));
        let registry = mock_tool_registry(&executed);
        // Second tool with the same underlying counter: only approved calls
        // reach execution.
        let registry2 = registry.clone();
        registry2.register_tool(Tool {
            id: "tool-2".to_string(),
            name: "mock_read".to_string(),
            description: "mock read".to_string(),
            tool_type: wf_types::tool::ToolType::Stateless,
            parameters: None,
            metadata: Some(wf_types::tool::ToolMetadata {
                category: Some("mock".to_string()),
                tags: None,
                documentation_url: None,
                custom_fields: None,
                risk_level: Some(ToolRiskLevel::ReadOnly),
                auto_approvable: None,
                create_checkpoint: None,
                exposure: None,
            }),
            config: None,
            enabled: Some(true),
            strict: None,
            default_timeout_ms: Some(5000),
        });
        {
            let handler: wf_tools::executor::stateless::StatelessHandler = {
                let executed = executed.clone();
                Arc::new(
                    move |_p: &Value, _c: &wf_tools::executor::trait_def::ToolExecutionContext| {
                        executed.fetch_add(1, Ordering::SeqCst);
                        Ok(Value::from("tool-result-ok"))
                    },
                )
            };
            registry2.register_stateless_handler("tool-2", handler);
        }
        let handler_executed = Arc::new(AtomicU32::new(0));
        let handler_executed_clone = handler_executed.clone();
        let handler = Arc::new(move |request: &crate::approval::ToolApprovalRequest| {
            handler_executed_clone.fetch_add(1, Ordering::SeqCst);
            crate::approval::ToolApprovalResult::approved(request.tool_call_id.clone())
        });

        struct FnHandler(
            Arc<
                dyn Fn(&crate::approval::ToolApprovalRequest) -> crate::approval::ToolApprovalResult
                    + Send
                    + Sync,
            >,
        );

        #[async_trait::async_trait]
        impl crate::approval::ToolApprovalHandler for FnHandler {
            async fn request_approval(
                &self,
                request: &crate::approval::ToolApprovalRequest,
            ) -> crate::approval::ToolApprovalResult {
                (self.0)(request)
            }
        }

        let coordinator = ToolExecutionCoordinator::new(registry.clone())
            .with_mode(ToolExecutionMode::Parallel)
            .with_approval(None, Some(Arc::new(FnHandler(handler))));
        let entity = make_entity();

        let messages = coordinator
            .execute_tool_calls(
                &entity,
                &[
                    make_tool_call("tc-1", "mock_write"),
                    make_tool_call("tc-2", "mock_read"),
                ],
            )
            .await
            .expect("parallel approval must not fail");

        assert_eq!(messages.len(), 2);
        // Both asked the handler (no auto approval with handler present).
        assert_eq!(handler_executed.load(Ordering::SeqCst), 2);
        // Both approved and executed exactly once.
        assert_eq!(executed.load(Ordering::SeqCst), 2);
        assert!(messages
            .iter()
            .all(|m| text_of(m).contains("tool-result-ok")));
    }

    // ---- orchestration enhancements ----

    #[tokio::test]
    async fn test_predefined_read_file_auto_approved_under_safe_preset() {
        let registry = Arc::new(ToolRegistry::new());
        let tool = wf_tools::predefined::filesystem::READ_FILE.tool_def();
        let tool_name = tool.name.clone();
        let handler: wf_tools::executor::stateless::StatelessHandler = Arc::new(
            move |_p: &Value, _c: &wf_tools::executor::trait_def::ToolExecutionContext| {
                Ok(Value::from(format!("content of {}", tool_name)))
            },
        );
        registry.register_tool(tool);
        registry.register_stateless_handler("read_file", handler);

        let options = ToolApprovalOptions {
            auto_approval_enabled: Some(true),
            security_preset: Some(wf_types::tool::approval::SecurityPreset::Safe),
            risk_threshold: None,
            auto_approve_patterns: None,
            categories: None,
            workspace_boundary: None,
            file_permissions: None,
            command: None,
            mcp: None,
            network: None,
            interaction: None,
            allow_write_protected: None,
        };
        let coordinator =
            ToolExecutionCoordinator::new(registry).with_approval(Some(options), None);
        let entity = make_entity();

        let mut tc = make_tool_call("tc-read", "read_file");
        tc.function.arguments = serde_json::json!({ "path": "/tmp/readme.md" }).to_string();

        let messages = coordinator
            .execute_tool_calls(&entity, &[tc])
            .await
            .expect("read-only tool must be auto-approved and executed");

        assert_eq!(messages.len(), 1);
        assert!(text_of(&messages[0]).contains("content of read_file"));
    }

    #[tokio::test]
    async fn test_progress_events_emitted() {
        let executed = Arc::new(AtomicU32::new(0));
        let registry = mock_tool_registry(&executed);
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        let coordinator = ToolExecutionCoordinator::new(registry).with_progress_tx(Some(tx));
        let entity = make_entity();

        let messages = coordinator
            .execute_tool_calls(&entity, &[make_tool_call("tc-1", "mock_write")])
            .await
            .expect("execution must succeed");

        assert_eq!(messages.len(), 1);
        assert_eq!(executed.load(Ordering::SeqCst), 1);
        let mut statuses = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            assert_eq!(ev.tool_call_id, "tc-1");
            statuses.push(ev.status);
        }
        assert_eq!(
            statuses,
            vec![ToolProgressStatus::Started, ToolProgressStatus::Completed]
        );
    }

    struct BlockingVisibilityStore;

    #[async_trait]
    impl ToolVisibilityStore for BlockingVisibilityStore {
        async fn is_tool_visible(&self, _execution_id: &str, _tool_name: &str) -> bool {
            false
        }
    }

    #[tokio::test]
    async fn test_visibility_gate_blocks_tool() {
        let executed = Arc::new(AtomicU32::new(0));
        let registry = mock_tool_registry(&executed);
        let coordinator = ToolExecutionCoordinator::new(registry)
            .with_visibility_store(Some(Arc::new(BlockingVisibilityStore)));
        let entity = make_entity();

        let messages = coordinator
            .execute_tool_calls(&entity, &[make_tool_call("tc-1", "mock_write")])
            .await
            .expect("visibility rejection must not fail the loop");

        assert_eq!(executed.load(Ordering::SeqCst), 0);
        assert_eq!(messages.len(), 1);
        assert!(text_of(&messages[0]).contains("not visible"));
    }

    fn mock_failing_registry() -> Arc<ToolRegistry> {
        let registry = Arc::new(ToolRegistry::new());
        let handler: wf_tools::executor::stateless::StatelessHandler = Arc::new(
            move |_p: &Value, _c: &wf_tools::executor::trait_def::ToolExecutionContext| {
                Err(wf_tools::error::ToolError::ExecutionFailed {
                    tool_id: "fail-id".to_string(),
                    reason: "boom".to_string(),
                })
            },
        );
        registry.register_tool(Tool {
            id: "fail-id".to_string(),
            name: "mock_fail".to_string(),
            description: "mock failing tool".to_string(),
            tool_type: wf_types::tool::ToolType::Stateless,
            parameters: None,
            metadata: Some(wf_types::tool::ToolMetadata {
                category: Some("mock".to_string()),
                tags: None,
                documentation_url: None,
                custom_fields: None,
                risk_level: Some(ToolRiskLevel::Write),
                auto_approvable: None,
                create_checkpoint: None,
                exposure: None,
            }),
            config: None,
            enabled: Some(true),
            strict: None,
            default_timeout_ms: Some(5000),
        });
        registry.register_stateless_handler("fail-id", handler);
        registry
    }

    #[tokio::test]
    async fn test_failure_protection_blocks_after_consecutive_failures() {
        let registry = mock_failing_registry();
        let protection = Arc::new(ToolFailureProtectionState::new(
            wf_tools::failure_protection::ToolFailureProtectionConfig {
                max_consecutive_failures: 2,
                cooldown_period: Duration::from_secs(60),
                enabled: true,
            },
        ));
        let coordinator = ToolExecutionCoordinator::new(registry)
            .with_failure_protection(Some(protection.clone()));
        let entity = make_entity();

        // First two executions are allowed and record failures.
        for _ in 0..2 {
            let messages = coordinator
                .execute_tool_calls(&entity, &[make_tool_call("tc-x", "mock_fail")])
                .await
                .expect("execution must not fail");
            assert_eq!(messages.len(), 1);
        }
        assert!(protection.is_blocked("mock_fail"));

        // The third execution is blocked by the protection gate.
        let messages = coordinator
            .execute_tool_calls(&entity, &[make_tool_call("tc-y", "mock_fail")])
            .await
            .expect("blocked execution must not fail");
        assert_eq!(messages.len(), 1);
        assert!(text_of(&messages[0]).contains("blocked"));
    }

    struct CountingCheckpointHandler {
        before: Arc<AtomicU32>,
        after: Arc<AtomicU32>,
    }

    #[async_trait]
    impl ToolCheckpointHandler for CountingCheckpointHandler {
        async fn create_checkpoint(&self, _execution_id: &str, reason: &str) -> AgentResult<()> {
            if reason.starts_with("before") {
                self.before.fetch_add(1, Ordering::SeqCst);
            } else {
                self.after.fetch_add(1, Ordering::SeqCst);
            }
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_checkpoint_before_and_after() {
        let executed = Arc::new(AtomicU32::new(0));
        let registry = mock_tool_registry(&executed);
        let mut tool = registry.get_tool("tool-1").unwrap();
        tool.metadata.as_mut().unwrap().create_checkpoint = Some(CheckpointTiming::Both);
        registry.register_tool(tool);

        let before = Arc::new(AtomicU32::new(0));
        let after = Arc::new(AtomicU32::new(0));
        let handler = Arc::new(CountingCheckpointHandler {
            before: before.clone(),
            after: after.clone(),
        });
        let coordinator =
            ToolExecutionCoordinator::new(registry).with_checkpoint_handler(Some(handler));
        let entity = make_entity();

        let messages = coordinator
            .execute_tool_calls(&entity, &[make_tool_call("tc-1", "mock_write")])
            .await
            .expect("checkpoint-enabled execution must succeed");

        assert_eq!(messages.len(), 1);
        assert_eq!(executed.load(Ordering::SeqCst), 1);
        assert_eq!(before.load(Ordering::SeqCst), 1);
        assert_eq!(after.load(Ordering::SeqCst), 1);
    }

    fn mock_mixed_registry() -> Arc<ToolRegistry> {
        let registry = Arc::new(ToolRegistry::new());
        registry.register_tool(Tool {
            id: "fail-id".to_string(),
            name: "mock_fail".to_string(),
            description: "mock failing tool".to_string(),
            tool_type: wf_types::tool::ToolType::Stateless,
            parameters: None,
            metadata: Some(wf_types::tool::ToolMetadata {
                category: Some("mock".to_string()),
                tags: None,
                documentation_url: None,
                custom_fields: None,
                risk_level: Some(ToolRiskLevel::Write),
                auto_approvable: None,
                create_checkpoint: None,
                exposure: None,
            }),
            config: None,
            enabled: Some(true),
            strict: None,
            default_timeout_ms: Some(5000),
        });
        registry.register_stateless_handler(
            "fail-id",
            Arc::new(
                move |_p: &Value, _c: &wf_tools::executor::trait_def::ToolExecutionContext| {
                    Err(wf_tools::error::ToolError::ExecutionFailed {
                        tool_id: "fail-id".to_string(),
                        reason: "boom".to_string(),
                    })
                },
            ),
        );
        registry.register_tool(Tool {
            id: "slow-id".to_string(),
            name: "mock_slow".to_string(),
            description: "slow mock tool".to_string(),
            tool_type: wf_types::tool::ToolType::Stateless,
            parameters: None,
            metadata: Some(wf_types::tool::ToolMetadata {
                category: Some("mock".to_string()),
                tags: None,
                documentation_url: None,
                custom_fields: None,
                risk_level: Some(ToolRiskLevel::ReadOnly),
                auto_approvable: None,
                create_checkpoint: None,
                exposure: None,
            }),
            config: None,
            enabled: Some(true),
            strict: None,
            default_timeout_ms: Some(5000),
        });
        registry.register_stateless_async_handler(
            "slow-id",
            Arc::new(
                |_p: Value, _c: wf_tools::executor::trait_def::ToolExecutionContext| {
                    Box::pin(async move {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                        Ok(Value::from("tool-result-ok"))
                    })
                },
            ),
        );
        registry
    }

    #[tokio::test]
    async fn test_parallel_cancel_on_failure_aborts_batch() {
        let registry = mock_mixed_registry();
        let coordinator = ToolExecutionCoordinator::new(registry)
            .with_mode(ToolExecutionMode::Parallel)
            .with_cancel_on_failure(true);
        let entity = make_entity();

        let messages = coordinator
            .execute_tool_calls(
                &entity,
                &[
                    make_tool_call("tc-fail", "mock_fail"),
                    make_tool_call("tc-slow", "mock_slow"),
                ],
            )
            .await
            .expect("parallel execution must not fail");

        assert_eq!(messages.len(), 2);
        let texts: Vec<String> = messages.iter().map(text_of).collect();
        assert!(
            texts.iter().any(|t| t.contains("boom")),
            "failing tool must surface its error: {:?}",
            texts
        );
        assert!(
            texts.iter().any(|t| t.contains("did not complete")),
            "aborted tool must be reported: {:?}",
            texts
        );
    }

    // ── general tool invoker ─────────────────────────────────────────

    use wf_tools::general::GeneralToolInvoker;

    fn echo_registry() -> Arc<ToolRegistry> {
        let registry = Arc::new(ToolRegistry::new());
        let handler: wf_tools::executor::stateless::StatelessHandler = Arc::new(
            |params: &Value, _ctx: &wf_tools::executor::trait_def::ToolExecutionContext| {
                Ok(serde_json::json!({ "echo": params }))
            },
        );
        registry.register_tool(Tool {
            id: "web_search".to_string(),
            name: "web_search".to_string(),
            description: "Search the web".to_string(),
            tool_type: wf_types::tool::ToolType::Stateless,
            parameters: Some(wf_types::tool::ToolParameterSchema {
                r#type: "object".to_string(),
                properties: std::collections::BTreeMap::from([(
                    "query".to_string(),
                    wf_types::tool::ToolPropertySchema::typed("string"),
                )]),
                required: vec!["query".to_string()],
                additional_properties: Some(false),
            }),
            metadata: None,
            config: None,
            enabled: Some(true),
            strict: None,
            default_timeout_ms: Some(5000),
        });
        registry.register_stateless_handler("web_search", handler);
        registry
    }

    fn general_entity(registry: &ToolRegistry) -> Arc<AgentLoopEntity> {
        let entity = Arc::new(
            AgentLoopEntity::new(Id::from("exec-general-1".to_string()))
                .with_available_tool_names(vec!["web_search".to_string(), "write_file".to_string()])
                .with_initial_tool_names(vec!["web_search".to_string()])
                .with_discoverable_tool_names(vec!["web_search".to_string()]),
        );
        // Ensure the inner tool is registered (mirrors the pipeline).
        assert!(registry.list_tools().iter().any(|t| t.name == "web_search"));
        entity
    }

    fn general_ctx(
        registry: Arc<ToolRegistry>,
        entity: Arc<AgentLoopEntity>,
    ) -> GeneralToolContext {
        let run_ctx = ToolRunCtx {
            registry,
            metrics: None,
            progress_tx: None,
            checkpoint_handler: None,
            failure_protection: None,
            visibility_store: None,
            general_invoker: None,
            retry_budget: None,
        };
        GeneralToolContext::new(run_ctx, entity, None)
    }

    #[tokio::test]
    async fn test_general_invoke_returns_inner_tool_native_result() {
        let registry = echo_registry();
        let entity = general_entity(&registry);
        let ctx = general_ctx(registry, entity);

        let result = ctx
            .invoke_request(
                "{\"tool\": \"web_search\", \"parameters\": {\"query\": \"rust 异步\"}}",
            )
            .await
            .expect("general invoke must succeed");
        assert_eq!(
            result,
            serde_json::json!({ "echo": { "query": "rust 异步" } })
        );
    }

    #[tokio::test]
    async fn test_general_parse_error_returns_format_hint() {
        let registry = echo_registry();
        let entity = general_entity(&registry);
        let ctx = general_ctx(registry, entity);

        for request in ["", "plain text", "{\"tool\": 123}"] {
            let err = ctx.invoke_request(request).await.unwrap_err();
            assert!(
                err.to_string().contains("\"tool\""),
                "parse errors must carry the format hint: {err}"
            );
        }
    }

    #[tokio::test]
    async fn test_general_rejects_hidden_and_non_whitelisted_tools() {
        let registry = echo_registry();
        registry.register_tool(Tool {
            id: "secret_admin".to_string(),
            name: "secret_admin".to_string(),
            description: "hidden admin".to_string(),
            tool_type: wf_types::tool::ToolType::Stateless,
            parameters: None,
            metadata: None,
            config: None,
            enabled: Some(true),
            strict: None,
            default_timeout_ms: Some(5000),
        });
        let handler: wf_tools::executor::stateless::StatelessHandler = Arc::new(
            |_p: &Value, _c: &wf_tools::executor::trait_def::ToolExecutionContext| {
                Ok(Value::from("admin-ok"))
            },
        );
        registry.register_stateless_handler("secret_admin", handler);

        let entity = Arc::new(
            AgentLoopEntity::new(Id::from("exec-general-2".to_string()))
                .with_available_tool_names(vec!["web_search".to_string()])
                .with_initial_tool_names(vec!["web_search".to_string()])
                .with_discoverable_tool_names(vec!["web_search".to_string()])
                .with_hidden_tool_names(vec!["secret_admin".to_string()]),
        );
        let ctx = general_ctx(registry, entity);

        let hidden = ctx
            .invoke_request("{\"tool\": \"secret_admin\", \"parameters\": {\"x\": 1}}")
            .await
            .unwrap_err();
        assert!(hidden.to_string().contains("not callable"));

        let outside = ctx
            .invoke_request("{\"tool\": \"write_file\", \"parameters\": {\"path\": \"a\"}}")
            .await
            .unwrap_err();
        assert!(outside
            .to_string()
            .contains("not in the available tool set"));
    }

    #[tokio::test]
    async fn test_general_rejects_gated_tool_until_activated() {
        // write_file is gated: available but neither initial nor discoverable.
        let registry = echo_registry();
        registry.register_tool(Tool {
            id: "write_file".to_string(),
            name: "write_file".to_string(),
            description: "write".to_string(),
            tool_type: wf_types::tool::ToolType::Stateless,
            parameters: None,
            metadata: None,
            config: None,
            enabled: Some(true),
            strict: None,
            default_timeout_ms: Some(5000),
        });
        let handler: wf_tools::executor::stateless::StatelessHandler = Arc::new(
            |_p: &Value, _c: &wf_tools::executor::trait_def::ToolExecutionContext| {
                Ok(Value::from("written"))
            },
        );
        registry.register_stateless_handler("write_file", handler);

        let entity = general_entity(&registry);
        let ctx = general_ctx(registry, entity.clone());

        let before = ctx
            .invoke_request("{\"tool\": \"write_file\", \"parameters\": {\"path\": \"a.txt\"}}")
            .await
            .unwrap_err();
        assert!(before.to_string().contains("not activated"));

        // Formal activation (TOOL_VISIBILITY unblock) allows the call.
        entity
            .state
            .write()
            .await
            .tool_discovery_mut()
            .activate_tool("write_file");
        let after = ctx
            .invoke_request("{\"tool\": \"write_file\", \"parameters\": {\"path\": \"a.txt\"}}")
            .await
            .expect("activated gated tool must be invokable");
        assert_eq!(after, serde_json::json!("written"));
    }

    #[tokio::test]
    async fn test_general_rejects_self_invocation() {
        let registry = echo_registry();
        let entity = general_entity(&registry);
        let ctx = general_ctx(registry, entity);

        let err = ctx
            .invoke_request("{\"tool\": \"general\", \"parameters\": {\"request\": \"{}\"}}")
            .await
            .unwrap_err();
        assert!(
            err.to_string()
                .contains("cannot be invoked through the general tool"),
            "self invocation must be rejected: {err}"
        );
    }

    #[tokio::test]
    async fn test_general_invokes_metadata_discoverable_tool() {
        // Discoverability from tool metadata (not the config list) must be
        // honored by the runtime gate: the assembly injects the metadata and
        // enables `general`, so the call must not be rejected as gated.
        let registry = echo_registry();
        registry.register_tool(Tool {
            id: "beta_db".to_string(),
            name: "beta_db".to_string(),
            description: "db tool".to_string(),
            tool_type: wf_types::tool::ToolType::Stateless,
            parameters: None,
            metadata: Some(wf_types::tool::ToolMetadata {
                category: None,
                tags: None,
                documentation_url: None,
                custom_fields: None,
                risk_level: Some(ToolRiskLevel::ReadOnly),
                auto_approvable: None,
                create_checkpoint: None,
                exposure: Some(wf_types::tool::ToolExposure::Discoverable),
            }),
            config: None,
            enabled: Some(true),
            strict: None,
            default_timeout_ms: Some(5000),
        });
        let handler: wf_tools::executor::stateless::StatelessHandler = Arc::new(
            |_p: &Value, _c: &wf_tools::executor::trait_def::ToolExecutionContext| {
                Ok(Value::from("db-ok"))
            },
        );
        registry.register_stateless_handler("beta_db", handler);

        let entity = Arc::new(
            AgentLoopEntity::new(Id::from("exec-general-3".to_string()))
                .with_available_tool_names(vec!["web_search".to_string(), "beta_db".to_string()])
                .with_initial_tool_names(vec!["web_search".to_string()])
                .with_discoverable_tool_names(vec!["web_search".to_string()]),
        );
        let ctx = general_ctx(registry, entity);

        let result = ctx
            .invoke_request("{\"tool\": \"beta_db\", \"parameters\": {\"q\": 1}}")
            .await
            .expect("metadata-discoverable tool must be invokable via general");
        assert_eq!(result, serde_json::json!("db-ok"));
    }

    #[tokio::test]
    async fn test_general_blocked_tool_rejected_by_pipeline() {
        let registry = echo_registry();
        let entity = general_entity(&registry);
        let run_ctx = ToolRunCtx {
            registry,
            metrics: None,
            progress_tx: None,
            checkpoint_handler: None,
            failure_protection: None,
            visibility_store: Some(Arc::new(BlockingVisibilityStore)),
            general_invoker: None,
            retry_budget: None,
        };
        let ctx = GeneralToolContext::new(run_ctx, entity, None);

        let err = ctx
            .invoke_request("{\"tool\": \"web_search\", \"parameters\": {\"query\": \"x\"}}")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("not visible"));
    }

    #[tokio::test]
    async fn test_general_records_discovery_state() {
        let registry = echo_registry();
        let entity = general_entity(&registry);
        let ctx = general_ctx(registry, entity.clone());

        let _ = ctx
            .invoke_request("{\"tool\": \"web_search\", \"parameters\": {\"query\": \"rust\"}}")
            .await
            .expect("invoke must succeed");

        let state = entity.state.read().await;
        assert!(state
            .tool_discovery()
            .discovered_via_general
            .contains("web_search"));
    }
}
