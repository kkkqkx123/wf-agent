use std::sync::Arc;

use dashmap::DashMap;
use serde_json::Value;
use wf_common::retry::RetryBudget;
use wf_core::internal_signal::InternalSignalBus;
use wf_core::EventBus;
use wf_llm::token_tracker::TokenUsageTracker;
use wf_metrics::MetricsRegistry;
use wf_tools::registry::ToolRegistry;
use wf_types::workflow_execution::WorkflowExecutionOptions;
use wf_types::Id;

use wf_resource::registry::ResourceRegistries;

use crate::hooks::HookRegistry;

pub struct ExecutorContext {
    pub execution_id: Id,
    pub workflow_id: Id,
    pub event_bus: Option<Arc<EventBus>>,
    pub tool_registry: Arc<ToolRegistry>,
    /// Shared resource registries (templates, fragments, tool descriptions);
    /// injected by the workflow executor when configured. Handlers render
    /// templateable prompt texts through them and fall back to built-in
    /// defaults when absent.
    pub resource_registries: Option<Arc<ResourceRegistries>>,
    pub variables: Arc<DashMap<String, Value>>,
    pub options: WorkflowExecutionOptions,
    pub parent_execution_id: Option<Id>,
    pub metrics: Option<Arc<MetricsRegistry>>,
    /// Execution-scoped token usage tracker shared by LLM nodes.
    pub token_tracker: Option<Arc<tokio::sync::Mutex<TokenUsageTracker>>>,
    /// Global retry budget shared across the execution (fork branches,
    /// node retries). `None` = no budget constraint.
    pub retry_budget: Option<Arc<RetryBudget>>,
    /// Shared hook receiver registry; hook points and engine signals of this
    /// execution dispatch through it.
    pub hook_registry: Option<Arc<HookRegistry>>,
    /// Tool-level approval: external handler consulted before every tool call
    /// (pre-execution side-effect guard). `None` falls back to
    /// `tool_approval_options` (policy engine) and then to auto-approval.
    pub tool_approval_handler: Option<Arc<dyn crate::approval::ToolApprovalHandler>>,
    /// Tool-level approval policy options (auto-approval presets / patterns /
    /// risk rules). Ignored while a `tool_approval_handler` is attached.
    pub tool_approval_options: Option<wf_types::tool::approval::ToolApprovalOptions>,
    /// Names of variables declared `readonly` in the workflow definition.
    /// VARIABLE nodes targeting them are skipped; `None` = no declarations
    /// were injected (no extra protection).
    pub readonly_variables: Option<Arc<std::collections::HashSet<String>>>,
    /// Live registries of the forks launched by this execution, keyed by
    /// fork node id. Shared with branch executions so SYNC nodes read the
    /// source branch's state and JOIN nodes aggregate the final results.
    pub fork_registries: Arc<std::collections::HashMap<String, Arc<crate::fork::ForkRegistry>>>,
    /// Typed signal bus for internal workflow/agent signals
    /// (replaces the `__`-prefixed variable protocol).
    pub signal_bus: Option<Arc<InternalSignalBus>>,
}

impl ExecutorContext {
    pub fn new(
        execution_id: Id,
        workflow_id: Id,
        event_bus: Option<Arc<EventBus>>,
        tool_registry: Arc<ToolRegistry>,
        options: WorkflowExecutionOptions,
    ) -> Self {
        Self {
            execution_id,
            workflow_id,
            event_bus,
            tool_registry,
            resource_registries: None,
            variables: Arc::new(DashMap::new()),
            options,
            parent_execution_id: None,
            metrics: None,
            token_tracker: Some(Arc::new(tokio::sync::Mutex::new(TokenUsageTracker::new(0)))),
            retry_budget: None,
            hook_registry: None,
            tool_approval_handler: None,
            tool_approval_options: None,
            readonly_variables: None,
            fork_registries: Arc::new(std::collections::HashMap::new()),
            signal_bus: None,
        }
    }

