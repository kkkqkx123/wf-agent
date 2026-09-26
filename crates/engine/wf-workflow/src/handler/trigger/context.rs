//! Per-execution context for one synchronous trigger action set.
//!
//! In-graph only: shares the `TriggerAction` type with the event-driven
//! listener but never touches the `EventBus` async dispatch. Message nodes
//! refuse nested agent execution and cold-start actions per the support
//! matrix (no session anchor / no fresh run from inside an execution).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use dashmap::DashMap;
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use wf_core::internal_signal::InternalSignalBus;
use wf_core::EventBus;
use wf_metrics::MetricsRegistry;
use wf_types::node::StaticNodeType;
use wf_types::Id;

use crate::handler::trigger::runner::ScriptRunner;
use crate::handler::NodeHandler;
use crate::registry::ScriptRegistry;
use wf_execution_shared::script_router::ScriptRouter;
use wf_tools::registry::ToolRegistry;

/// Synchronous in-node execution context for one trigger action set.
#[derive(Clone)]
pub struct TriggerContext {
    pub execution_id: Id,
    pub workflow_id: Id,
    /// Graph node whose execution owns this trigger (empty for the
    /// trigger-listener path, which runs outside any node).
    pub node_id: String,
    pub variables: Arc<DashMap<String, Value>>,
    pub event_bus: Option<Arc<EventBus>>,
    /// Typed signal bus for internal workflow/agent signals
    /// (replaces the `__`-prefixed variable protocol).
    pub signal_bus: Option<Arc<InternalSignalBus>>,
    pub handlers: Option<Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>>>,
    pub tool_registry: Option<Arc<ToolRegistry>>,
    pub metrics: Option<Arc<MetricsRegistry>>,
    pub script_runner: Option<Arc<dyn ScriptRunner>>,
    pub script_registry: Option<Arc<ScriptRegistry>>,
    pub script_router: Option<Arc<ScriptRouter>>,
    /// Abort signal of the owning execution; background triggered
    /// sub-workflows race against it so a cancelled parent stops them.
    pub cancellation: Option<CancellationToken>,
    /// Optional session-level cache shared across multiple trigger actions
    /// within the same message node. Allows actions to share intermediate
    /// state without resorting to global variables.
    pub session_cache: Option<Arc<Mutex<HashMap<String, Value>>>>,
    /// The owning execution's resolved node wall-clock budget (milliseconds),
    /// inherited so a triggered sub-workflow shares the parent's single budget
    /// source instead of resetting to the engine fallback.
    pub parent_node_timeout_ms: Option<u64>,
    /// The owning execution's resolved total wall-clock budget (milliseconds);
    /// a triggered sub-workflow falls back to it when no explicit timeout is
    /// declared on the action.
    pub parent_max_execution_time_ms: Option<u64>,
}

impl TriggerContext {
    pub fn new(execution_id: Id, workflow_id: Id) -> Self {
        Self {
            execution_id,
            workflow_id,
            node_id: String::new(),
            variables: Arc::new(DashMap::new()),
            event_bus: None,
            signal_bus: None,
            handlers: None,
            tool_registry: None,
            metrics: None,
            script_runner: None,
            script_registry: None,
            script_router: None,
            cancellation: None,
            session_cache: None,
            parent_node_timeout_ms: None,
            parent_max_execution_time_ms: None,
        }
    }

    pub fn with_variables(mut self, variables: Arc<DashMap<String, Value>>) -> Self {
        self.variables = variables;
        self
    }

    pub fn with_event_bus(mut self, bus: Arc<EventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    /// Inject a typed signal bus for internal signals.
    pub fn with_signal_bus(mut self, bus: Arc<InternalSignalBus>) -> Self {
        self.signal_bus = Some(bus);
        self
    }

    pub fn with_handlers(
        mut self,
        handlers: Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>>,
    ) -> Self {
        self.handlers = Some(handlers);
        self
    }

    pub fn with_tool_registry(mut self, registry: Arc<ToolRegistry>) -> Self {
        self.tool_registry = Some(registry);
        self
    }

    pub fn with_metrics(mut self, metrics: Arc<MetricsRegistry>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    pub fn with_script_runner(mut self, runner: Arc<dyn ScriptRunner>) -> Self {
        self.script_runner = Some(runner);
        self
    }

    pub fn with_script_router(mut self, router: Arc<ScriptRouter>) -> Self {
        self.script_router = Some(router);
        self
    }

    pub fn with_script_registry(mut self, registry: Arc<ScriptRegistry>) -> Self {
        self.script_registry = Some(registry);
        self
    }

    pub fn with_cancellation(mut self, token: CancellationToken) -> Self {
        self.cancellation = Some(token);
        self
    }

    /// Attach a session-level cache shared across multiple trigger actions
    /// within the same message node. The cache is a simple key-value store
    /// scoped to the session; it is not persisted or checkpointed.
    pub fn with_session_cache(mut self, cache: Arc<Mutex<HashMap<String, Value>>>) -> Self {
        self.session_cache = Some(cache);
        self
    }

    /// Carry the owning execution's resolved budgets so a triggered
    /// sub-workflow inherits the parent's single source rather than resetting
    /// to the engine fallback.
    pub fn with_parent_timeouts(
        mut self,
        node_timeout_ms: Option<u64>,
        max_execution_time_ms: Option<u64>,
    ) -> Self {
        self.parent_node_timeout_ms = node_timeout_ms;
        self.parent_max_execution_time_ms = max_execution_time_ms;
        self
    }
}

/// Shared inputs for one trigger script execution (legacy runner or
/// routed transport).
pub(crate) struct ScriptRun<'a> {
    pub script: &'a wf_script::ScriptDefinition,
    pub script_name: &'a str,
    pub language: &'a str,
    pub parameters: Option<&'a Value>,
    pub provided: &'a HashMap<String, Value>,
    pub context_variables: &'a HashMap<String, Value>,
    pub timeout: u64,
}
