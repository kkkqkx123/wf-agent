use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::panic::AssertUnwindSafe;
use std::sync::Arc;

use futures::FutureExt;
use serde_json::Value;
use wf_common::now;
use wf_core::condition::ConditionEvaluator;
use wf_core::internal_signal::{InternalSignal, InternalSignalReceiver};
use wf_core::interruption::InterruptionSignal;
use wf_core::EventBus;
use wf_execution_shared::context::{
    ExecutorContext, NodeExecutionContext, NodeExecutionResult, NodeInputShape,
};
use wf_execution_shared::execution_state::ExecutionStateManager;
use wf_execution_shared::fork::ForkRegistry;
use wf_execution_shared::hooks::types::HookDefinition;
use wf_execution_shared::interruption::check_execution_interruption;
use wf_execution_shared::types::execution_entity::ExecutionEntity;
use wf_execution_shared::types::state_manager::StateManager;
use wf_metrics::collectors::node::NodeExecutionRecord as MetricsNodeExecutionRecord;
use wf_metrics::collectors::node::NodeMetricsCollector;
use wf_types::checkpoint::NodeCheckpointConfig;
use wf_types::events::{BaseEvent, EventType};
use wf_types::node::StaticNodeType;
use wf_types::workflow::error_branch::{is_merge_point, ErrorSuspendState};
use wf_types::workflow_execution::WorkflowGraphStructure;

use crate::checkpoint::WorkflowCheckpointIntegration;
use crate::coordinator::NodeCoordinator;
use crate::entity::WorkflowExecutionEntity;
use crate::error::{WorkflowError, WorkflowResult};
use crate::error_branch::{
    check_branch_budget, classify_error, ErrorBranchScope, ErrorFailureAction,
};

/// Engine-wide fallback node timeout in milliseconds. Applied when neither
/// the node-level `timeout_seconds` nor the global options default is set,
/// so no ordinary node runs unbounded. Long-running nodes (nested
/// executions: agent loops, sub-graphs, interactive sessions) are exempt
/// from the fallback because they carry their own budgets; see
/// `StaticNodeType::is_long_running`.
pub const DEFAULT_NODE_TIMEOUT_MS: u64 = 30_000;

/// Human-readable message from a `catch_unwind` panic payload.
fn panic_message(payload: &(dyn Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

/// Resolve the wall-clock budget wrapping one node execution.
/// Priority: node-level `timeout_seconds` (seconds) > global options
/// default (milliseconds) > engine-wide fallback. The fallback is skipped
/// for long-running node types, which are bounded by their inner budgets.
fn resolve_node_timeout(
    node: &wf_types::workflow_execution::WorkflowNode,
    node_type: &StaticNodeType,
    options_default_ms: Option<u64>,
) -> Option<std::time::Duration> {
    let ms = node
        .inner
        .get("timeout_seconds")
        .and_then(|v| v.as_u64())
        .map(|secs| secs.saturating_mul(1000))
        .or(options_default_ms)
        .or(if node_type.is_long_running() {
            None
        } else {
            Some(DEFAULT_NODE_TIMEOUT_MS)
        });
    ms.map(std::time::Duration::from_millis)
}
use crate::error_analysis::workflow_error_record;
use crate::graph::GraphTraversal;
use crate::handler::NodeHandler;
use crate::hook::WorkflowHookEmitter;
use crate::persistence::build_workflow_execution;
use crate::state::{NodeExecutionRecord, WorkflowExecutionStateSnapshot};

/// Serialized size of a value in bytes, used for node input/output metrics.
fn json_size(value: &Value) -> u64 {
    serde_json::to_string(value)
        .map(|s| s.len() as u64)
        .unwrap_or(0)
}

/// Identity of the node being executed in the coordinator loop: the execution
/// entity it runs under plus the node's id and parsed type. Grouped so the
/// execute / record / retry helpers stay small.
struct NodeAttempt<'a> {
    entity: &'a WorkflowExecutionEntity,
    node_id: &'a str,
    node_type: &'a StaticNodeType,
}

/// Timing and accounting inputs captured once a node execution has a result.
/// Shared by the success/failure recording and retry paths.
struct NodeOutcome<'a> {
    node_type_str: &'a str,
    metrics: Option<&'a NodeMetricsCollector>,
    start: i64,
    duration_ms: f64,
    checkpoint_config: Option<NodeCheckpointConfig>,
    /// Node-level `checkpoint_after_execute` force flag, carried from the
    /// pre-execution read so the completion path needs no second lookup.
    force_checkpoint_after: bool,
}

/// Parse the node-level checkpoint configuration embedded in the node config
/// under the `checkpoint` key. A malformed config is a user error: it fails
/// with a structured `ConfigError` instead of silently falling back to the
/// workflow-level policy.
fn node_checkpoint_config(
    node_id: &str,
    node_inner: &Value,
) -> WorkflowResult<Option<NodeCheckpointConfig>> {
    match node_inner.get("checkpoint") {
        None => Ok(None),
        Some(v) => crate::config_parse::parse_node_config(node_id, "inner.checkpoint", v).map(Some),
    }
}

/// Read a node-level force-checkpoint flag from the node config blob
/// (`checkpoint_before_execute` / `checkpoint_after_execute`, snake case as
/// serialized from `NodeExecutionConfig`). Absent or non-boolean means no
/// force: the strategy decision stands on its own.
fn node_force_checkpoint(node_inner: &Value, key: &str) -> bool {
    node_inner
        .get(key)
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn parse_node_type(node_type_str: &str) -> WorkflowResult<StaticNodeType> {
    match node_type_str {
        "START" => Ok(StaticNodeType::Start),
        "END" => Ok(StaticNodeType::End),
        "EMBED_START" => Ok(StaticNodeType::EmbedStart),
        "EMBED_END" => Ok(StaticNodeType::EmbedEnd),
        "VARIABLE" => Ok(StaticNodeType::Variable),
        "FORK" => Ok(StaticNodeType::Fork),
        "JOIN" => Ok(StaticNodeType::Join),
        "SYNC" => Ok(StaticNodeType::Sync),
        "SUBGRAPH" => Ok(StaticNodeType::Subgraph),
        "EMBED_GRAPH" => Ok(StaticNodeType::EmbedGraph),
        "SCRIPT" => Ok(StaticNodeType::Script),
        "INTERACTIVE_SCRIPT" => Ok(StaticNodeType::InteractiveScript),
        "LLM" => Ok(StaticNodeType::Llm),
        "TOOL_VISIBILITY" => Ok(StaticNodeType::ToolVisibility),
        "USER_INTERACTION" => Ok(StaticNodeType::UserInteraction),
        "ROUTE" => Ok(StaticNodeType::Route),
        "CONTEXT_PROCESSOR" => Ok(StaticNodeType::ContextProcessor),
        "LOOP_START" => Ok(StaticNodeType::LoopStart),
        "LOOP_END" => Ok(StaticNodeType::LoopEnd),
        "AGENT_LOOP" => Ok(StaticNodeType::AgentLoop),
        "START_FROM_MESSAGE" => Ok(StaticNodeType::StartFromMessage),
        "CONTINUE_FROM_MESSAGE" => Ok(StaticNodeType::ContinueFromMessage),
        // Unknown types are kept as plugin-contributed node types; handler
        // resolution falls back to the plugin source for them.
        other => Ok(StaticNodeType::Custom(other.to_string())),
    }
}

/// One completed node execution attempt, shared by the main execution path
/// and the retry path; each attempt yields an independent record.
struct ExecutionAttempt<'a> {
    node_id: &'a str,
    node_type: &'a str,
    start_time: i64,
    success: bool,
    error: Option<String>,
    /// Input passed to the node handler (audit detail).
    input: Option<Value>,
    /// Result produced by the node (audit detail).
    result: Option<Value>,
    /// Fork/join branch the node ran under (audit detail).
    branch_id: Option<String>,
}

pub struct WorkflowCoordinator {
    ctx: ExecutorContext,
    entity: Option<Arc<WorkflowExecutionEntity>>,
    traversal: GraphTraversal,
    handlers: Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>>,
    current_node_id: Option<String>,
    completed_nodes: Vec<String>,
    node_outputs: HashMap<String, Value>,
    node_errors: Vec<String>,
    start_time: i64,
    hooks: Vec<HookDefinition>,
    /// Navigation counter for detecting non-loop dead cycles (e.g., cycles in
    /// DAG edges that don't pass through LOOP_START/LOOP_END). Reset when
    /// entering a loop scope; only counts nodes outside loop bodies.
    navigation_count: u32,
    /// Navigation counter for detecting loops that execute too many iterations.
    /// Tracks the number of nodes executed within the current loop body.
    loop_navigation_count: u32,
    /// Whether the coordinator is currently inside a loop body.
    in_loop_body: bool,
    total_node_count: u32,
    max_navigation_multiplier: u32,
    checkpoint: Option<WorkflowCheckpointIntegration>,
    /// Optional write point for the persisted `WorkflowExecution` record;
    /// wired by the application (via `wf-api`) so the coordinator persists
    /// the record at execution start and on every terminal exit.
    state_manager: Option<ExecutionStateManager>,
    /// Optional live-variable sink for a fork branch execution: after every
    /// completed node the coordinator publishes the branch's public
    /// variables into the fork registry so SYNC nodes can read the source
    /// branch's intermediate state.
    fork_branch_progress: Option<(Arc<ForkRegistry>, String)>,
    /// Plugin-contributed node handlers consulted when the builtin map has
    /// no handler for a node type (resolution chain: builtin → plugin).
    /// Built once per execution by the application layer from the plugin
    /// contribution source.
    plugin_handlers: Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>>,
    /// Receiver for typed internal signals (replaces the `__`-prefixed
    /// variable protocol).
    signal_receiver: Option<InternalSignalReceiver>,
    /// Nodes requested for skipping by `InternalSignal::SkipNode`, applied
    /// at dispatch time (one-shot per node).
    skipped_nodes: HashSet<String>,
    /// Active error-branch variable scope (`None` on the main path). While
    /// set, the branch runs on an overlay cloned from the entry snapshot;
    /// the main-path map stays frozen until an explicit JOIN merge.
    error_scope: Option<ErrorBranchScope>,
}

