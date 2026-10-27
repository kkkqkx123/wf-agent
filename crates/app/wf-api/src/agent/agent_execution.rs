use std::sync::Arc;

use wf_agent::approval::ToolApprovalHandler;
use wf_agent::coordinator::lifecycle::AgentLoopCoordinator;
use wf_agent::entity::AgentLoopEntity;
use wf_agent::registry::AgentLoopRegistry;
use wf_tools::callback::{AgentLoopConfig, AgentLoopInput};

use crate::infra::context::ApiContext;
use crate::infra::error::ApiError;

/// Default wall-clock timeout for an agent loop when the config sets neither
/// `max_execution_time` nor `max_iterations` (30s per iteration, 3 default
/// iterations). An elapse maps onto `ApiError::Timeout`.
const DEFAULT_AGENT_TIMEOUT_MS: u64 = 90_000;

/// Parameters for running an agent loop.
#[derive(Clone)]
pub struct RunAgentLoopParams {
    /// Agent loop config (model, tool visibility, token limits, ...).
    pub config: AgentLoopConfig,
    /// Loop input (initial user message, context, imported conversation).
    pub input: AgentLoopInput,
    /// Caller-preset agent loop id (per-run execution identifier). When
    /// absent a fresh id is generated per run; presetting lets callers
    /// correlate diagnostics / control handles before the first event.
    pub agent_loop_id: Option<wf_types::Id>,
    /// Optional engine policy evaluated before the handler: approvals run,
    /// denials are terminal, only `Ask` reaches the handler. Caller-supplied
    /// options take precedence over the host default; without either the
    /// gate falls back to ask-everything when a handler is attached.
    pub approval_options: Option<wf_types::tool::approval::ToolApprovalOptions>,
    /// Optional tool-approval handler routed into the loop coordinator.
    /// With a handler registered every tool call the policy routes to a
    /// human is answered by it; policy denials never reach it.
    pub approval_handler: Option<Arc<dyn ToolApprovalHandler>>,
}

impl RunAgentLoopParams {
    /// Parameters with generated id and no approval wiring.
    pub fn new(config: AgentLoopConfig, input: AgentLoopInput) -> Self {
        Self {
            config,
            input,
            agent_loop_id: None,
            approval_options: None,
            approval_handler: None,
        }
    }
}

impl std::fmt::Debug for RunAgentLoopParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunAgentLoopParams")
            .field("config", &self.config)
            .field("input", &self.input)
            .field("agent_loop_id", &self.agent_loop_id)
            .field("approval_options", &self.approval_options.is_some())
            .field("approval_handler", &self.approval_handler.is_some())
            .finish()
    }
}

mod accessor;
mod checkpoint;
mod lifecycle;
#[cfg(test)]
mod tests;

pub use accessor::AgentStateAccessor;
pub use checkpoint::{auto_resume, resume_from_checkpoint};
pub use lifecycle::{append, cancel, convert_pending_to_steering, pause, resume, run, set_append_mode, status, stream};

// helpers re-exported as pub(crate) maybe
pub fn registry(ctx: &ApiContext) -> Arc<AgentLoopRegistry> {
    ctx.agent_loops.clone()
}

pub(crate) fn coordinator(ctx: &ApiContext) -> AgentLoopCoordinator {
    let mut coordinator = AgentLoopCoordinator::with_store(
        ctx.llm_gateway.clone(),
        ctx.tool_registry.clone(),
        ctx.checkpoint_store.clone(),
    )
    .with_event_bus(ctx.event_bus.clone())
    .with_entity_registry(ctx.agent_loops.clone())
    .with_state_manager(ctx.state_manager.clone());
    if let Some(ref metrics) = ctx.metrics {
        coordinator = coordinator.with_metrics(metrics.clone());
    }
    if let Some(ref registry) = ctx.hook_handler_registry {
        coordinator = coordinator.with_hook_handler_registry(registry.clone());
    }
    if let Some(manager) = ctx.file_checkpoint_manager() {
        coordinator = coordinator.with_file_checkpoint_manager(manager.clone());
    }
    coordinator
}

pub(crate) fn coordinator_for(
    ctx: &ApiContext,
    params: &RunAgentLoopParams,
    execution_id: Option<&str>,
) -> AgentLoopCoordinator {
    let mut coordinator = coordinator(ctx);
    if let Some(options) = params.approval_options.clone() {
        coordinator = coordinator.with_approval_options(options);
    }
    if let Some(handler) = params.approval_handler.clone() {
        coordinator = coordinator.with_approval_handler(handler);
    }
    if params.approval_options.is_none() || params.approval_handler.is_none() {
        if let Some(execution_id) = execution_id {
            if let Some(wiring) =
                crate::workflow::tool_approval_handler::host_tool_approval(ctx, execution_id)
            {
                if params.approval_options.is_none() {
                    coordinator = coordinator.with_approval_options(wiring.options);
                }
                if params.approval_handler.is_none() {
                    coordinator = coordinator.with_approval_handler(wiring.handler);
                }
            }
        }
    }
    coordinator
}

pub(crate) fn live_entity(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> crate::infra::error::ApiResult<Arc<AgentLoopEntity>> {
    ctx.agent_loop(agent_loop_id)
        .ok_or_else(|| ApiError::execution_not_found(agent_loop_id))
}

pub(crate) fn gate_agent_config(
    ctx: &ApiContext,
    config: &AgentLoopConfig,
) -> crate::ApiResult<()> {
    match wf_agent::validation::AgentLoopValidator::validate_or_fail(config, &ctx.tool_registry) {
        Ok(_) => Ok(()),
        Err(errors) => {
            let detail = errors
                .iter()
                .map(|e| format!("{}: {}", e.field, e.message))
                .collect::<Vec<_>>()
                .join("; ");
            Err(ApiError::Validation(format!(
                "agent config validation failed ({} error(s)): {}",
                errors.len(),
                detail
            )))
        }
    }
}

pub(crate) fn agent_timeout_ms(config: &AgentLoopConfig) -> u64 {
    if let Some(max_execution_time) = config.max_execution_time {
        return max_execution_time;
    }
    config
        .max_iterations
        .map(|iterations| iterations as u64 * 30_000)
        .unwrap_or(90_000)
        .max(1)
}