    /// Configure the execution-scoped token limit (0 disables limit checks).
    pub fn with_token_limit(self, token_limit: u64) -> Self {
        if let Some(ref tracker) = self.token_tracker {
            match tracker.try_lock() {
                Ok(mut guard) => guard.set_token_limit(token_limit),
                Err(_) => tracing::warn!("token tracker busy when setting limit; ignored"),
            }
        }
        self
    }

    pub fn with_parent_execution(mut self, parent_id: Id) -> Self {
        self.parent_execution_id = Some(parent_id);
        self
    }

    pub fn with_metrics(mut self, metrics: Arc<MetricsRegistry>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Inject the shared resource registries into the execution (template
    /// rendering in handlers; absent executions fall back to built-in
    /// texts).
    pub fn with_resource_registries(mut self, regs: Arc<ResourceRegistries>) -> Self {
        self.resource_registries = Some(regs);
        self
    }

    /// Set the global retry budget for this execution.
    pub fn with_retry_budget(mut self, budget: Arc<RetryBudget>) -> Self {
        self.retry_budget = Some(budget);
        self
    }

    /// Inject the shared hook receiver registry (dispatch target of hook
    /// points and engine signals during this execution).
    pub fn with_hook_registry(mut self, registry: Arc<HookRegistry>) -> Self {
        self.hook_registry = Some(registry);
        self
    }

    /// Configure tool-level approval for this execution: an external handler
    /// consulted before every tool call, and/or the policy options used when
    /// no handler is attached. Mirrors the agent coordinator's
    /// `with_approval`.
    pub fn with_tool_approval(
        mut self,
        options: Option<wf_types::tool::approval::ToolApprovalOptions>,
        handler: Option<Arc<dyn crate::approval::ToolApprovalHandler>>,
    ) -> Self {
        self.tool_approval_options = options;
        self.tool_approval_handler = handler;
        self
    }

    /// Inject the set of variables declared `readonly` in the workflow
    /// definition. VARIABLE nodes targeting them are skipped at runtime.
    pub fn with_readonly_variables(
        mut self,
        names: Arc<std::collections::HashSet<String>>,
    ) -> Self {
        self.readonly_variables = Some(names);
        self
    }

    /// Inject a typed signal bus for internal workflow/agent signals.
    pub fn with_signal_bus(mut self, bus: Arc<InternalSignalBus>) -> Self {
        self.signal_bus = Some(bus);
        self
    }

    pub fn with_fork_registries(
        mut self,
        registries: Arc<std::collections::HashMap<String, Arc<crate::fork::ForkRegistry>>>,
    ) -> Self {
        self.fork_registries = registries;
        self
    }
}

/// How a node's `input` was produced from its incoming edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeInputShape {
    /// No incoming edges; the input fell back to the workflow-level input.
    None,
    /// Exactly one incoming edge; the input is that source node's raw output.
    Single,
    /// Multiple incoming edges; the input is an object merging each edge's
    /// output keyed by source node id (or edge label).
    Merged,
}

pub struct NodeExecutionContext {
    pub execution_id: Id,
    pub node_id: String,
    pub node_type: wf_types::node::StaticNodeType,
    pub node_name: Option<String>,
    pub node_config: Option<Value>,
    pub input: Value,
    /// How `input` was produced from the incoming edges. `Single` means the
    /// input holds one source node's raw output (not wrapped); `Merged` means
    /// it is an object merging multiple incoming edge outputs; `None` means
    /// the input fell back to the workflow-level input. Handlers use this to
    /// tell a bare object value apart from a merged multi-edge object.
    pub input_shape: NodeInputShape,
    pub variables: Arc<DashMap<String, Value>>,
    pub parent_execution_id: Option<Id>,
    pub depth: u32,
    pub event_bus: Option<Arc<EventBus>>,
    /// Handler registry inherited from the parent execution (strongly
    /// typed; `None` only when no registry was wired, which nested
    /// executions surface as a structured error). Nested executions
    /// (subgraphs, fork branches, triggered sub-workflows) resolve the
    /// engine's handlers through it.
    pub handler_registry: Option<Arc<crate::handler::NodeHandlerRegistry>>,
    /// The execution graph the current node belongs to (subgraph executions
    /// carry their own subgraph). Strongly typed since wf-types is a base
    /// dependency of this crate.
    pub graph_structure: Option<Arc<wf_types::workflow_execution::WorkflowGraphStructure>>,
    /// Tool registry inherited from the parent execution; absent when no
    /// registry was set up (branch/subgraph executors then fall back to a
    /// fresh registry).
    pub tool_registry: Option<Arc<ToolRegistry>>,
    /// Shared resource registries inherited from the parent execution;
    /// handlers render templateable prompt texts through them and fall back
    /// to built-in defaults when absent (mirrors [`ExecutorContext`]).
    pub resource_registries: Option<Arc<ResourceRegistries>>,
    /// Shared metrics registry; absent when metrics are disabled.
    pub metrics: Option<Arc<MetricsRegistry>>,
    /// Execution-scoped token usage tracker (LLM nodes record into it).
    pub token_tracker: Option<Arc<tokio::sync::Mutex<TokenUsageTracker>>>,
    /// Abort signal of the owning execution entity; `None` when the context
    /// was built without an entity. Handlers that spawn sub-tasks (fork
    /// branches, triggered sub-executions) race their work against it so a
    /// cancelled parent stops them.
    pub cancellation: Option<tokio_util::sync::CancellationToken>,
    /// Global retry budget inherited from the parent execution.
    pub retry_budget: Option<Arc<RetryBudget>>,
    /// Shared hook receiver registry inherited from the parent execution.
    pub hook_registry: Option<Arc<HookRegistry>>,
    /// Tool-level approval handler inherited from the parent execution
    /// (pre-execution side-effect guard). `None` falls back to
    /// `tool_approval_options` (policy engine) and then to auto-approval.
    pub tool_approval_handler: Option<Arc<dyn crate::approval::ToolApprovalHandler>>,
    /// Tool-level approval policy options inherited from the parent
    /// execution.
    pub tool_approval_options: Option<wf_types::tool::approval::ToolApprovalOptions>,
    /// Variables declared `readonly` in the workflow definition (inherited
    /// from the execution). VARIABLE nodes targeting them are skipped.
    pub readonly_variables: Option<Arc<std::collections::HashSet<String>>>,
    /// Fork registries inherited from the parent execution (keyed by fork
    /// node id). SYNC/JOIN nodes locate the fork that launched their source
    /// branch and read/wait on its registry.
    pub fork_registries: Arc<std::collections::HashMap<String, Arc<crate::fork::ForkRegistry>>>,
    /// Typed signal bus for internal workflow/agent signals
    /// (replaces the `__`-prefixed variable protocol).
    pub signal_bus: Option<Arc<InternalSignalBus>>,
    /// Session-level cache shared across the trigger actions of one message
    /// node visit. Not persisted or checkpointed; `None` for non-message
    /// nodes.
    pub session_cache: Option<Arc<std::sync::Mutex<std::collections::HashMap<String, Value>>>>,
}

impl NodeExecutionContext {
    pub fn new(
        execution_id: Id,
        node_id: String,
        node_type: wf_types::node::StaticNodeType,
        input: Value,
        variables: Arc<DashMap<String, Value>>,
    ) -> Self {
        Self {
            execution_id,
            node_id,
            node_type,
            node_name: None,
            node_config: None,
            input,
            input_shape: NodeInputShape::None,
            variables,
            parent_execution_id: None,
            depth: 0,
            event_bus: None,
            handler_registry: None,
            graph_structure: None,
            tool_registry: None,
            resource_registries: None,
            metrics: None,
            token_tracker: None,
            cancellation: None,
            retry_budget: None,
            hook_registry: None,
            tool_approval_handler: None,
            tool_approval_options: None,
            readonly_variables: None,
            fork_registries: Arc::new(std::collections::HashMap::new()),
            signal_bus: None,
            session_cache: None,
        }
    }