impl WorkflowCoordinator {
    /// Build a coordinator over a workflow graph. The graph is preprocessed
    /// first: EMBED_GRAPH nodes are expanded in place (START -> EMBED_START,
    /// END -> EMBED_END), so runtime executes the flattened structure — the
    /// single execution graph. Preprocessing failures (validation errors,
    /// EMBED_GRAPH constraint violations) reject the workflow up front.
    pub fn new(
        ctx: ExecutorContext,
        graph: WorkflowGraphStructure,
        handlers: Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>>,
    ) -> WorkflowResult<Self> {
        let pre = crate::preprocess::preprocess_graph(graph);
        for warning in &pre.warnings {
            tracing::warn!(%warning, "workflow graph consistency warning");
        }
        if !pre.is_valid() {
            let detail = pre
                .validation_errors
                .iter()
                .map(|e| format!("{}: {}", e.field, e.message))
                .collect::<Vec<_>>()
                .join("; ");
            return Err(WorkflowError::GraphError(format!(
                "Workflow preprocessing failed ({} error(s)): {}",
                pre.validation_errors.len(),
                detail
            )));
        }
        Self::new_preprocessed(ctx, pre.graph, handlers)
    }

    /// Build a coordinator over an already-preprocessed (flattened)
    /// execution graph. A fork registry is pre-created for every FORK node
    /// in the graph so fork handlers, branch executions and SYNC/JOIN nodes
    /// share the same live state. ResourceRegistries already inherited from a parent
    /// execution (a fork branch) are kept as-is — the parent pre-created them
    /// for the whole graph, including nested forks.
    fn new_preprocessed(
        ctx: ExecutorContext,
        graph: WorkflowGraphStructure,
        handlers: Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>>,
    ) -> WorkflowResult<Self> {
        let mut fork_registries = ctx.fork_registries.as_ref().clone();
        for node in &graph.nodes {
            if node.node_type == "FORK" {
                fork_registries
                    .entry(node.id.clone())
                    .or_insert_with(|| Arc::new(ForkRegistry::new()));
            }
        }
        let ctx = ctx.with_fork_registries(Arc::new(fork_registries));

        let traversal = GraphTraversal::new(graph)?;
        let start_node_id = traversal
            .start_node_id()
            .ok_or_else(|| WorkflowError::GraphError("Start node not found".to_string()))?
            .to_string();

        let total_node_count = traversal.node_count() as u32;
        let max_navigation_multiplier = ctx.options.max_navigation_multiplier.unwrap_or(5);

        // Publish the runtime-injected loop iteration cap through the shared
        // variable table so LOOP_START handlers resolve it per execution.
        // Absent, handlers fall back to the engine's built-in constant.
        if let Some(cap) = ctx.options.loop_max_iterations_cap {
            ctx.variables.insert(
                crate::loop_state::LOOP_MAX_ITERATIONS_CAP_KEY.to_string(),
                Value::from(cap),
            );
        }

        let signal_receiver = ctx.signal_bus.as_ref().map(|bus| bus.subscribe());

        if let Some(max_execution_time) = ctx.options.max_execution_time {
            if max_execution_time > 0 {
                if let Some(ref metrics) = ctx.metrics {
                    metrics.timeout().record_registration(
                        "workflow_wall_clock",
                        max_execution_time as f64,
                        &ctx.execution_id.to_string(),
                    );
                }
            }
        }

        Ok(Self {
            ctx,
            entity: None,
            traversal,
            handlers,
            current_node_id: Some(start_node_id),
            completed_nodes: Vec::new(),
            node_outputs: HashMap::new(),
            node_errors: Vec::new(),
            start_time: now(),
            hooks: Vec::new(),
            navigation_count: 0,
            loop_navigation_count: 0,
            in_loop_body: false,
            total_node_count,
            max_navigation_multiplier,
            checkpoint: None,
            state_manager: None,
            fork_branch_progress: None,
            plugin_handlers: Arc::new(HashMap::new()),
            signal_receiver,
            skipped_nodes: HashSet::new(),
            error_scope: None,
        })
    }

    /// Attach plugin-contributed node handlers used as fallback when the
    /// builtin handler map has no entry for a node type.
    pub fn with_plugin_handlers(
        mut self,
        handlers: Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>>,
    ) -> Self {
        self.plugin_handlers = handlers;
        self
    }

    /// Resolve the handler for a node type: builtin map first, then the
    /// plugin-contributed fallback map (builtin → plugin resolution chain).
    fn resolve_node_handler(&self, node_type: &StaticNodeType) -> Option<&dyn NodeHandler> {
        self.handlers
            .get(node_type)
            .map(|h| h.as_ref())
            .or_else(|| self.plugin_handlers.get(node_type).map(|h| h.as_ref()))
    }

    pub fn with_entity(mut self, entity: WorkflowExecutionEntity) -> Self {
        let entity = Arc::new(entity);
        self.completed_nodes = {
            if let Ok(state) = entity.state.try_read() {
                state.completed_nodes().to_vec()
            } else {
                Vec::new()
            }
        };
        self.entity = Some(entity);
        self
    }

    /// Like [`WorkflowCoordinator::with_entity`], but accepts a shared
    /// `Arc` so callers can keep a handle to pause/resume/cancel the
    /// execution through the same entity the coordinator drives.
    pub fn with_entity_arc(mut self, entity: Arc<WorkflowExecutionEntity>) -> Self {
        self.completed_nodes = {
            if let Ok(state) = entity.state.try_read() {
                state.completed_nodes().to_vec()
            } else {
                Vec::new()
            }
        };
        self.entity = Some(entity);
        self
    }

    pub fn with_hooks(mut self, hooks: Vec<HookDefinition>) -> Self {
        self.hooks = hooks;
        self
    }

    pub fn with_checkpoint(mut self, checkpoint: WorkflowCheckpointIntegration) -> Self {
        self.checkpoint = Some(checkpoint);
        self
    }

    /// Wire the execution state manager used to persist the
    /// `WorkflowExecution` record. Without it the execution is driven fully in
    /// memory and nothing is written to the execution store.
    pub fn with_state_manager(mut self, state_manager: ExecutionStateManager) -> Self {
        self.state_manager = Some(state_manager);
        self
    }

    /// Publish this execution's public variables into the fork registry
    /// after every completed node. Used by fork branch executions so SYNC
    /// nodes can read the source branch's intermediate state.
    pub fn with_fork_branch_progress(
        mut self,
        registry: Arc<ForkRegistry>,
        path_id: String,
    ) -> Self {
        self.fork_branch_progress = Some((registry, path_id));
        self
    }

    pub fn completed_nodes(&self) -> &[String] {
        &self.completed_nodes
    }

    /// Resume from a restored checkpoint snapshot: seed completed node
    /// outputs and restart at the checkpointed node. Completed nodes are
    /// skipped by the main loop; their outputs feed the edges downstream.
    pub fn resume_from(
        &mut self,
        snapshot: &wf_types::checkpoint::workflow::WorkflowExecutionStateSnapshot,
    ) {
        if let Some(node_results) = &snapshot.node_results {
            for (node_id, output) in node_results {
                self.node_outputs.insert(node_id.clone(), output.clone());
                if !self.completed_nodes.contains(node_id) {
                    self.completed_nodes.push(node_id.clone());
                }
            }
        }
        if let Some(node_id) = &snapshot.current_node_id {
            if !self.completed_nodes.contains(node_id) {
                self.current_node_id = Some(node_id.clone());
            }
        }
    }

    /// Snapshot of the owned entity's execution state.
    pub async fn state_snapshot(&self) -> WorkflowResult<WorkflowExecutionStateSnapshot> {
        let entity = self.entity.as_ref().ok_or_else(|| {
            WorkflowError::CoordinatorError("Entity not set on WorkflowCoordinator".to_string())
        })?;
        Ok(entity.state.read().await.create_snapshot().await?)
    }

    /// Record a node completion on the innermost active loop's current
    /// iteration, so the completed-skip decision can tell iterations apart.
    fn record_loop_iteration_completion(&self, node_id: &str) {
        let is_loop_control = self
            .traversal
            .get_node(node_id)
            .is_some_and(|n| matches!(n.node_type.as_str(), "LOOP_START" | "LOOP_END"));
        if !is_loop_control {
            crate::loop_state::record_iteration_completion(&self.ctx.variables, node_id);
        }
    }

    /// Append one node execution record to the shared entity state.
    async fn record_node_execution(
        &self,
        entity: &WorkflowExecutionEntity,
        attempt: ExecutionAttempt<'_>,
    ) {
        let node_name = self
            .traversal
            .get_node(attempt.node_id)
            .and_then(|n| n.name.clone())
            .unwrap_or_else(|| attempt.node_id.to_string());

        entity
            .state
            .write()
            .await
            .record_node_execution(NodeExecutionRecord {
                node_id: attempt.node_id.to_string(),
                node_name,
                node_type: attempt.node_type.to_string(),
                start_time: attempt.start_time,
                end_time: Some(wf_common::now()),
                success: attempt.success,
                error: attempt.error,
                // Pre-capped payload audit detail (truncation footprint
                // marking stays with the checkpoint type).
                input: attempt
                    .input
                    .as_ref()
                    .map(wf_types::checkpoint::workflow::cap_node_payload),
                result: attempt
                    .result
                    .as_ref()
                    .map(wf_types::checkpoint::workflow::cap_node_payload),
                branch_id: attempt.branch_id,
            });
    }

