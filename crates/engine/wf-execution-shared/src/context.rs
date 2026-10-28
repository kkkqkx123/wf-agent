use std::sync::Arc;

use crate::token_tracker::TokenUsageTracker;
use dashmap::DashMap;
use serde_json::Value;
use wf_core::internal_signal::InternalSignalBus;
use wf_core::EventBus;
use wf_metrics::MetricsRegistry;
use wf_tools::registry::ToolRegistry;
use wf_types::workflow_execution::WorkflowExecutionOptions;
use wf_types::Id;

use wf_resource::registry::ResourceRegistries;

use crate::hooks::HookHandlerRegistry;

pub struct ExecutorContext {
    pub execution_id: Id,
    pub workflow_id: Id,
    pub event_bus: Option<Arc<EventBus>>,
    pub hierarchy_manager: Option<Arc<wf_core::hierarchy::manager::ExecutionHierarchyManager>>,
    pub tool_registry: Arc<ToolRegistry>,
    /// Shared resource registries (templates, fragments, tool descriptions);
    /// injected by the workflow executor when configured. Handlers render
    /// templateable prompt texts through them and fall back to built-in
    /// defaults when absent.
    pub resource_registries: Option<Arc<ResourceRegistries>>,
    pub variables: Arc<DashMap<String, Value>>,
    pub options: WorkflowExecutionOptions,
    /// Checkpoint wiring inherited from the parent execution. Child
    /// executions build their own integration from it (depth-adjusted
    /// strategy); `None` means checkpoints stay disabled below this point.
    pub checkpoint_scope: Option<crate::checkpoint_scope::CheckpointScope>,
    /// Resolved execution filesystem scope. `None` keeps the legacy global
    /// workspace behavior; `Some` gates tool paths and checkpoint capture.
    pub file_scope: wf_types::config::file_scope::ResolvedExecutionScope,
    pub metrics: Option<Arc<MetricsRegistry>>,
    /// Execution-scoped token usage tracker shared by LLM nodes.
    pub token_tracker: Option<Arc<tokio::sync::Mutex<TokenUsageTracker>>>,
    /// Shared hook handler registry; hook points and engine signals of this
    /// execution fire through it.
    pub hook_handler_registry: Option<Arc<HookHandlerRegistry>>,
    /// Tool-level approval: external handler consulted for the tool calls
    /// the policy engine routes to a human (its `Ask` decisions;
    /// policy denials are final and never reach the handler).
    /// `None` falls back to `tool_approval_options` (policy engine) and
    /// then to auto-approval.
    pub tool_approval_handler: Option<Arc<dyn crate::approval::ToolApprovalHandler>>,
    /// Tool-level approval policy options (auto-approval presets / patterns /
    /// risk rules). Evaluated first on every tool call; only `Ask` decisions
    /// consult the `tool_approval_handler`.
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
            hierarchy_manager: None,
            tool_registry,
            resource_registries: None,
            variables: Arc::new(DashMap::new()),
            options,
            checkpoint_scope: None,
            file_scope: wf_types::config::file_scope::ResolvedExecutionScope::none(),
            metrics: None,
            token_tracker: Some(Arc::new(tokio::sync::Mutex::new(TokenUsageTracker::new(0)))),
            hook_handler_registry: None,
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

    pub fn with_hierarchy_manager(
        mut self,
        manager: Arc<wf_core::hierarchy::manager::ExecutionHierarchyManager>,
    ) -> Self {
        self.hierarchy_manager = Some(manager);
        self
    }

    /// Parent execution id resolved from the hierarchy manager (`None`
    /// for roots and manager-less contexts).
    pub fn parent_id(&self) -> Option<Id> {
        self.hierarchy_manager.as_ref().and_then(|m| m.parent_id())
    }

    /// Root-to-parent execution id chain (oldest first, excluding self),
    /// resolved from the hierarchy manager (empty without one).
    pub fn ancestor_chain(&self) -> Vec<Id> {
        self.hierarchy_manager
            .as_ref()
            .map(|m| m.ancestors())
            .unwrap_or_default()
    }

    /// Nesting depth of the owning execution (0 = root or manager-less).
    pub fn depth(&self) -> u32 {
        self.hierarchy_manager
            .as_ref()
            .map(|m| m.depth())
            .unwrap_or(0)
    }