    pub fn with_node_name(mut self, name: impl Into<String>) -> Self {
        self.node_name = Some(name.into());
        self
    }

    pub fn with_node_config(mut self, config: Value) -> Self {
        self.node_config = Some(config);
        self
    }

    pub fn with_parent_execution(mut self, parent_id: Id) -> Self {
        self.parent_execution_id = Some(parent_id);
        self
    }

    pub fn with_depth(mut self, depth: u32) -> Self {
        self.depth = depth;
        self
    }

    pub fn with_metrics(mut self, metrics: Arc<MetricsRegistry>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Inject the shared resource registries (template rendering in
    /// handlers; absent contexts fall back to built-in texts).
    pub fn with_resource_registries(mut self, regs: Arc<ResourceRegistries>) -> Self {
        self.resource_registries = Some(regs);
        self
    }

    /// Set the global retry budget for this node execution.
    pub fn with_retry_budget(mut self, budget: Arc<RetryBudget>) -> Self {
        self.retry_budget = Some(budget);
        self
    }

    /// Inject the shared hook receiver registry (dispatch target of hook
    /// points and engine signals during this node execution).
    pub fn with_hook_registry(mut self, registry: Arc<HookRegistry>) -> Self {
        self.hook_registry = Some(registry);
        self
    }

    /// Configure tool-level approval for this node execution (external
    /// handler and/or policy options). Mirrors
    /// [`ExecutorContext::with_tool_approval`].
    pub fn with_tool_approval(
        mut self,
        options: Option<wf_types::tool::approval::ToolApprovalOptions>,
        handler: Option<Arc<dyn crate::approval::ToolApprovalHandler>>,
    ) -> Self {
        self.tool_approval_options = options;
        self.tool_approval_handler = handler;
        self
    }

    /// Inject the workflow-declared readonly variable names for this node
    /// execution (mirrors [`ExecutorContext::with_readonly_variables`]).
    pub fn with_readonly_variables(
        mut self,
        names: Arc<std::collections::HashSet<String>>,
    ) -> Self {
        self.readonly_variables = Some(names);
        self
    }

    /// Inject a typed signal bus for internal workflow/agent signals.
    pub fn with_signal_bus(mut self, bus: Arc<InternalSignalBus>) -> Self {
        self.signal_bus = Some(bus);
        self
    }

    /// Attach a session-level cache shared across the trigger actions of the
    /// message node visit (not persisted or checkpointed).
    pub fn with_session_cache(
        mut self,
        cache: Arc<std::sync::Mutex<std::collections::HashMap<String, Value>>>,
    ) -> Self {
        self.session_cache = Some(cache);
        self
    }

    pub fn set_event_bus(&mut self, event_bus: Arc<EventBus>) {
        self.event_bus = Some(event_bus);
    }

    pub fn get_variable(&self, name: &str) -> Option<Value> {
        self.variables.get(name).map(|v| v.clone())
    }

    /// Write a variable through the protected entry: names with the `__`
    /// internal prefix are rejected with a structured error. Engine-internal
    /// state (loop stacks, message contexts, fork handovers, interaction
    /// markers) must use [`Self::set_internal_variable`] instead.
    pub fn set_variable(
        &self,
        name: impl Into<String>,
        value: Value,
    ) -> crate::error::ExecutionSharedResult<()> {
        let name = name.into();
        if name.starts_with("__") {
            return Err(crate::error::ExecutionSharedError::VariableError(format!(
                "refusing to write internal variable '{}' through the public entry",
                name
            )));
        }
        self.variables.insert(name, value);
        Ok(())
    }

    /// Engine-internal bypass of the [`Self::set_variable`] guard, used for
    /// `__`-prefixed state owned by the engine (loop stacks, message
    /// contexts, fork branch handovers, interaction markers).
    pub fn set_internal_variable(&self, name: impl Into<String>, value: Value) {
        self.variables.insert(name.into(), value);
    }
}

pub struct NodeExecutionResult {
    pub output: Value,
    pub next_node_ids: Vec<String>,
    pub metadata: std::collections::HashMap<String, Value>,
}
impl NodeExecutionResult {
    pub fn simple(output: Value) -> Self {
        Self {
            output,
            next_node_ids: Vec::new(),
            metadata: std::collections::HashMap::new(),
        }
    }

    pub fn with_next_nodes(output: Value, next_node_ids: Vec<String>) -> Self {
        Self {
            output,
            next_node_ids,
            metadata: std::collections::HashMap::new(),
        }
    }
}