    /// Persist a structured error record for a failed node attempt. Each
    /// record is its own root cause with a single-entry chain.
    async fn record_workflow_error(
        entity: &WorkflowExecutionEntity,
        error: &WorkflowError,
        node_id: &str,
    ) {
        let mut state = entity.state.write().await;
        let mut record = workflow_error_record(error, entity.id(), node_id);
        record.error_chain = vec![record.id.clone()];
        record.root_cause_id = record.id.clone();
        state.add_error_record(record);
    }

    /// Drive the workflow graph to completion.
    ///
    /// Persists the `WorkflowExecution` record through the wired state manager
    /// in two phases: a start record before the nodes run and a final record
    /// after the execution reaches a terminal state (completed / failed /
    /// cancelled / paused). Phase-based writes keep the record consistent with
    /// the live entity state on every exit path.
    pub async fn execute(&mut self) -> WorkflowResult<Value> {
        self.persist_start().await;
        self.execute_workflow_scope_hook("WORKFLOW_BEFORE").await;
        let result = self.execute_inner().await;
        self.execute_workflow_scope_hook("WORKFLOW_AFTER").await;
        match &result {
            Ok(output) => self.persist_final(Some(output)).await,
            Err(_) => self.persist_final(None).await,
        }
        result
    }

    /// Fire a workflow-scope lifecycle hook (WORKFLOW_BEFORE / WORKFLOW_AFTER)
    /// once around the whole execution. The hook pipeline is event-only:
    /// condition failures only degrade to a skipped event, never to an
    /// execution error.
    async fn execute_workflow_scope_hook(&self, hook_type: &str) {
        let Some(entity) = self.entity.as_ref() else {
            return;
        };
        WorkflowHookEmitter::fire_workflow_point(
            entity,
            &self.hooks,
            hook_type,
            HashMap::new(),
            self.ctx.hook_handler_registry.as_deref(),
            self.ctx.event_bus.as_deref(),
        )
        .await;
        WorkflowHookEmitter::maybe_hook_checkpoint(
            &self.hooks,
            hook_type,
            self.checkpoint.as_ref(),
            entity,
        )
        .await;
    }

    /// Persist the start record (status is whatever the entity currently
    /// holds, normally `Running`).
    async fn persist_start(&self) {
        let (Some(entity), Some(manager)) = (self.entity.as_ref(), self.state_manager.as_ref())
        else {
            return;
        };
        let record =
            build_workflow_execution(entity, self.traversal.graph(), &self.ctx.options, None).await;
        manager.persist_workflow(&record).await;
    }

    /// Persist the final record after the run reaches a terminal state. When
    /// the run errored without a terminal status (e.g. node failure), the
    /// entity state is marked failed first so the record reflects reality.
    async fn persist_final(&self, output: Option<&Value>) {
        let (Some(entity), Some(manager)) = (self.entity.as_ref(), self.state_manager.as_ref())
        else {
            return;
        };

        let terminal = {
            let state = entity.state.read().await;
            matches!(
                state.status(),
                wf_execution_shared::types::execution_entity::ExecutionStatus::Completed
                    | wf_execution_shared::types::execution_entity::ExecutionStatus::Failed
                    | wf_execution_shared::types::execution_entity::ExecutionStatus::Cancelled
                    | wf_execution_shared::types::execution_entity::ExecutionStatus::Stopped
                    | wf_execution_shared::types::execution_entity::ExecutionStatus::Paused
            )
        };
        if !terminal {
            let state_transition = match entity.interruption().check() {
                Some(InterruptionSignal::Stop) => entity.state.write().await.cancel(),
                Some(InterruptionSignal::Pause) => entity.state.write().await.pause(),
                _ => entity
                    .state
                    .write()
                    .await
                    .fail("workflow execution failed".to_string()),
            };
            if let Err(e) = state_transition {
                tracing::debug!(
                    execution_id = %entity.id(),
                    error = %e,
                    "persist_final skipped terminal state transition"
                );
            }
        }

        let record = build_workflow_execution(
            entity,
            self.traversal.graph(),
            &self.ctx.options,
            output.cloned(),
        )
        .await;
        manager.persist_workflow(&record).await;
    }

    async fn execute_inner(&mut self) -> WorkflowResult<Value> {
        let entity = self.entity.clone().ok_or_else(|| {
            WorkflowError::CoordinatorError("Entity not set on WorkflowCoordinator".to_string())
        })?;

        // A freshly built entity is still `Created`; start it before the
        // loop so the terminal `complete()` transition is legal. Resumed
        // entities already hold `Running`, where the transition is
        // idempotent.
        entity.state.write().await.start()?;

        let event_bus: Option<Arc<EventBus>> = self.ctx.event_bus.clone();
        let event_bus_ref = event_bus.as_deref();

        self.emit_event(
            event_bus_ref,
            EventType::WorkflowExecutionStarted,
            &entity,
            &serde_json::json!({
                "workflow_id": self.ctx.workflow_id,
            }),
        )
        .await;

        if let Some(ref mut cp) = self.checkpoint {
            cp.on_workflow_start(&entity).await;
        }

        let node_timeout = self.ctx.options.node_timeout;

        while let Some(node_id) = &self.current_node_id.clone() {
            self.check_interruption_and_timeout(&entity, event_bus_ref, node_id)
                .await?;

            if self
                .ctx
                .options
                .max_steps
                .is_some_and(|max| self.completed_nodes.len() as u32 >= max)
            {
                break;
            }

            self.check_navigation_backstop(node_id)?;

            if self
                .skip_completed_node(&entity, event_bus_ref, node_id)
                .await?
            {
                continue;
            }

            if self.skipped_nodes.remove(node_id) {
                // Trigger-requested skip (InternalSignal::SkipNode): the
                // node is not executed; navigate from its outgoing edges.
                self.current_node_id = self.determine_next_node_without_output().await?;
                continue;
            }

            entity
                .state
                .write()
                .await
                .set_current_node(Some(node_id.clone()));

            let node = self.traversal.get_node(node_id).ok_or_else(|| {
                WorkflowError::GraphError(format!("Node {} not found in graph", node_id))
            })?;

            let node_type = parse_node_type(&node.node_type)?;
            let node_type_str = node.node_type.clone();
            let checkpoint_config = node_checkpoint_config(node_id, &node.inner)?;
            let force_checkpoint_before =
                node_force_checkpoint(&node.inner, "checkpoint_before_execute");
            let force_checkpoint_after =
                node_force_checkpoint(&node.inner, "checkpoint_after_execute");

            if let Some(ref mut cp) = self.checkpoint {
                cp.on_node_before(&entity, checkpoint_config.as_ref(), force_checkpoint_before)
                    .await;
            }
            // BEFORE_EXECUTE hook opt-in checkpoints even when the node
            // policy would not: the hook fired, so its request is honored
            // (a later veto still denies the node via the fire summary).
            WorkflowHookEmitter::maybe_hook_checkpoint(
                &self.hooks,
                "BEFORE_EXECUTE",
                self.checkpoint.as_ref(),
                &entity,
            )
            .await;

            let mut node_ctx = self.build_node_context(node_id, &node_type).await?;

            let metrics = self.ctx.metrics.clone();
            let node_metrics = metrics.as_ref().map(|m| m.node());
            if let Some(node_metrics) = &node_metrics {
                node_metrics.record_execution_start(node_id, &node_type_str);
            }
            let node_start = wf_common::now();

            let attempt = NodeAttempt {
                entity: &entity,
                node_id: node_id.as_str(),
                node_type: &node_type,
            };

            let result = self
                .execute_node_once(&attempt, node, &mut node_ctx, event_bus_ref, node_timeout)
                .await;
            let node_duration_ms = (wf_common::now() - node_start) as f64;

            let outcome = NodeOutcome {
                node_type_str: &node_type_str,
                metrics: node_metrics.as_deref(),
                start: node_start,
                duration_ms: node_duration_ms,
                checkpoint_config,
                force_checkpoint_after,
            };

            match result {
                Ok(output) => {
                    self.record_node_success(&attempt, &outcome, &node_ctx, &output)
                        .await;
                    self.current_node_id = self.determine_next_node(&output).await?;
                }
                Err(e) => {
                    self.record_node_failure(&attempt, &outcome, &node_ctx, &e)
                        .await;
                    match self
                        .route_node_failure(&entity, event_bus_ref, node_id, &node_type_str, &e)
                        .await?
                    {
                        ErrorFailureAction::Continue => {}
                        ErrorFailureAction::Suspended(suspend_err) => {
                            return Err(suspend_err);
                        }
                        ErrorFailureAction::Interrupt => {
                            // A handler may have paused the execution as the terminal
                            // handling of a failure it must not absorb (a context
                            // compression failure, for instance). End the run through
                            // the standard paused protocol so the outcome is recorded
                            // as paused-for-handling instead of a plain node failure.
                            if matches!(
                                entity.interruption().check(),
                                Some(InterruptionSignal::Pause)
                            ) {
                                self.check_interruption_and_timeout(
                                    &entity,
                                    event_bus_ref,
                                    node_id,
                                )
                                .await?;
                            }
                            return Err(e);
                        }
                    }
                }
            }

            // Trigger actions (Stop/Pause/Resume/Skip) publish typed
            // signals; translate them into entity interruption so the next
            // iteration of the loop handles them through the standard path.
            self.process_trigger_effects(&entity).await;
        }

        let result = self.compute_final_output();
        // A branch that reaches the end without an explicit JOIN merge keeps
        // the default no-write-back rule: drop the overlay and its error
        // namespace instead of leaking branch writes into the main path.
        self.discard_error_scope();
        let execution_time = now() - self.start_time;

        entity.state.write().await.complete()?;

        self.emit_event(
            event_bus_ref,
            EventType::WorkflowExecutionCompleted,
            &entity,
            &serde_json::json!({
                "execution_time": execution_time,
                "node_count": self.completed_nodes.len(),
            }),
        )
        .await;

        if let Some(ref mut cp) = self.checkpoint {
            cp.on_workflow_end(&entity).await;
        }

        Ok(result)
    }

