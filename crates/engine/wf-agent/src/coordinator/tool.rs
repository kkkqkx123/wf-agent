use std::sync::Arc;

use serde_json::Value;
use tokio_util::sync::CancellationToken;

use wf_execution_shared::hooks::HookContext;
use wf_execution_shared::hooks::HookHandlerRegistry;
use wf_execution_shared::types::execution_entity::ExecutionEntity;
use wf_metrics::MetricsRegistry;
use wf_tools::registry::ToolRegistry;
use wf_types::message::{LlmToolCall, Message, MessageContentValue, MessageRole};
use wf_types::tool::approval::ToolApprovalOptions;

use crate::approval::RejectionMessageBuilder;
use crate::entity::AgentLoopEntity;
use crate::error::AgentResult;
use crate::hook::AgentHookEmitter;

mod approval;
mod general;
mod runner;
#[cfg(test)]
mod test;
mod types;

use approval::ToolApprovalGate;
use runner::{build_hook_data, error_message, run_tool};
use types::{ApprovalOutcome, TaskOutcome, ToolRunCtx};

// Facade re-exports: `coordinator::tool` stays the single public path for
// tool-execution types consumed by sibling modules and external crates.
pub use general::GeneralToolContext;
pub use types::{
    ToolCheckpointHandler, ToolExecutionMode, ToolProgressEvent, ToolProgressStatus,
    ToolVisibilityStore,
};

pub struct ToolExecutionCoordinator {
    tool_registry: Arc<ToolRegistry>,
    event_bus: Option<Arc<wf_core::EventBus>>,
    /// Shared hook receiver registry; hook points dispatch through it.
    hook_handler_registry: Option<Arc<HookHandlerRegistry>>,
    mode: ToolExecutionMode,
    metrics: Option<Arc<MetricsRegistry>>,
    approval: ToolApprovalGate,
    rejection_builder: RejectionMessageBuilder,
    progress_tx: Option<tokio::sync::mpsc::Sender<ToolProgressEvent>>,
    cancellation: Option<CancellationToken>,
    cancel_on_failure: bool,
    visibility_store: Option<Arc<dyn ToolVisibilityStore>>,
    checkpoint_handler: Option<Arc<dyn types::ToolCheckpointHandler>>,
    failure_protection: Option<Arc<wf_tools::failure_protection::ToolFailureProtectionState>>,
    /// Per-run `general` tool invoker. Injected once when the run starts
    /// (set after the coordinator is assembled); carried into every
    /// execution context snapshot so the builtin `general` handler resolves
    /// its invoker from the context instead of global per-execution state.
    general_invoker: Arc<std::sync::Mutex<Option<Arc<dyn wf_tools::general::GeneralToolInvoker>>>>,
    retry_budget: Option<Arc<wf_common::retry::RetryBudget>>,
    /// File-content observer (agent actor partition). Independent from the
    /// execution-state `checkpoint_handler` above.
    checkpoint_session: Option<wf_checkpoint::CheckpointSession>,
}

impl ToolExecutionCoordinator {
    pub fn new(tool_registry: Arc<ToolRegistry>) -> Self {
        Self {
            tool_registry,
            event_bus: None,
            hook_handler_registry: None,
            mode: ToolExecutionMode::default(),
            metrics: None,
            approval: ToolApprovalGate::new(None, None),
            rejection_builder: RejectionMessageBuilder::new(),
            progress_tx: None,
            cancellation: None,
            cancel_on_failure: false,
            visibility_store: None,
            checkpoint_handler: None,
            failure_protection: None,
            general_invoker: Arc::new(std::sync::Mutex::new(None)),
            retry_budget: None,
            checkpoint_session: None,
        }
    }

    /// Attach the event bus tool-call hook events are published to.
    pub fn with_event_bus(mut self, event_bus: Option<Arc<wf_core::EventBus>>) -> Self {
        self.event_bus = event_bus;
        self
    }

