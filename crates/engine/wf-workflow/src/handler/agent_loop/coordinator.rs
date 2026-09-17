//! Construction of the `AgentLoopCoordinator` for one run: wires the injected
//! gateway and registry with the node's event bus, approval plumbing, runtime
//! visibility gate, pause budget, and checkpoint strategy.

use std::sync::Arc;

use wf_agent::checkpoint::AgentCheckpointStrategy;
use wf_agent::coordinator::lifecycle::AgentLoopCoordinator;
use wf_agent::VariableBackedVisibilityStore;
use wf_execution_shared::context::NodeExecutionContext;
use wf_llm::LlmGateway;
use wf_tools::registry::ToolRegistry;

pub(crate) fn build_coordinator(
    gateway: Arc<LlmGateway>,
    ctx: &NodeExecutionContext,
    agent_config: Option<&wf_types::agent::AgentConfig>,
) -> AgentLoopCoordinator {
    let tool_registry = ctx
        .tool_registry
        .clone()
        .unwrap_or_else(|| Arc::new(ToolRegistry::new()));
    let mut coordinator = AgentLoopCoordinator::new(gateway, tool_registry);
    if let Some(ref bus) = ctx.event_bus {
        coordinator = coordinator.with_event_bus(bus.clone());
    }
    // Nested agent loops inherit the parent's tool-level approval config
    // (external handler and/or policy options).
    if let Some(options) = ctx.tool_approval_options.clone() {
        coordinator = coordinator.with_approval_options(options);
    }
    if let Some(handler) = ctx.tool_approval_handler.clone() {
        coordinator = coordinator.with_approval_handler(handler);
    }
    // Runtime visibility gate: reads `__tool_blocked_*` workflow markers
    // (written by TOOL_VISIBILITY nodes). Blocks intercept at execution
    // time only; the visible schema is assembled independently.
    let visibility_store = Arc::new(VariableBackedVisibilityStore::new(ctx.variables.clone()));
    coordinator = coordinator.with_visibility_store(visibility_store);
    if let Some(max_pause_duration) = agent_config.and_then(|c| c.max_pause_duration) {
        coordinator = coordinator.with_max_pause_duration(max_pause_duration);
    }
    if let Some(checkpoint) = agent_config.and_then(|c| c.checkpoint.as_ref()) {
        if checkpoint.enabled {
            coordinator =
                coordinator.with_checkpoint_strategy(AgentCheckpointStrategy::from_agent_config(
                    checkpoint.interval_iterations.unwrap_or(1),
                    checkpoint.on_error.unwrap_or(true),
                    checkpoint.on_tool_call.unwrap_or(true),
                    checkpoint.on_compression.unwrap_or(true),
                    checkpoint.message_interval,
                ));
        }
    }
    coordinator
}