    /// Abort the run when the execution is interrupted (Stopped/Paused) or
    /// exceeds its wall-clock `max_execution_time`. Emits the matching event
    /// and marks the entity state; returns `Err` to stop the main loop.
    async fn check_interruption_and_timeout(
        &mut self,
        entity: &WorkflowExecutionEntity,
        event_bus: Option<&EventBus>,
        node_id: &str,
    ) -> WorkflowResult<()> {
        let interruption_check = check_execution_interruption(entity.interruption(), None);
        match interruption_check {
            wf_execution_shared::types::interruption::ExecutionInterruptionCheckResult::Stopped { .. } => {
                entity
                    .state
                    .write()
                    .await
                    .record_interruption(serde_json::json!({
                        "type": "stop",
                        "recovered": false,
                        "timestamp": now(),
                    }));
                self.emit_event(
                    event_bus,
                    EventType::WorkflowExecutionCancelled,
                    entity,
                    &serde_json::json!({ "reason": "interrupted" }),
                )
                .await;
                // Persist the cancelled state so the run can be audited and
                // resumed from storage instead of only living in memory.
                if let Some(ref mut cp) = self.checkpoint {
                    cp.on_interruption(entity).await;
                }
                return Err(WorkflowError::CoordinatorError(
                    "Execution stopped by interruption".to_string(),
                ));
            }
            wf_execution_shared::types::interruption::ExecutionInterruptionCheckResult::Paused { .. } => {
                entity
                    .state
                    .write()
                    .await
                    .record_interruption(serde_json::json!({
                        "type": "pause",
                        "recovered": true,
                        "timestamp": now(),
                    }));
                self.emit_event(
                    event_bus,
                    EventType::WorkflowExecutionPaused,
                    entity,
                    &serde_json::json!({ "node_id": node_id }),
                )
                .await;
                // The state is already `Paused`; snapshot it so a paused run
                // survives a crash and can be resumed from storage.
                if let Some(ref mut cp) = self.checkpoint {
                    cp.on_pause(entity).await;
                }
                return Err(WorkflowError::CoordinatorError(
                    "Execution paused".to_string(),
                ));
            }
            _ => {}
        }

        if let Some(max_execution_time) = self.ctx.options.max_execution_time {
            if max_execution_time > 0 && (now() - self.start_time) as u64 >= max_execution_time {
                tracing::warn!(
                    execution_id = %entity.id(),
                    max_execution_time,
                    "Workflow execution wall-clock timeout exceeded, stopping execution"
                );
                self.emit_event(
                    event_bus,
                    EventType::WorkflowExecutionCancelled,
                    entity,
                    &serde_json::json!({
                        "reason": "max_execution_time",
                        "max_execution_time": max_execution_time,
                    }),
                )
                .await;
                {
                    if let Some(ref metrics) = self.ctx.metrics {
                        metrics.timeout().record_expiration(
                            "workflow_wall_clock",
                            (now() - self.start_time) as f64,
                            &entity.id().to_string(),
                        );
                        metrics
                            .workflow()
                            .record_timeout(&entity.workflow_id().to_string());
                    }
                    let mut state = entity.state.write().await;
                    state.increment_timeout_count();
                    state.record_interruption(serde_json::json!({
                        "type": "timeout",
                        "reason": "max_execution_time",
                        "max_execution_time": max_execution_time,
                        "recovered": false,
                        "timestamp": now(),
                    }));
                    state.timeout("Workflow execution exceeded max_execution_time".to_string())?;
                }
                if let Some(ref mut cp) = self.checkpoint {
                    cp.on_timeout(entity).await;
                }
                return Err(WorkflowError::ExecutionTimeout(format!(
                    "Workflow execution exceeded max_execution_time ({}ms)",
                    max_execution_time
                )));
            }
        }
        Ok(())
    }

    /// Loop-aware navigation backstop. Tracks two separate counters:
    ///
    /// 1. **`navigation_count`**: Counts node navigations *outside* any loop
    ///    body. Cycles in DAG edges that never pass through a LOOP_START node
    ///    keep accumulating and eventually trip the detector.
    ///
    /// 2. **`loop_navigation_count`**: Counts node navigations *inside* the
    ///    current loop body. Reset each time a LOOP_START is re-entered.
    ///    This catches loops whose body alone is too large relative to the
    ///    graph size (e.g., a loop body with 100 nodes and a multiplier of 5
    ///    would trigger at 500 iterations within a single pass).
    ///
    /// Both counters use `total_node_count * max_navigation_multiplier` as
    /// their threshold. The `max_iterations` cap on LOOP_START provides the
    /// primary bound; this backstop is a safety net for loops that bypass
    /// the iteration counter or for non-loop cycles.
    fn check_navigation_backstop(&mut self, node_id: &str) -> WorkflowResult<()> {
        let node_type = self
            .traversal
            .get_node(node_id)
            .map(|n| n.node_type.as_str());

        match node_type {
            Some("LOOP_START") => {
                // Entering a loop body: reset the loop-local counter and
                // mark that we are inside a loop scope. The global
                // navigation_count is *not* reset here – it continues to
                // track non-loop cycles.
                self.in_loop_body = true;
                self.loop_navigation_count = 0;
            }
            Some("LOOP_END") => {
                // Exiting the loop body: clear the in-loop flag. The next
                // node (outside the loop) will resume incrementing
                // navigation_count.
                self.in_loop_body = false;
                self.loop_navigation_count = 0;
            }
            _ => {}
        }

        if self.in_loop_body {
            self.loop_navigation_count += 1;
            let max_allowed = self.total_node_count * self.max_navigation_multiplier;
            if self.loop_navigation_count > max_allowed && max_allowed > 0 {
                return Err(WorkflowError::CoordinatorError(format!(
                    "Loop body exceeded navigation limit: {} node visits (max {} = {} nodes x {})",
                    self.loop_navigation_count,
                    max_allowed,
                    self.total_node_count,
                    self.max_navigation_multiplier
                )));
            }
        } else {
            self.navigation_count += 1;
            let max_allowed = self.total_node_count * self.max_navigation_multiplier;
            if self.navigation_count > max_allowed && max_allowed > 0 {
                return Err(WorkflowError::CoordinatorError(format!(
                    "Infinite loop detected: {} navigations exceeded max {} ({} nodes x {})",
                    self.navigation_count,
                    max_allowed,
                    self.total_node_count,
                    self.max_navigation_multiplier
                )));
            }
        }
        Ok(())
    }

    /// Completed-node skip decision. Loop iterations legitimately re-visit
    /// completed nodes: a completed node re-executes when a loop is active
    /// and the node belongs to an earlier iteration (missing from the current
    /// iteration's completion list) or is a loop control node
    /// (LOOP_START/LOOP_END, always idempotent). Completed-node skipping
    /// otherwise applies (checkpoint resume semantics). Returns `true` when
    /// the caller should `continue` the main loop.
    async fn skip_completed_node(
        &mut self,
        entity: &WorkflowExecutionEntity,
        event_bus: Option<&EventBus>,
        node_id: &str,
    ) -> WorkflowResult<bool> {
        let top_loop = crate::loop_state::stack(&self.ctx.variables)
            .pop()
            .filter(|s| !s.loop_id.is_empty());
        let is_loop_control = self
            .traversal
            .get_node(node_id)
            .is_some_and(|n| matches!(n.node_type.as_str(), "LOOP_START" | "LOOP_END"));
        let reexec_in_loop = top_loop
            .as_ref()
            .is_some_and(|s| is_loop_control || !s.iteration_nodes.iter().any(|n| n == node_id));
        if self.completed_nodes.iter().any(|n| n == node_id) && !reexec_in_loop {
            self.emit_event(
                event_bus,
                EventType::NodeSkipped,
                entity,
                &serde_json::json!({
                    "node_id": node_id,
                    "reason": "already_completed",
                }),
            )
            .await;
            self.current_node_id = self.determine_next_node_without_output().await?;
            return Ok(true);
        }
        Ok(false)
    }