    /// Inject the shared hook receiver registry; tool-call hooks dispatch
    /// through it (synchronous receiver notification + audit event).
    pub fn with_hook_handler_registry(
        mut self,
        registry: Option<Arc<HookHandlerRegistry>>,
    ) -> Self {
        self.hook_handler_registry = registry;
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

    /// Enable execution-state snapshot creation around tools whose metadata
    /// opts in via `create_checkpoint`. Default: no checkpoints. This is
    /// unrelated to file-content checkpoints (see `with_file_observer`):
    /// it snapshots the execution record, never file bytes.
    pub fn with_checkpoint_handler(
        mut self,
        handler: Option<Arc<dyn types::ToolCheckpointHandler>>,
    ) -> Self {
        self.checkpoint_handler = handler;
        self
    }

    /// Inject the file-content observer (agent actor partition). Kept as an
    /// independent capability so callers cannot mistake execution-state
    /// snapshots for file-content checkpoints.
    pub fn with_checkpoint_session(
        mut self,
        session: Option<wf_checkpoint::CheckpointSession>,
    ) -> Self {
        self.checkpoint_session = session;
        self
    }

    /// Enable failure protection: tools are blocked after a configurable
    /// number of consecutive failures, and successes reset the counter.
    /// Default: disabled.
    pub fn with_failure_protection(
        mut self,
        state: Option<Arc<wf_tools::failure_protection::ToolFailureProtectionState>>,
    ) -> Self {
        self.failure_protection = state;
        self
    }

    /// Enforce a shared retry budget on tool failure retries: each failed
    /// attempt consumes the budget; when exhausted retries are abandoned and
    /// the failure reason is returned. Default: no budget (fail on first
    /// error, matching the historical single-attempt behavior).
    pub fn with_retry_budget(mut self, budget: Option<Arc<wf_common::retry::RetryBudget>>) -> Self {
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
        handler: Option<Arc<dyn crate::approval::ToolApprovalHandler>>,
    ) -> Self {
        self.approval = ToolApprovalGate::new(options, handler);
        self
    }

    /// Current approval wiring (options + handler); lets callers rebuild
    /// the coordinator without silently dropping the approval contract.
    pub fn approval_config(
        &self,
    ) -> (
        Option<ToolApprovalOptions>,
        Option<Arc<dyn crate::approval::ToolApprovalHandler>>,
    ) {
        self.approval.config()
    }

    /// Current file observer wiring; lets coordinator rebuilds preserve the
    /// file-content observation contract.
    pub fn checkpoint_session_config(&self) -> Option<wf_checkpoint::CheckpointSession> {
        self.checkpoint_session.clone()
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
            checkpoint_session: self.checkpoint_session.clone(),
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

    /// Apply approval-edited parameters to a tool call copy.
    fn apply_edited_parameters(tc: &LlmToolCall, edited_parameters: &Option<Value>) -> LlmToolCall {
        let mut tc = tc.clone();
        if let Some(edited) = edited_parameters {
            tc.function.arguments = serde_json::to_string(edited).unwrap_or(tc.function.arguments);
        }
        tc
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
            .approval
            .approve_tool_calls(entity, std::slice::from_ref(tc), &self.tool_registry)
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
        let outcomes = self
            .approval
            .approve_tool_calls(entity, tool_calls, &self.tool_registry)
            .await;
        let mut messages = Vec::with_capacity(tool_calls.len());

        for (idx, tc) in tool_calls.iter().enumerate() {
            let outcome = &outcomes[idx];
            match outcome {
                ApprovalOutcome::Rejected { reason } => {
                    // The call never executes, so it has no tool lifecycle:
                    // no BEFORE/AFTER hook fires (the parallel path never
                    // fired them here either). The denial is observable
                    // through the rejection message itself.
                    messages.push(self.build_rejection_message(tc, reason));
                    continue;
                }
                ApprovalOutcome::Execute { edited_parameters } => {
                    let tc = Self::apply_edited_parameters(tc, edited_parameters);
                    // BEFORE_TOOL_CALL is a gate point: a veto denies the
                    // call exactly like an approval rejection (same
                    // rejection message, same error-carrying AFTER fire).
                    let before = AgentHookEmitter::fire_agent_point(
                        entity,
                        "BEFORE_TOOL_CALL",
                        build_hook_data(&tc),
                        self.hook_handler_registry.as_deref(),
                        self.event_bus.as_deref(),
                    )
                    .await;
                    if let Some(reason) = before.vetoed_reason() {
                        let reason = format!("hook veto at BEFORE_TOOL_CALL: {reason}");
                        let msg = self.build_rejection_message(&tc, &reason);
                        let mut hook_data = build_hook_data(&tc);
                        hook_data.insert("error".to_string(), Value::String(reason.clone()));
                        AgentHookEmitter::fire_agent_point(
                            entity,
                            "AFTER_TOOL_CALL",
                            hook_data,
                            self.hook_handler_registry.as_deref(),
                            self.event_bus.as_deref(),
                        )
                        .await;
                        messages.push(msg);
                        continue;
                    }

                    let msg = self.execute_single_tool(entity, &tc).await?;

                    AgentHookEmitter::fire_agent_point(
                        entity,
                        "AFTER_TOOL_CALL",
                        build_hook_data(&tc),
                        self.hook_handler_registry.as_deref(),
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
        let outcomes = self
            .approval
            .approve_tool_calls(entity, tool_calls, &self.tool_registry)
            .await;
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
                    let tool_call = Self::apply_edited_parameters(tc, edited_parameters);
                    let run_ctx = run_ctx.clone();
                    let event_bus = self.event_bus.clone();
                    let hook_handler_registry = self.hook_handler_registry.clone();
                    let entity_state = entity.state.clone();
                    let entity_hooks = entity.hooks().to_vec();
                    let entity_id = entity.id().clone();
                    let task_cancellation = batch_cancellation.child_token();

                    set.spawn(async move {
                        let hook_data = build_hook_data(&tool_call);
                        let before_ctx = HookContext {
                            execution_id: entity_id.clone(),
                            hook_type: "BEFORE_TOOL_CALL".to_string(),
                            data: hook_data.clone(),
                        };

                        let before = AgentHookEmitter::fire_point(
                            &entity_hooks,
                            "BEFORE_TOOL_CALL",
                            &before_ctx,
                            hook_handler_registry.as_deref(),
                            event_bus.as_deref(),
                        )
                        .await;

                        // BEFORE_TOOL_CALL is a gate point: a veto denies
                        // the call without running it. The denial surfaces
                        // as a task failure (same channel as execution
                        // errors) with an error-carrying AFTER fire, mirroring
                        // the sequential path's rejection handling.
                        if let Some(reason) = before.vetoed_reason() {
                            let reason = format!("hook veto at BEFORE_TOOL_CALL: {reason}");
                            let mut hook_data = hook_data;
                            hook_data.insert("error".to_string(), Value::String(reason.clone()));
                            let after_ctx = HookContext {
                                execution_id: entity_id.clone(),
                                hook_type: "AFTER_TOOL_CALL".to_string(),
                                data: hook_data,
                            };
                            AgentHookEmitter::fire_point(
                                &entity_hooks,
                                "AFTER_TOOL_CALL",
                                &after_ctx,
                                hook_handler_registry.as_deref(),
                                event_bus.as_deref(),
                            )
                            .await;
                            return (idx, TaskOutcome::Failed(reason));
                        }

                        let result = tokio::select! {
                            res = run_tool(
                                &run_ctx,
                                &tool_call,
                                &entity_id,
                                &entity_state,
                            ) => res,
                            _ = task_cancellation.cancelled() => Err(
                                "Tool execution was cancelled".to_string()
                            ),
                        };

                        let after_ctx = HookContext {
                            execution_id: entity_id.clone(),
                            hook_type: "AFTER_TOOL_CALL".to_string(),
                            data: hook_data,
                        };
                        AgentHookEmitter::fire_point(
                            &entity_hooks,
                            "AFTER_TOOL_CALL",
                            &after_ctx,
                            hook_handler_registry.as_deref(),
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
                        messages[idx] = Some(error_message(&reason, None, None));
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
                    *slot = Some(error_message(
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
                error_message(&e.to_string(), Some(&tc.id), Some(&tc.function.name))
            })
    }

    async fn execute_single_tool(
        &self,
        entity: &AgentLoopEntity,
        tc: &LlmToolCall,
    ) -> AgentResult<Message> {
        let ctx = self.run_ctx();
        Ok(run_tool(&ctx, tc, entity.id(), &entity.state)
            .await
            .unwrap_or_else(|reason| error_message(&reason, Some(&tc.id), Some(&tc.function.name))))
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
}