    /// Root execution id of the hierarchy (`None` without a manager).
    pub fn root_id(&self) -> Option<Id> {
        self.hierarchy_manager
            .as_ref()
            .map(|m| m.root_execution_id())
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

    /// Inject the shared hook handler registry (fire target of hook
    /// points and engine signals during this execution).
    pub fn with_hook_handler_registry(mut self, registry: Arc<HookHandlerRegistry>) -> Self {
        self.hook_handler_registry = Some(registry);
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

    /// Inherit the parent's checkpoint wiring; spawned child executions
    /// build their own depth-adjusted integration from it.
    pub fn with_checkpoint_scope(
        mut self,
        scope: crate::checkpoint_scope::CheckpointScope,
    ) -> Self {
        self.checkpoint_scope = Some(scope);
        self
    }

    /// Inject the resolved execution filesystem scope.
    pub fn with_file_scope(
        mut self,
        scope: Option<wf_types::config::file_scope::ResolvedFileScope>,
    ) -> Self {
        self.file_scope = wf_types::config::file_scope::ResolvedExecutionScope { inner: scope };
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
    pub hierarchy_manager: Option<Arc<wf_core::hierarchy::manager::ExecutionHierarchyManager>>,
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
    /// Checkpoint wiring inherited from the parent execution (see
    /// [`ExecutorContext::checkpoint_scope`]).
    pub checkpoint_scope: Option<crate::checkpoint_scope::CheckpointScope>,
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
    /// Interruption state of the owning execution entity (`None` when the
    /// context was built without an entity, mirroring [`Self::cancellation`]).
    /// A clone of the entity's handle over the same shared state. A handler
    /// that needs the emitting execution to wait for external handling
    /// (e.g. a terminal context-compression failure) pauses it through
    /// this state instead of failing the run.
    pub interruption: Option<wf_core::interruption::InterruptionState>,
    /// Shared hook handler registry inherited from the parent execution.
    pub hook_handler_registry: Option<Arc<HookHandlerRegistry>>,
    /// Tool-level approval handler inherited from the parent execution
    /// (consulted for the policy engine's `Ask` decisions; denials are
    /// final). `None` falls back to `tool_approval_options` (policy engine)
    /// and then to auto-approval.
    pub tool_approval_handler: Option<Arc<dyn crate::approval::ToolApprovalHandler>>,
    /// Tool-level approval policy options inherited from the parent
    /// execution. Evaluated first on every tool call; only `Ask` decisions
    /// consult the handler.
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
    /// The owning execution's resolved node wall-clock budget (milliseconds),
    /// inherited from its `WorkflowExecutionOptions`. `TRIGGER` sub-workflows
    /// reuse it so a child shares the same single source (entry/limits) as
    /// its parent instead of resetting to the engine fallback.
    pub parent_node_timeout_ms: Option<u64>,
    /// The owning execution's resolved total wall-clock budget
    /// (milliseconds). A `TRIGGER` sub-workflow falls back to it when the
    /// action declares no explicit timeout, keeping parent and child on one
    /// budget source.
    pub parent_max_execution_time_ms: Option<u64>,
    /// The owning execution's checkpoint switch, inherited by nested
    /// executions (subgraph, fork branch, triggered sub-workflow) so a
    /// checkpoint-enabled parent yields checkpoint-capable children.
    /// `None` means the parent did not declare a value (treat as enabled).
    pub parent_checkpoints_enabled: Option<bool>,
    /// Resolved execution filesystem scope inherited from the parent
    /// execution. Branch and sub-workflow children derive their own scope
    /// from it; `None` keeps legacy global behavior.
    pub file_scope: wf_types::config::file_scope::ResolvedExecutionScope,
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
            hierarchy_manager: None,
            node_id,
            node_type,
            node_name: None,
            node_config: None,
            input,
            input_shape: NodeInputShape::None,
            variables,
            checkpoint_scope: None,
            event_bus: None,
            handler_registry: None,
            graph_structure: None,
            tool_registry: None,
            resource_registries: None,
            metrics: None,
            token_tracker: None,
            cancellation: None,
            interruption: None,
            hook_handler_registry: None,
            tool_approval_handler: None,
            tool_approval_options: None,
            readonly_variables: None,
            fork_registries: Arc::new(std::collections::HashMap::new()),
            signal_bus: None,
            session_cache: None,
            parent_node_timeout_ms: None,
            parent_max_execution_time_ms: None,
            parent_checkpoints_enabled: None,
            file_scope: wf_types::config::file_scope::ResolvedExecutionScope::none(),
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

    pub fn with_hierarchy_manager(
        mut self,
        manager: Arc<wf_core::hierarchy::manager::ExecutionHierarchyManager>,
    ) -> Self {
        self.hierarchy_manager = Some(manager);
        self
    }

    /// Parent execution id resolved from the hierarchy manager (`None`
    /// for roots and manager-less contexts).
    pub fn parent_id(&self) -> Option<Id> {
        self.hierarchy_manager.as_ref().and_then(|m| m.parent_id())
    }

    /// Root-to-parent execution id chain (oldest first, excluding self),
    /// resolved from the hierarchy manager (empty without one).
    pub fn ancestor_chain(&self) -> Vec<Id> {
        self.hierarchy_manager
            .as_ref()
            .map(|m| m.ancestors())
            .unwrap_or_default()
    }

    /// Nesting depth of the owning execution (0 = root or manager-less).
    pub fn depth(&self) -> u32 {
        self.hierarchy_manager
            .as_ref()
            .map(|m| m.depth())
            .unwrap_or(0)
    }

    /// Root execution id of the hierarchy (`None` without a manager).
    pub fn root_id(&self) -> Option<Id> {
        self.hierarchy_manager
            .as_ref()
            .map(|m| m.root_execution_id())
    }

    /// Inherit the parent's checkpoint wiring; spawned child executions
    /// build their own depth-adjusted integration from it.
    pub fn with_checkpoint_scope(
        mut self,
        scope: crate::checkpoint_scope::CheckpointScope,
    ) -> Self {
        self.checkpoint_scope = Some(scope);
        self
    }

    /// Inject the owning execution's resolved budgets (node timeout and total
    /// wall-clock, milliseconds) so spawned `TRIGGER` sub-workflows inherit
    /// the same source instead of the engine fallback.
    pub fn with_parent_timeouts(
        mut self,
        node_timeout_ms: Option<u64>,
        max_execution_time_ms: Option<u64>,
    ) -> Self {
        self.parent_node_timeout_ms = node_timeout_ms;
        self.parent_max_execution_time_ms = max_execution_time_ms;
        self
    }

    pub fn with_parent_checkpoints(mut self, enabled: Option<bool>) -> Self {
        self.parent_checkpoints_enabled = enabled;
        self
    }

    /// Inject the resolved execution filesystem scope.
    pub fn with_file_scope(
        mut self,
        scope: Option<wf_types::config::file_scope::ResolvedFileScope>,
    ) -> Self {
        self.file_scope = wf_types::config::file_scope::ResolvedExecutionScope { inner: scope };
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

    /// Inject the shared hook handler registry (fire target of hook
    /// points and engine signals during this node execution).
    pub fn with_hook_handler_registry(mut self, registry: Arc<HookHandlerRegistry>) -> Self {
        self.hook_handler_registry = Some(registry);
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
        if is_internal_variable(&name) {
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

/// Canonical node metadata keys and conversion entry points.
///
/// Handlers reference these constants instead of hand-writing strings so a
/// typo becomes a compile error. Storage keeps the `Value::Object` shape and
/// transport keeps the map shape; conversion goes through the helpers below.
pub mod node_metadata {
    pub const AGENT_LOOP_ID: &str = "agent_loop_id";
    pub const ITERATION_COUNT: &str = "iteration_count";
    pub const MESSAGE_COUNT: &str = "message_count";
    pub const NODE_COUNT: &str = "node_count";
    pub const CHILD_ERROR_IGNORED: &str = "child_error_ignored";
    pub const CHILD_ERROR: &str = "child_error";
    pub const BRANCH_COUNT: &str = "branch_count";
    pub const SUCCESS_COUNT: &str = "success_count";
    pub const OUTCOME: &str = "outcome";
    pub const FAILED_BRANCHES: &str = "failed_branches";
    pub const MODEL: &str = "model";
    pub const FINISH_REASON: &str = "finish_reason";
    pub const PROMPT_TOKENS: &str = "prompt_tokens";
    pub const COMPLETION_TOKENS: &str = "completion_tokens";
    pub const TOOL_CALLS: &str = "tool_calls";
    pub const TOOL_CALL_COUNT: &str = "tool_call_count";
    pub const STREAM: &str = "stream";
    pub const ROUNDS_EXHAUSTED: &str = "rounds_exhausted";
    pub const EVENT: &str = "event";
    pub const ACTION: &str = "action";
    pub const TOOLS: &str = "tools";
    pub const OPERATION_TYPE: &str = "operation_type";
    pub const RESPONDED: &str = "responded";
    pub const APPROVED: &str = "approved";
    pub const INTERACTION_ID: &str = "interaction_id";
    pub const OPERATION: &str = "operation";
    pub const SOURCE_PATH_ID: &str = "source_path_id";
    pub const SYNCED_VARIABLES: &str = "synced_variables";
    pub const LOOP_ID: &str = "loop_id";
    pub const ITERATION: &str = "iteration";
    pub const SHOULD_CONTINUE: &str = "should_continue";
    pub const SCRIPT_NAME: &str = "script_name";
    pub const EXECUTION_TIME: &str = "execution_time";
    pub const LANGUAGE: &str = "language";
    pub const EXECUTOR_MODE: &str = "executor_mode";
    pub const SANDBOX_MODE: &str = "sandbox_mode";
    pub const STRATEGY: &str = "strategy";
    pub const FLOW: &str = "flow";
    pub const INTERACTION_MODE: &str = "interaction_mode";
    pub const COMPLETED_ROUNDS: &str = "completed_rounds";
    pub const HAD_INPUT: &str = "had_input";
    pub const OUTPUT_TRUNCATED: &str = "output_truncated";
    pub const SESSION_ID: &str = "session_id";
    pub const VARIABLE_NAME: &str = "variable_name";
    pub const SKIPPED: &str = "skipped";
    pub const OLD_VALUE: &str = "old_value";
    pub const NEW_VALUE: &str = "new_value";

    /// Nested field carrying handler metadata inside a `NodeCompleted` payload.
    pub const NODE_METADATA_FIELD: &str = "node_metadata";
    /// `NodeCompleted` payload keys owned by the coordinator; handler metadata
    /// never overwrites them.
    pub const RESERVED_EVENT_KEYS: &[&str] = &[
        "has_next_nodes",
        "node_name",
        "node_type",
        "duration_ms",
        "node_id",
        "rejection_source",
        NODE_METADATA_FIELD,
    ];
    /// Internal variable holding the node id -> agent loop id addressing map.
    pub const AGENT_LOOP_IDS_VAR: &str = "__agent_loop_ids";
}

/// Prefix marking engine-owned variables. User writes through the public
/// entry are rejected for these names; engine state uses the internal entry.
pub const INTERNAL_VARIABLE_PREFIX: &str = "__";

/// Single predicate for engine-owned variable names so every guard and view
/// agrees on the boundary.
pub fn is_internal_variable(name: &str) -> bool {
    name.starts_with(INTERNAL_VARIABLE_PREFIX)
}

/// Single writer for the node id -> agent loop id addressing map so the
/// handler and the `agent_loops` query path never diverge on the key.
pub fn record_agent_loop_binding(
    variables: &Arc<DashMap<String, Value>>,
    node_id: &str,
    agent_loop_id: &str,
) {
    let mut mapping = variables
        .get(node_metadata::AGENT_LOOP_IDS_VAR)
        .and_then(|entry| entry.value().as_object().cloned())
        .unwrap_or_default();
    mapping.insert(
        node_id.to_string(),
        Value::String(agent_loop_id.to_string()),
    );
    variables.insert(
        node_metadata::AGENT_LOOP_IDS_VAR.to_string(),
        Value::Object(mapping),
    );
}

/// Handler metadata map converted to the storage object shape.
pub fn metadata_map_to_value(map: &std::collections::HashMap<String, Value>) -> Value {
    Value::Object(map.clone().into_iter().collect())
}

/// Storage object converted back to the builder map shape.
pub fn metadata_value_to_map(value: &Value) -> std::collections::HashMap<String, Value> {
    value
        .as_object()
        .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default()
}

/// One metadata value capped to the checkpoint payload budget so a single
/// large array never dominates an event or a state record.
pub fn cap_metadata_value(value: &Value) -> Value {
    wf_types::checkpoint::workflow::cap_node_payload(value)
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

    /// Insert one handler metadata entry through the canonical key set.
    pub fn insert_metadata(&mut self, key: &str, value: Value) {
        self.metadata.insert(key.to_string(), value);
    }

    /// Storage object shape of this result's metadata.
    pub fn metadata_object(&self) -> Value {
        metadata_map_to_value(&self.metadata)
    }
}