    /// Execute one node through the `NodeCoordinator`, wrapped in the
    /// node-level timeout when configured.
    async fn execute_node_once(
        &self,
        attempt: &NodeAttempt<'_>,
        node: &wf_types::workflow_execution::WorkflowNode,
        node_ctx: &mut NodeExecutionContext,
        event_bus: Option<&EventBus>,
        node_timeout: Option<u64>,
    ) -> WorkflowResult<NodeExecutionResult> {
        let entity = attempt.entity;
        let node_id = attempt.node_id;
        let node_type = attempt.node_type;

        let handler =
            self.resolve_node_handler(node_type)
                .ok_or_else(|| WorkflowError::HandlerNotFound {
                    node_type: node.node_type.clone(),
                })?;

        let coordinator = NodeCoordinator::new();
        let timeout_dur = resolve_node_timeout(node, node_type, node_timeout);

        let fut = coordinator.execute_node(
            entity,
            handler,
            node_ctx,
            event_bus,
            &self.hooks,
            self.ctx.hook_handler_registry.as_deref(),
        );
        // Panic isolation: a panicking handler must surface as a routed node
        // failure instead of aborting the whole execution task.
        let guarded = AssertUnwindSafe(fut).catch_unwind();
        let panic_failure = |payload: Box<dyn Any + Send>| {
            tracing::error!(node_id = %node_id, "node handler panicked");
            WorkflowError::NodeFailure {
                node_id: node_id.to_string(),
                category: wf_types::workflow::error_branch::NodeErrorCategory::BusinessFailure,
                detail: format!("node handler panicked: {}", panic_message(&payload)),
            }
        };

        match timeout_dur {
            Some(tout_dur) => {
                let timeout_metrics = self.ctx.metrics.as_ref().map(|m| m.timeout());
                let execution_id = self.ctx.execution_id.to_string();
                if let Some(ref metrics) = timeout_metrics {
                    metrics.record_registration(
                        "workflow_node",
                        tout_dur.as_millis() as f64,
                        &execution_id,
                    );
                }
                let node_start = wf_common::now();
                let result = tokio::time::timeout(tout_dur, guarded)
                    .await
                    .map_err(|_| {
                        WorkflowError::NodeFailure {
                            node_id: node_id.to_string(),
                            category: wf_types::workflow::error_branch::NodeErrorCategory::TransportTimeout,
                            detail: format!("timed out after {:?}", tout_dur),
                        }
                    })
                    .and_then(|handler_result| handler_result.map_err(panic_failure));
                match &result {
                    Err(_) => {
                        if let Some(ref metrics) = timeout_metrics {
                            metrics.record_expiration(
                                "workflow_node",
                                (wf_common::now() - node_start) as f64,
                                &execution_id,
                            );
                        }
                    }
                    Ok(_) => {
                        if let Some(ref metrics) = timeout_metrics {
                            metrics.record_cancellation("workflow_node", "complete", &execution_id);
                        }
                    }
                }
                result?
            }
            None => guarded.await.map_err(panic_failure)?,
        }
    }

    /// Record a successful node execution: outputs, completion state, audit
    /// record, metrics and node-level checkpoint.
    async fn record_node_success(
        &mut self,
        attempt: &NodeAttempt<'_>,
        outcome: &NodeOutcome<'_>,
        node_ctx: &NodeExecutionContext,
        output: &NodeExecutionResult,
    ) {
        let entity = attempt.entity;
        let node_id = attempt.node_id;
        let node_type_str = outcome.node_type_str;
        let node_metrics = outcome.metrics;
        let node_start = outcome.start;
        let node_duration_ms = outcome.duration_ms;
        let checkpoint_config = &outcome.checkpoint_config;

        self.node_outputs
            .insert(node_id.to_string(), output.output.clone());
        self.completed_nodes.push(node_id.to_string());
        self.record_loop_iteration_completion(node_id);
        entity.set_node_result(node_id.to_string(), output.output.clone());

        for (k, v) in &output.metadata {
            self.ctx.variables.insert(k.clone(), v.clone());
        }

        entity
            .state
            .write()
            .await
            .mark_node_completed(node_id.to_string());

        self.record_node_execution(
            entity,
            ExecutionAttempt {
                node_id,
                node_type: node_type_str,
                start_time: node_start,
                success: true,
                error: None,
                input: Some(node_ctx.input.clone()),
                result: Some(output.output.clone()),
                branch_id: None,
            },
        )
        .await;

        if let Some(ref mut cp) = self.checkpoint {
            cp.on_node_completed(
                entity,
                checkpoint_config.as_ref(),
                outcome.force_checkpoint_after,
            )
            .await;
        }
        WorkflowHookEmitter::maybe_hook_checkpoint(
            &self.hooks,
            "AFTER_EXECUTE",
            self.checkpoint.as_ref(),
            entity,
        )
        .await;

        if let Some(node_metrics) = node_metrics {
            node_metrics.record_execution(MetricsNodeExecutionRecord {
                node_id,
                node_type: node_type_str,
                execution_id: &self.ctx.execution_id,
                success: true,
                duration_ms: node_duration_ms,
                input_size: json_size(&node_ctx.input),
                output_size: json_size(&output.output),
                error_type: None,
            });
        }

        // Publish the branch's public variables into the fork registry after
        // every completed node (SYNC reads the source branch's live state).
        if let Some((registry, path_id)) = &self.fork_branch_progress {
            let snapshot = self
                .ctx
                .variables
                .iter()
                .filter(|entry| !entry.key().starts_with("__"))
                .map(|entry| (entry.key().clone(), entry.value().clone()))
                .collect();
            registry.update_variables(path_id, snapshot);
        }

        // An explicit merge point brings isolated error-branch writes back to
        // the main path (last-writer-wins with audit) and reclaims the error
        // namespace; the run continues as a normal execution.
        if self.error_scope.is_some()
            && self
                .traversal
                .get_node(node_id)
                .is_some_and(|node| is_merge_point(&node.inner))
        {
            self.merge_error_scope(entity).await;
        }
    }

    /// Record a failed node execution: error chain, audit record, metrics and
    /// node-level checkpoint.
    async fn record_node_failure(
        &mut self,
        attempt: &NodeAttempt<'_>,
        outcome: &NodeOutcome<'_>,
        node_ctx: &NodeExecutionContext,
        error: &WorkflowError,
    ) {
        let entity = attempt.entity;
        let node_id = attempt.node_id;
        let node_type_str = outcome.node_type_str;
        let node_metrics = outcome.metrics;
        let node_start = outcome.start;
        let node_duration_ms = outcome.duration_ms;
        let checkpoint_config = &outcome.checkpoint_config;

        self.node_errors
            .push(format!("Node {}: {}", node_id, error));

        Self::record_workflow_error(entity, error, node_id).await;

        self.record_node_execution(
            entity,
            ExecutionAttempt {
                node_id,
                node_type: node_type_str,
                start_time: node_start,
                success: false,
                error: Some(error.to_string()),
                input: Some(node_ctx.input.clone()),
                result: None,
                branch_id: None,
            },
        )
        .await;

        if let Some(ref mut cp) = self.checkpoint {
            cp.on_node_failed(entity, checkpoint_config.as_ref()).await;
        }
        WorkflowHookEmitter::maybe_hook_checkpoint(
            &self.hooks,
            "ON_ERROR",
            self.checkpoint.as_ref(),
            entity,
        )
        .await;

        if let Some(node_metrics) = node_metrics {
            node_metrics.record_execution(MetricsNodeExecutionRecord {
                node_id,
                node_type: node_type_str,
                execution_id: &self.ctx.execution_id,
                success: false,
                duration_ms: node_duration_ms,
                input_size: json_size(&node_ctx.input),
                output_size: 0,
                error_type: Some("node_failed"),
            });
        }
    }

    /// Route a terminal node failure through the error-branch table before
    /// interrupting the execution. Node routes win in declaration order, then
    /// the workflow catch-all default; no match keeps fail-fast. A matched
    /// continue route jumps to its target on an isolated overlay, a matched
    /// suspend route parks the execution for external recovery.
    async fn route_node_failure(
        &mut self,
        entity: &WorkflowExecutionEntity,
        event_bus: Option<&EventBus>,
        failed_node_id: &str,
        node_type_str: &str,
        error: &WorkflowError,
    ) -> WorkflowResult<ErrorFailureAction> {
        let category = classify_error(error);
        // External cancellation always wins: a pending Stop is never
        // re-routed into a branch.
        if matches!(
            entity.interruption().check(),
            Some(InterruptionSignal::Stop)
        ) {
            return Ok(ErrorFailureAction::Interrupt);
        }
        let Some(target) = self
            .traversal
            .error_table()
            .resolve(failed_node_id, category)
        else {
            return Ok(ErrorFailureAction::Interrupt);
        };
        if self.traversal.get_node(&target.target_node_id).is_none() {
            return Err(WorkflowError::ConfigError {
                node_id: failed_node_id.to_string(),
                field: "error_route".to_string(),
                detail: format!(
                    "error route target '{}' does not exist in the graph",
                    target.target_node_id
                ),
            });
        }
        check_branch_budget(
            self.traversal.graph(),
            &target.target_node_id,
            self.ctx.options.max_steps,
            self.completed_nodes.len(),
        );
        if let Some(ref metrics) = self.ctx.metrics {
            metrics
                .node()
                .record_error(failed_node_id, node_type_str, category.as_str());
        }
        // `attempts` counts this node's recorded terminal failures (error
        // records), not engine-level retries: transport retries inside the
        // LLM/script layers are invisible to the coordinator.
        let attempts = entity
            .state
            .read()
            .await
            .error_records()
            .iter()
            .filter(|record| record.node_id.as_deref() == Some(failed_node_id))
            .count()
            .max(1) as u32;
        let summary = wf_types::workflow::error_branch::ErrorBranchSummary::new(
            error.to_string(),
            category,
            failed_node_id,
            attempts,
        );
        if target.suspend {
            return Ok(self
                .suspend_error_branch(entity, event_bus, summary, &target.target_node_id)
                .await);
        }
        // An explicit error branch takes precedence over a handler-requested
        // pause (the compression Fail fallback parks through the same pause
        // signal): the graph author handles this failure internally. A Stop
        // is never cleared — external cancellation always wins.
        if matches!(
            entity.interruption().check(),
            Some(InterruptionSignal::Pause)
        ) {
            let _ = entity.interruption().resume();
        }
        self.enter_error_branch(entity, event_bus, summary, false)
            .await;
        self.current_node_id = Some(target.target_node_id.clone());
        Ok(ErrorFailureAction::Continue)
    }

