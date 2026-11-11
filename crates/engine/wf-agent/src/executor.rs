mod callback_adapter;
mod control;
mod gates;
mod reclaim;
mod spawn;
mod sync_run;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use wf_execution_shared::hooks::HookHandlerRegistry;
use wf_llm::LlmGateway;
use wf_tools::callback::ParentLink;
use wf_tools::registry::ToolRegistry;
use wf_types::execution::MAX_EXECUTION_DEPTH;

use crate::constants::{AGENT_MAX_ITERATIONS_CAP, DEFAULT_MAX_ITERATIONS};
use crate::coordinator::lifecycle::AgentLoopCoordinator;
use crate::registry::AgentLoopRegistry;

/// Agent loop engine entry point: runs agent loops (sync), dispatches them
/// asynchronously (`spawn_agent_loop`) and serves status queries and
/// cancellation over the shared execution registry.
pub struct AgentLoopExecutor {
    gateway: std::sync::Arc<LlmGateway>,
    registry: std::sync::Arc<ToolRegistry>,
    agent_registry: std::sync::Arc<AgentLoopRegistry>,
    max_iterations: u32,
    max_iterations_cap: u32,
    max_sub_agent_depth: u32,
    event_bus: Option<Arc<wf_core::EventBus>>,
    /// Typed signal bus: control signals (stop/pause/resume) targeting
    /// loops started here are delivered to their coordinators.
    signal_bus: Option<Arc<wf_core::internal_signal::InternalSignalBus>>,
    hook_handler_registry: Option<Arc<HookHandlerRegistry>>,
}

impl AgentLoopExecutor {
    pub fn new(
        gateway: std::sync::Arc<LlmGateway>,
        registry: std::sync::Arc<ToolRegistry>,
    ) -> Self {
        Self {
            gateway,
            registry,
            agent_registry: std::sync::Arc::new(AgentLoopRegistry::new()),
            max_iterations: DEFAULT_MAX_ITERATIONS,
            max_iterations_cap: AGENT_MAX_ITERATIONS_CAP,
            max_sub_agent_depth: MAX_EXECUTION_DEPTH,
            event_bus: None,
            signal_bus: None,
            hook_handler_registry: None,
        }
    }

    pub fn with_max_iterations(mut self, max: u32) -> Self {
        self.max_iterations = max;
        self
    }

    /// Hard cap on `max_iterations`; configs above it are rejected at
    /// execution time. Defaults to `AGENT_MAX_ITERATIONS_CAP`.
    pub fn with_max_iterations_cap(mut self, cap: u32) -> Self {
        self.max_iterations_cap = cap;
        self
    }

    /// Maximum sub-agent recursion depth (root = depth 0). A nested spawn
    /// whose resolved depth would exceed the limit is rejected with
    /// `AgentError::HierarchyLimitReached`. This policy may only tighten the
    /// structural limit, never exceed it.
    pub fn with_max_sub_agent_depth(mut self, max: u32) -> Self {
        self.max_sub_agent_depth = max;
        self.agent_registry.set_max_sub_agent_depth(max);
        self
    }

    /// Concurrent-execution limit enforced by the registry's capacity gate.
    pub fn with_max_concurrent(self, max: usize) -> Self {
        self.agent_registry.set_max_concurrent(max);
        self
    }

    /// Inject a shared registry so external consumers (wf-api execution
    /// views, wf-runtime composite callback) observe the same executions as
    /// this executor. Without injection a private instance is used. The
    /// executor's capacity/depth limits are applied to the injected registry.
    pub fn with_shared_registry(
        mut self,
        agent_registry: std::sync::Arc<AgentLoopRegistry>,
    ) -> Self {
        agent_registry.set_max_sub_agent_depth(self.max_sub_agent_depth);
        self.agent_registry = agent_registry;
        self
    }

    /// The registry this executor registers its executions into.
    pub fn agent_registry(&self) -> &std::sync::Arc<AgentLoopRegistry> {
        &self.agent_registry
    }

    /// Inject the shared event bus; loops started here publish their hook
    /// executions as `HOOK_TRIGGERED` events on it.
    pub fn with_event_bus(mut self, event_bus: Arc<wf_core::EventBus>) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    /// Inject the typed signal bus: control signals (stop/pause/resume)
    /// targeting loops started here are delivered to their coordinators.
    pub fn with_signal_bus(
        mut self,
        bus: Arc<wf_core::internal_signal::InternalSignalBus>,
    ) -> Self {
        self.signal_bus = Some(bus);
        self
    }

    /// Inject the shared hook receiver registry; hook points and engine
    /// signals of loops started here dispatch through it.
    pub fn with_hook_handler_registry(mut self, registry: Arc<HookHandlerRegistry>) -> Self {
        self.hook_handler_registry = Some(registry);
        self
    }

    fn coordinator(&self, parent: Option<&ParentLink>) -> AgentLoopCoordinator {
        // The typed link is the sole parent channel: it carries the live
        // parent handle, whatever execution type owns it. A missing link
        // means the child runs as a root execution.
        let mut coordinator =
            AgentLoopCoordinator::new(self.gateway.clone(), self.registry.clone())
                .with_default_max_iterations(self.max_iterations)
                .with_max_iterations_cap(self.max_iterations_cap)
                .with_entity_registry(self.agent_registry.clone())
                .with_parent_execution_id(parent.map(|link| link.execution_id.clone()));
        if let Some(manager) = parent.map(|link| link.manager.clone()) {
            coordinator = coordinator.with_parent_hierarchy_manager(manager);
        }
        if let Some(bus) = &self.event_bus {
            coordinator = coordinator.with_event_bus(bus.clone());
        }
        if let Some(bus) = &self.signal_bus {
            coordinator = coordinator.with_signal_bus(bus.clone());
        }
        if let Some(registry) = &self.hook_handler_registry {
            coordinator = coordinator.with_hook_handler_registry(registry.clone());
        }
        coordinator
    }
}