    /// Enter (or re-enter, on nested branch failures) the isolated error
    /// scope: freeze the main-path map, run the branch on an overlay cloned
    /// from the entry snapshot plus the read-only error namespace.
    async fn enter_error_branch(
        &mut self,
        entity: &WorkflowExecutionEntity,
        event_bus: Option<&EventBus>,
        summary: wf_types::workflow::error_branch::ErrorBranchSummary,
        from_suspend: bool,
    ) {
        let event_type = if from_suspend {
            wf_types::events::EventType::WorkflowErrorBranchResumed
        } else {
            wf_types::events::EventType::WorkflowErrorBranchTaken
        };
        if let Some(scope) = self.error_scope.as_mut() {
            scope.summary = summary.clone();
            for (key, value) in summary.variables() {
                self.ctx.variables.insert(key, value);
            }
        } else {
            let main_variables = self.ctx.variables.clone();
            let snapshot: HashMap<String, Value> = main_variables
                .iter()
                .map(|entry| (entry.key().clone(), entry.value().clone()))
                .collect();
            let overlay = std::sync::Arc::new(dashmap::DashMap::new());
            for (key, value) in &snapshot {
                overlay.insert(key.clone(), value.clone());
            }
            for (key, value) in summary.variables() {
                overlay.insert(key, value);
            }
            self.ctx.variables = overlay;
            self.error_scope = Some(ErrorBranchScope {
                summary: summary.clone(),
                main_variables,
                snapshot,
            });
        }
        self.emit_event(
            event_bus,
            event_type,
            entity,
            &serde_json::json!({
                "error_category": summary.category.as_str(),
                "error_message": summary.message,
                "source_node_id": summary.source_node_id,
                "attempts": summary.attempts,
            }),
        )
        .await;
    }

    /// Park the execution at an error suspend point: persist the typed
    /// recovery record into the execution state (checkpointed through the
    /// state snapshot's `error_suspend` domain, never the business variable
    /// map), point the resume cursor at the branch target, and end the run
    /// through the standard paused protocol so a crash still resumes from
    /// storage. The failed node itself never re-runs; recovery continues
    /// from the branch target.
    async fn suspend_error_branch(
        &mut self,
        entity: &WorkflowExecutionEntity,
        event_bus: Option<&EventBus>,
        summary: wf_types::workflow::error_branch::ErrorBranchSummary,
        target_node_id: &str,
    ) -> ErrorFailureAction {
        entity
            .state
            .write()
            .await
            .set_error_suspend(Some(ErrorSuspendState {
                summary: summary.clone(),
                target_node_id: target_node_id.to_string(),
                suspended_at: now(),
            }));
        entity
            .state
            .write()
            .await
            .set_current_node(Some(target_node_id.to_string()));
        let _ = entity.interruption().pause();
        self.emit_event(
            event_bus,
            wf_types::events::EventType::WorkflowErrorBranchSuspended,
            entity,
            &serde_json::json!({
                "error_category": summary.category.as_str(),
                "error_message": summary.message,
                "source_node_id": summary.source_node_id,
                "target_node_id": target_node_id,
                "attempts": summary.attempts,
            }),
        )
        .await;
        match self
            .check_interruption_and_timeout(entity, event_bus, target_node_id)
            .await
        {
            Err(paused) => ErrorFailureAction::Suspended(paused),
            Ok(()) => {
                // The pause was cleared concurrently: fall back to continuing
                // on the isolated branch instead of losing the failure, and
                // consume the suspend record so it cannot resurrect on a
                // later checkpoint restore.
                entity.state.write().await.set_error_suspend(None);
                self.enter_error_branch(entity, event_bus, summary, false)
                    .await;
                self.current_node_id = Some(target_node_id.to_string());
                ErrorFailureAction::Continue
            }
        }
    }

    /// Explicit merge point: flush isolated branch writes back to the main
    /// path (last-writer-wins), audit the conflicting keys with names only,
    /// and reclaim the error namespace by restoring the main-path map.
    async fn merge_error_scope(&mut self, entity: &WorkflowExecutionEntity) {
        let Some(scope) = self.error_scope.take() else {
            return;
        };
        let event_bus = self.ctx.event_bus.clone();
        let mut merged: Vec<String> = Vec::new();
        let mut conflicts: Vec<String> = Vec::new();
        for entry in self.ctx.variables.iter() {
            let key = entry.key().clone();
            if ErrorBranchScope::is_machinery_key(&key) {
                continue;
            }
            let value = entry.value().clone();
            match scope.snapshot.get(&key) {
                Some(before) if before == &value => {}
                Some(_) => {
                    scope.main_variables.insert(key.clone(), value);
                    merged.push(key.clone());
                    conflicts.push(key);
                }
                None => {
                    scope.main_variables.insert(key.clone(), value);
                    merged.push(key);
                }
            }
        }
        self.ctx.variables = scope.main_variables;
        if !merged.is_empty() {
            self.emit_event(
                event_bus.as_deref(),
                wf_types::events::EventType::VariableChanged,
                entity,
                &serde_json::json!({
                    "merged_keys": merged,
                    "conflicts": conflicts,
                    "source": "error_branch_merge",
                }),
            )
            .await;
        }
    }

    /// Drop an unfinished error scope without merging (branch writes stay
    /// isolated per the default no-write-back rule) and restore the
    /// main-path map.
    fn discard_error_scope(&mut self) {
        if let Some(scope) = self.error_scope.take() {
            self.ctx.variables = scope.main_variables;
        }
    }

    /// Restore a checkpointed suspend: consume the typed `error_suspend`
    /// record from the execution state and rebuild the isolated scope plus
    /// the error namespace, so the resumed run continues from the branch
    /// target exactly as a fresh suspend entry would. Consuming the record
    /// keeps a resumed run from re-persisting it. No-op when nothing was
    /// suspended.
    pub async fn restore_suspended_error_branch(&mut self) {
        if self.error_scope.is_some() {
            return;
        }
        let entity = match self.entity.clone() {
            Some(entity) => entity,
            None => return,
        };
        let Some(state) = entity.state.write().await.take_error_suspend() else {
            return;
        };
        let event_bus = self.ctx.event_bus.clone();
        self.enter_error_branch(&entity, event_bus.as_deref(), state.summary, true)
            .await;
    }

    async fn process_trigger_effects(&mut self, entity: &WorkflowExecutionEntity) {
        // Typed signal bus. Check signals that target this
        // execution and react accordingly.
        if let Some(signal_receiver) = &mut self.signal_receiver {
            let execution_id = self.ctx.execution_id.to_string();
            while let Some(signal) = signal_receiver.try_recv() {
                if *signal.target_execution_id() != execution_id {
                    continue;
                }
                match signal {
                    InternalSignal::StopWorkflow { .. } => {
                        let _ = entity.interruption().stop();
                        return;
                    }
                    InternalSignal::PauseWorkflow { .. } => {
                        let _ = entity.interruption().pause();
                    }
                    InternalSignal::ResumeWorkflow { .. } => {
                        let _ = entity.interruption().resume();
                    }
                    InternalSignal::SkipNode { node_id, .. } => {
                        // Record the node for skipping at dispatch time.
                        self.skipped_nodes.insert(node_id);
                    }
                    _ => {
                        // Result signals (SubworkflowResult, ScriptResult,
                        // AgentResult) are consumed by the agent loop,
                        // not the workflow coordinator.
                    }
                }
            }
        }
    }

    async fn determine_next_node_without_output(&self) -> WorkflowResult<Option<String>> {
        let current_id = match &self.current_node_id {
            Some(id) => id.clone(),
            None => return Ok(None),
        };

        let outgoing = self.traversal.get_outgoing_edges(&current_id);
        if outgoing.is_empty() {
            return Ok(None);
        }

        if outgoing.len() == 1 {
            return Ok(Some(outgoing[0].target_node_id.clone()));
        }

        for edge in &outgoing {
            if let Some(ref condition) = edge.condition {
                let mut context_map = HashMap::new();
                for entry in self.ctx.variables.iter() {
                    context_map.insert(entry.key().clone(), entry.value().clone());
                }
                match ConditionEvaluator::evaluate(condition, &context_map) {
                    Ok(true) => return Ok(Some(edge.target_node_id.clone())),
                    // An unevaluable edge keeps the "do not take this edge"
                    // semantics, but the defect is logged so broken edge
                    // conditions stay discoverable.
                    Ok(false) => continue,
                    Err(e) => {
                        tracing::warn!(
                            edge_from = %current_id,
                            edge_to = %edge.target_node_id,
                            error = %e,
                            "edge condition failed to evaluate; edge skipped"
                        );
                        continue;
                    }
                }
            } else {
                return Ok(Some(edge.target_node_id.clone()));
            }
        }

        Ok(None)
    }

    async fn build_node_context(
        &self,
        node_id: &str,
        node_type: &StaticNodeType,
    ) -> WorkflowResult<NodeExecutionContext> {
        let (input, input_shape) = self.compute_node_input(node_id);

        let node = self.traversal.get_node(node_id);
        let node_name = node.and_then(|n| n.name.clone());
        let node_config = node.map(|n| n.inner.clone());

        let mut ctx = NodeExecutionContext::new(
            self.ctx.execution_id.clone(),
            node_id.to_string(),
            node_type.clone(),
            input,
            self.ctx.variables.clone(),
        );
        ctx.input_shape = input_shape;

        if let Some(name) = node_name {
            ctx = ctx.with_node_name(name);
        }
        if let Some(config) = node_config {
            ctx = ctx.with_node_config(config);
        }
        if let Some(ref parent_id) = self.ctx.parent_execution_id {
            ctx = ctx.with_parent_execution(parent_id.clone());
        }
        ctx.event_bus = self.ctx.event_bus.clone();
        ctx.handler_registry = Some(self.handlers.clone());
        ctx.graph_structure = Some(Arc::new(self.traversal.graph().clone()));
        ctx.tool_registry = Some(self.ctx.tool_registry.clone());
        ctx.resource_registries = self.ctx.resource_registries.clone();
        ctx.metrics = self.ctx.metrics.clone();
        ctx.token_tracker = self.ctx.token_tracker.clone();
        ctx.cancellation = self.entity.as_ref().map(|e| e.get_abort_signal());
        ctx.interruption = self.entity.as_ref().map(|e| e.interruption().clone());
        ctx.hook_handler_registry = self.ctx.hook_handler_registry.clone();
        ctx.tool_approval_handler = self.ctx.tool_approval_handler.clone();
        ctx.tool_approval_options = self.ctx.tool_approval_options.clone();
        ctx.fork_registries = self.ctx.fork_registries.clone();
        ctx.signal_bus = self.ctx.signal_bus.clone();
        // Carry the owning execution's resolved budgets so a TRIGGER
        // sub-workflow inherits the same source (entry config / limits)
        // rather than resetting to the engine fallback.
        ctx = ctx.with_parent_timeouts(
            self.ctx.options.node_timeout,
            self.ctx.options.max_execution_time,
        );

        // Message nodes execute trigger actions within one visit; give them a
        // shared session cache so consecutive actions can exchange state.
        if matches!(
            node_type,
            StaticNodeType::StartFromMessage | StaticNodeType::ContinueFromMessage
        ) {
            ctx.session_cache = Some(Arc::new(std::sync::Mutex::new(HashMap::new())));
        }

        Ok(ctx)
    }

    /// Compute a node's input and how it was assembled from incoming edges.
    ///
    /// Shape contract (aligned with `NodeInputShape`): a node with no incoming
    /// edges receives the workflow-level input; a node with exactly one
    /// incoming edge receives that source's raw output unwrapped (`Single`);
    /// a node with multiple incoming edges receives an object merging each
    /// edge's output keyed by source node id / edge label (`Merged`).
    fn compute_node_input(&self, node_id: &str) -> (Value, NodeInputShape) {
        let incoming_edges = self.traversal.get_incoming_edges(node_id);

        if incoming_edges.is_empty() {
            return (
                self.ctx.options.input.clone().unwrap_or(Value::Null),
                NodeInputShape::None,
            );
        }

        let mut inputs = serde_json::Map::new();
        for edge in incoming_edges {
            if let Some(output) = self.node_outputs.get(&edge.source_node_id) {
                let key = edge.label.as_deref().unwrap_or(&edge.source_node_id);
                inputs.insert(key.to_string(), output.clone());
            }
        }

        if inputs.len() == 1 {
            (
                inputs.values().next().cloned().unwrap_or(Value::Null),
                NodeInputShape::Single,
            )
        } else {
            (Value::Object(inputs), NodeInputShape::Merged)
        }
    }

    async fn determine_next_node(
        &self,
        result: &NodeExecutionResult,
    ) -> WorkflowResult<Option<String>> {
        if !result.next_node_ids.is_empty() {
            return Ok(result.next_node_ids.first().cloned());
        }

        let current_id = match &self.current_node_id {
            Some(id) => id.clone(),
            None => return Ok(None),
        };
        let outgoing = self.traversal.get_outgoing_edges(&current_id);

        if outgoing.is_empty() {
            return Ok(None);
        }

        if self.traversal.is_end_node(&current_id) {
            return Ok(None);
        }

        if outgoing.len() == 1 {
            let edge = &outgoing[0];
            return Ok(Some(edge.target_node_id.clone()));
        }

        for edge in &outgoing {
            if let Some(ref condition) = edge.condition {
                let mut context_map = HashMap::new();
                for entry in self.ctx.variables.iter() {
                    context_map.insert(entry.key().clone(), entry.value().clone());
                }
                match ConditionEvaluator::evaluate(condition, &context_map) {
                    Ok(true) => return Ok(Some(edge.target_node_id.clone())),
                    // An unevaluable edge keeps the "do not take this edge"
                    // semantics, but the defect is logged so broken edge
                    // conditions stay discoverable.
                    Ok(false) => continue,
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            "edge condition failed to evaluate; edge skipped"
                        );
                        continue;
                    }
                }
            } else {
                return Ok(Some(edge.target_node_id.clone()));
            }
        }

        Ok(None)
    }

    fn compute_final_output(&self) -> Value {
        let end_ids = self.traversal.end_node_ids();
        if end_ids.is_empty() {
            return Value::Null;
        }

        let mut outputs = serde_json::Map::new();
        for id in end_ids {
            if let Some(output) = self.node_outputs.get(id) {
                outputs.insert(id.clone(), output.clone());
            }
        }

        if outputs.len() == 1 {
            outputs.values().next().cloned().unwrap_or(Value::Null)
        } else if outputs.is_empty() {
            Value::Null
        } else {
            Value::Object(outputs)
        }
    }

    async fn emit_event(
        &self,
        event_bus: Option<&EventBus>,
        event_type: EventType,
        entity: &WorkflowExecutionEntity,
        data: &serde_json::Value,
    ) {
        let Some(bus) = event_bus else {
            tracing::debug!(
                execution_id = %entity.id(),
                ?event_type,
                "no event bus attached, skipping event emission"
            );
            return;
        };
        let metadata = data.as_object().map(|obj| {
            obj.iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<HashMap<_, _>>()
        });
        // Lifecycle events are observability-critical: surface the loss at
        // error level. Execution itself is never aborted by a failed publish
        // (events are a side channel).
        let critical = matches!(
            event_type,
            EventType::WorkflowExecutionStarted
                | EventType::WorkflowExecutionCompleted
                | EventType::WorkflowExecutionFailed
                | EventType::WorkflowExecutionCancelled
        );
        let event_type_label = format!("{:?}", event_type);

        let event = BaseEvent {
            id: wf_types::Id::new(),
            r#type: event_type,
            timestamp: now(),
            event_name: None,
            workflow_id: Some(entity.workflow_id().clone()),
            execution_id: Some(entity.id().clone()),
            agent_loop_id: None,
            metadata,
        };
        match bus.publish_logged(
            event,
            &format!(
                "workflow={} node={}",
                entity.id(),
                self.current_node_id.as_deref().unwrap_or("")
            ),
        ) {
            Err(err) if critical => {
                tracing::error!(
                    execution_id = %entity.id(),
                    event_type = %event_type_label,
                    error = ?err,
                    "critical lifecycle event publish failed"
                );
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use wf_tools::registry::ToolRegistry;
    use wf_types::workflow::EdgeType;
    use wf_types::workflow_execution::{WorkflowEdge, WorkflowExecutionOptions, WorkflowNode};

    fn node(id: &str, node_type: &str, inner: Value) -> WorkflowNode {
        WorkflowNode {
            id: id.to_string(),
            name: Some(id.to_string()),
            node_type: node_type.to_string(),
            inner,
        }
    }

    fn edge(source: &str, target: &str) -> WorkflowEdge {
        WorkflowEdge {
            id: format!("{}-{}", source, target),
            source_node_id: source.to_string(),
            target_node_id: target.to_string(),
            r#type: EdgeType::Default,
            condition: None,
            label: None,
            description: None,
            error_route: None,
        }
    }

    fn graph(nodes: Vec<WorkflowNode>) -> WorkflowGraphStructure {
        WorkflowGraphStructure {
            edges: nodes.windows(2).map(|w| edge(&w[0].id, &w[1].id)).collect(),
            nodes,
            adjacency_list: HashMap::new(),
            reverse_adjacency_list: HashMap::new(),
            start_node_id: Some("start".to_string()),
            end_node_ids: vec!["end".to_string()],
            error_default: None,
        }
    }

    #[test]
    fn node_timeout_seconds_is_applied_as_seconds() {
        let n = node("x", "LLM", serde_json::json!({ "timeout_seconds": 2 }));
        let dur = resolve_node_timeout(&n, &StaticNodeType::Llm, None).expect("configured");
        assert_eq!(dur, std::time::Duration::from_secs(2));
    }

    #[test]
    fn node_timeout_priority_order() {
        let n = node("x", "LLM", serde_json::json!({ "timeout_seconds": 5 }));
        let dur = resolve_node_timeout(&n, &StaticNodeType::Llm, Some(1000)).expect("configured");
        assert_eq!(dur, std::time::Duration::from_secs(5));
        let n = node("x", "LLM", serde_json::json!({}));
        let dur = resolve_node_timeout(&n, &StaticNodeType::Llm, Some(1000)).expect("configured");
        assert_eq!(dur, std::time::Duration::from_millis(1000));
    }

    #[test]
    fn long_running_nodes_skip_the_engine_fallback_only() {
        let n = node("x", "AGENT_LOOP", serde_json::json!({}));
        assert_eq!(
            resolve_node_timeout(&n, &StaticNodeType::AgentLoop, None),
            None
        );
        let dur = resolve_node_timeout(&n, &StaticNodeType::AgentLoop, Some(7000))
            .expect("explicit default still applies");
        assert_eq!(dur, std::time::Duration::from_millis(7000));
        let n = node("x", "LLM", serde_json::json!({}));
        let dur = resolve_node_timeout(&n, &StaticNodeType::Llm, None).expect("fallback applies");
        assert_eq!(
            dur,
            std::time::Duration::from_millis(DEFAULT_NODE_TIMEOUT_MS)
        );
    }

    fn options() -> WorkflowExecutionOptions {
        WorkflowExecutionOptions {
            input: None,
            max_steps: None,
            timeout: None,
            max_execution_time: None,
            enable_checkpoints: Some(false),
            node_timeout: None,
            max_pause_duration: None,
            max_navigation_multiplier: None,
            loop_max_iterations_cap: None,
        }
    }

    async fn run(
        g: WorkflowGraphStructure,
        opts: WorkflowExecutionOptions,
        handlers: Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>>,
    ) -> WorkflowResult<Value> {
        let exec_ctx = ExecutorContext::new(
            wf_common::generate_id(),
            wf_common::generate_id(),
            None,
            Arc::new(ToolRegistry::new()),
            opts,
        );
        let entity = WorkflowExecutionEntity::new(
            exec_ctx.execution_id.clone(),
            exec_ctx.workflow_id.clone(),
        );
        let mut coordinator = WorkflowCoordinator::new(exec_ctx, g, handlers)?.with_entity(entity);
        coordinator.execute().await
    }

    fn base_handlers(
        extra: Vec<(StaticNodeType, Box<dyn NodeHandler>)>,
    ) -> Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>> {
        let mut map: HashMap<StaticNodeType, Box<dyn NodeHandler>> = HashMap::new();
        map.insert(
            StaticNodeType::Start,
            Box::new(crate::handler::start_end::StartHandler),
        );
        map.insert(
            StaticNodeType::End,
            Box::new(crate::handler::start_end::EndHandler),
        );
        for (ty, handler) in extra {
            map.insert(ty, handler);
        }
        Arc::new(map)
    }

    struct AlwaysFailingHandler;

    #[async_trait]
    impl NodeHandler for AlwaysFailingHandler {
        fn node_type(&self) -> StaticNodeType {
            StaticNodeType::Variable
        }

        async fn execute(
            &self,
            _ctx: &mut NodeExecutionContext,
        ) -> wf_execution_shared::error::ExecutionSharedResult<NodeExecutionResult> {
            Err(WorkflowError::OperationError("always fails".to_string()).into())
        }
    }

    #[tokio::test]
    async fn node_failure_propagates() {
        let handlers = base_handlers(vec![(
            StaticNodeType::Variable,
            Box::new(AlwaysFailingHandler) as Box<dyn NodeHandler>,
        )]);
        let g = graph(vec![
            node("start", "START", Value::Null),
            node("flaky", "VARIABLE", Value::Null),
            node("end", "END", Value::Null),
        ]);
        let result = run(g, options(), handlers).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("always fails"));
    }

    #[tokio::test]
    async fn failed_node_writes_structured_error_records() {
        let handlers = base_handlers(vec![(
            StaticNodeType::Variable,
            Box::new(AlwaysFailingHandler) as Box<dyn NodeHandler>,
        )]);
        let g = graph(vec![
            node("start", "START", Value::Null),
            node("flaky", "VARIABLE", Value::Null),
            node("end", "END", Value::Null),
        ]);
        let exec_ctx = ExecutorContext::new(
            wf_common::generate_id(),
            wf_common::generate_id(),
            None,
            Arc::new(ToolRegistry::new()),
            options(),
        );
        let entity = WorkflowExecutionEntity::new(
            exec_ctx.execution_id.clone(),
            exec_ctx.workflow_id.clone(),
        );
        let mut coordinator = WorkflowCoordinator::new(exec_ctx, g, handlers)
            .unwrap()
            .with_entity(entity);
        assert!(coordinator.execute().await.is_err());

        let entity = coordinator.entity.as_ref().unwrap();
        let state = entity.state.read().await;
        let records = state.error_records();
        assert_eq!(records.len(), 1, "a failed node writes one record");
        assert_eq!(records[0].node_id.as_deref(), Some("flaky"));
        assert!(records[0].error.contains("always fails"));
        assert!(records[0].error_type.is_some());
        assert!(records[0].parent_error_id.is_none());
        assert_eq!(records[0].root_cause_id, records[0].id);
        assert_eq!(records[0].error_chain, vec![records[0].id.clone()]);
    }

    struct PassthroughHandler;

    #[async_trait]
    impl NodeHandler for PassthroughHandler {
        fn node_type(&self) -> StaticNodeType {
            StaticNodeType::Variable
        }

        async fn execute(
            &self,
            ctx: &mut NodeExecutionContext,
        ) -> wf_execution_shared::error::ExecutionSharedResult<NodeExecutionResult> {
            Ok(NodeExecutionResult::simple(ctx.input.clone()))
        }
    }

    #[tokio::test]
    async fn navigation_backstop_respects_configured_multiplier() {
        // a -> b -> a structural cycle without any LOOP node: only the
        // navigation backstop can stop it, and the configured multiplier
        // bounds how far it gets before aborting.
        let handlers = base_handlers(vec![(
            StaticNodeType::Variable,
            Box::new(PassthroughHandler) as Box<dyn NodeHandler>,
        )]);
        let g = WorkflowGraphStructure {
            nodes: vec![
                node("start", "START", Value::Null),
                node("a", "VARIABLE", Value::Null),
                node("b", "VARIABLE", Value::Null),
            ],
            edges: vec![edge("start", "a"), edge("a", "b"), edge("b", "a")],
            adjacency_list: HashMap::new(),
            reverse_adjacency_list: HashMap::new(),
            start_node_id: Some("start".to_string()),
            end_node_ids: vec![],
            error_default: None,
        };

        // Default multiplier (5): max_allowed = 3 * 5 = 15 navigations.
        let err = run(g.clone(), options(), handlers.clone())
            .await
            .expect_err("the cycle must trip the navigation backstop");
        assert!(
            err.to_string().contains("Infinite loop detected"),
            "unexpected error: {err}"
        );

        // Multiplier 1: max_allowed = 3 * 1 = 3 navigations — the cycle is
        // caught much earlier.
        let mut opts = options();
        opts.max_navigation_multiplier = Some(1);
        let err = run(g, opts, handlers)
            .await
            .expect_err("the cycle must trip the navigation backstop");
        let text = err.to_string();
        assert!(
            text.contains("Infinite loop detected") && text.contains("max 3"),
            "multiplier 1 must bound navigations to 3, got: {text}"
        );
    }

    #[tokio::test]
    async fn invalid_node_checkpoint_config_fails_with_structured_error() {
        let handlers = base_handlers(vec![(
            StaticNodeType::Variable,
            Box::new(AlwaysFailingHandler) as Box<dyn NodeHandler>,
        )]);
        let g = graph(vec![
            node("start", "START", Value::Null),
            node(
                "cp_node",
                "VARIABLE",
                serde_json::json!({
                    "checkpoint": {"everyNNodes": "not-a-number"},
                }),
            ),
            node("end", "END", Value::Null),
        ]);
        let err = run(g, options(), handlers)
            .await
            .expect_err("invalid checkpoint config must fail");
        let text = err.to_string();
        assert!(
            text.contains("Config error")
                && text.contains("cp_node")
                && text.contains("checkpoint"),
            "error must carry node id and field path: {text}"
        );
    }

    #[tokio::test]
    async fn node_input_shape_distinguishes_single_vs_merged() {
        struct CaptureHandler {
            shapes: std::sync::Arc<std::sync::Mutex<Vec<NodeInputShape>>>,
        }

        #[async_trait]
        impl NodeHandler for CaptureHandler {
            fn node_type(&self) -> StaticNodeType {
                StaticNodeType::Variable
            }

            async fn execute(
                &self,
                ctx: &mut NodeExecutionContext,
            ) -> wf_execution_shared::error::ExecutionSharedResult<NodeExecutionResult>
            {
                self.shapes.lock().unwrap().push(ctx.input_shape);
                Ok(NodeExecutionResult::simple(ctx.input.clone()))
            }
        }

        let shapes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let handlers = base_handlers(vec![(
            StaticNodeType::Variable,
            Box::new(CaptureHandler {
                shapes: shapes.clone(),
            }) as Box<dyn NodeHandler>,
        )]);

        // Linear chain: single incoming edge -> Single shape.
        let g1 = graph(vec![
            node("start", "START", Value::Null),
            node("v1", "VARIABLE", Value::Null),
            node("end", "END", Value::Null),
        ]);
        run(g1, options(), handlers.clone()).await.unwrap();
        assert_eq!(shapes.lock().unwrap().as_slice(), &[NodeInputShape::Single]);

        // Direct `compute_node_input` contract for the merged fan-in case
        // (the linear navigator never schedules two parallel sources, so the
        // merged shape is exercised on the method directly).
        let nodes = vec![
            node("start", "START", Value::Null),
            node("a", "VARIABLE", Value::Null),
            node("b", "VARIABLE", Value::Null),
            node("join_v", "VARIABLE", Value::Null),
        ];
        let g2 = WorkflowGraphStructure {
            nodes,
            edges: vec![
                edge("start", "a"),
                edge("start", "b"),
                edge("a", "join_v"),
                edge("b", "join_v"),
            ],
            adjacency_list: HashMap::new(),
            reverse_adjacency_list: HashMap::new(),
            start_node_id: Some("start".to_string()),
            end_node_ids: vec!["join_v".to_string()],
            error_default: None,
        };
        let exec_ctx = ExecutorContext::new(
            wf_common::generate_id(),
            wf_common::generate_id(),
            None,
            Arc::new(ToolRegistry::new()),
            options(),
        );
        // `new_preprocessed` skips the reachability pre-check.
        let mut coordinator = WorkflowCoordinator::new_preprocessed(exec_ctx, g2, handlers)
            .unwrap()
            .with_entity(WorkflowExecutionEntity::new(
                wf_common::generate_id(),
                wf_common::generate_id(),
            ));
        coordinator
            .node_outputs
            .insert("a".to_string(), serde_json::json!({"x": 1}));
        coordinator
            .node_outputs
            .insert("b".to_string(), serde_json::json!({"y": 2}));
        let (input, shape) = coordinator.compute_node_input("join_v");
        assert_eq!(shape, NodeInputShape::Merged);
        assert_eq!(input, serde_json::json!({"a": {"x": 1}, "b": {"y": 2}}));
    }
}
