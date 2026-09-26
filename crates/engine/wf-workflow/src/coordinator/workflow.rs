use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde_json::Value;
use wf_common::now;
use wf_core::internal_signal::InternalSignalReceiver;
use wf_execution_shared::context::ExecutorContext;
use wf_execution_shared::execution_state::ExecutionStateManager;
use wf_execution_shared::fork::ForkRegistry;
use wf_execution_shared::hooks::types::HookDefinition;
use wf_types::node::StaticNodeType;
use wf_types::workflow_execution::WorkflowGraphStructure;

use crate::checkpoint::WorkflowCheckpointIntegration;
use crate::entity::WorkflowExecutionEntity;
use crate::error::{WorkflowError, WorkflowResult};
use crate::error_branch::ErrorBranchScope;
use crate::graph::GraphTraversal;
use crate::handler::NodeHandler;

mod error_branch_flow;
mod navigation;
mod node_exec;
mod persistence;
mod routing;
mod timeout;

pub struct WorkflowCoordinator {
    pub(super) ctx: ExecutorContext,
    pub(super) entity: Option<Arc<WorkflowExecutionEntity>>,
    pub(super) traversal: GraphTraversal,
    pub(super) handlers: Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>>,
    pub(super) current_node_id: Option<String>,
    pub(super) completed_nodes: Vec<String>,
    pub(super) node_outputs: HashMap<String, Value>,
    pub(super) node_errors: Vec<String>,
    pub(super) start_time: i64,
    pub(super) hooks: Vec<HookDefinition>,
    /// Navigation counter for detecting non-loop dead cycles (e.g., cycles in
    /// DAG edges that don't pass through LOOP_START/LOOP_END). Reset when
    /// entering a loop scope; only counts nodes outside loop bodies.
    pub(super) navigation_count: u32,
    /// Navigation counter for detecting loops that execute too many iterations.
    /// Tracks the number of nodes executed within the current loop body.
    pub(super) loop_navigation_count: u32,
    /// Whether the coordinator is currently inside a loop body.
    pub(super) in_loop_body: bool,
    pub(super) total_node_count: u32,
    pub(super) max_navigation_multiplier: u32,
    pub(super) checkpoint: Option<WorkflowCheckpointIntegration>,
    /// Optional write point for the persisted `WorkflowExecution` record;
    /// wired by the application (via `wf-api`) so the coordinator persists
    /// the record at execution start and on every terminal exit.
    pub(super) state_manager: Option<ExecutionStateManager>,
    /// Optional live-variable sink for a fork branch execution: after every
    /// completed node the coordinator publishes the branch's public
    /// variables into the fork registry so SYNC nodes can read the source
    /// branch's intermediate state.
    pub(super) fork_branch_progress: Option<(Arc<ForkRegistry>, String)>,
    /// Plugin-contributed node handlers consulted when the builtin map has
    /// no handler for a node type (resolution chain: builtin → plugin).
    /// Built once per execution by the application layer from the plugin
    /// contribution source.
    pub(super) plugin_handlers: Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>>,
    /// Receiver for typed internal signals (replaces the `__`-prefixed
    /// variable protocol).
    pub(super) signal_receiver: Option<InternalSignalReceiver>,
    /// Nodes requested for skipping by `InternalSignal::SkipNode`, applied
    /// at dispatch time (one-shot per node).
    pub(super) skipped_nodes: HashSet<String>,
    /// Active error-branch variable scope (`None` on the main path). While
    /// set, the branch runs on an overlay cloned from the entry snapshot;
    /// the main-path map stays frozen until an explicit JOIN merge.
    pub(super) error_scope: Option<ErrorBranchScope>,
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
    pub(super) fn new_preprocessed(
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
    pub(super) fn resolve_node_handler(
        &self,
        node_type: &StaticNodeType,
    ) -> Option<&dyn NodeHandler> {
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

    async fn execute_inner(&mut self) -> WorkflowResult<Value> {
        let entity = self.entity.clone().ok_or_else(|| {
            WorkflowError::CoordinatorError("Entity not set on WorkflowCoordinator".to_string())
        })?;

        // A freshly built entity is still `Created`; start it before the
        // loop so the terminal `complete()` transition is legal. Resumed
        // entities already hold `Running`, where the transition is
        // idempotent.
        entity.state.write().await.start()?;

        let event_bus: Option<Arc<wf_core::EventBus>> = self.ctx.event_bus.clone();
        let event_bus_ref = event_bus.as_deref();

        self.emit_event(
            event_bus_ref,
            wf_types::events::EventType::WorkflowExecutionStarted,
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

            let node_type = routing::parse_node_type(&node.node_type)?;
            let node_type_str = node.node_type.clone();
            let checkpoint_config = timeout::node_checkpoint_config(node_id, &node.inner)?;
            let force_checkpoint_before =
                timeout::node_force_checkpoint(&node.inner, "checkpoint_before_execute");
            let force_checkpoint_after =
                timeout::node_force_checkpoint(&node.inner, "checkpoint_after_execute");

            if let Some(ref mut cp) = self.checkpoint {
                cp.on_node_before(&entity, checkpoint_config.as_ref(), force_checkpoint_before)
                    .await;
            }
            // BEFORE_EXECUTE hook opt-in checkpoints even when the node
            // policy would not: the hook fired, so its request is honored
            // (a later veto still denies the node via the fire summary).
            crate::hook::WorkflowHookEmitter::maybe_hook_checkpoint(
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

            let attempt = node_exec::NodeAttempt {
                entity: &entity,
                node_id: node_id.as_str(),
                node_type: &node_type,
            };

            let result = self
                .execute_node_once(&attempt, node, &mut node_ctx, event_bus_ref, node_timeout)
                .await;
            let node_duration_ms = (wf_common::now() - node_start) as f64;

            let outcome = node_exec::NodeOutcome {
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
                        crate::error_branch::ErrorFailureAction::Continue => {}
                        crate::error_branch::ErrorFailureAction::Suspended(suspend_err) => {
                            return Err(suspend_err);
                        }
                        crate::error_branch::ErrorFailureAction::Interrupt => {
                            // A handler may have paused the execution as the terminal
                            // handling of a failure it must not absorb (a context
                            // compression failure, for instance). End the run through
                            // the standard paused protocol so the outcome is recorded
                            // as paused-for-handling instead of a plain node failure.
                            if matches!(
                                entity.interruption().check(),
                                Some(wf_core::interruption::InterruptionSignal::Pause)
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
            wf_types::events::EventType::WorkflowExecutionCompleted,
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
        let dur =
            timeout::resolve_node_timeout(&n, &StaticNodeType::Llm, None).expect("configured");
        assert_eq!(dur, std::time::Duration::from_secs(2));
    }

    #[test]
    fn node_timeout_priority_order() {
        let n = node("x", "LLM", serde_json::json!({ "timeout_seconds": 5 }));
        let dur = timeout::resolve_node_timeout(&n, &StaticNodeType::Llm, Some(1000))
            .expect("configured");
        assert_eq!(dur, std::time::Duration::from_secs(5));
        let n = node("x", "LLM", serde_json::json!({}));
        let dur = timeout::resolve_node_timeout(&n, &StaticNodeType::Llm, Some(1000))
            .expect("configured");
        assert_eq!(dur, std::time::Duration::from_millis(1000));
    }

    #[test]
    fn long_running_nodes_skip_the_engine_fallback_only() {
        let n = node("x", "AGENT_LOOP", serde_json::json!({}));
        assert_eq!(
            timeout::resolve_node_timeout(&n, &StaticNodeType::AgentLoop, None),
            None
        );
        let dur = timeout::resolve_node_timeout(&n, &StaticNodeType::AgentLoop, Some(7000))
            .expect("explicit default still applies");
        assert_eq!(dur, std::time::Duration::from_millis(7000));
        let n = node("x", "LLM", serde_json::json!({}));
        let dur = timeout::resolve_node_timeout(&n, &StaticNodeType::Llm, None)
            .expect("fallback applies");
        assert_eq!(
            dur,
            std::time::Duration::from_millis(timeout::DEFAULT_NODE_TIMEOUT_MS)
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
            _ctx: &mut wf_execution_shared::context::NodeExecutionContext,
        ) -> wf_execution_shared::error::ExecutionSharedResult<
            wf_execution_shared::context::NodeExecutionResult,
        > {
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
            ctx: &mut wf_execution_shared::context::NodeExecutionContext,
        ) -> wf_execution_shared::error::ExecutionSharedResult<
            wf_execution_shared::context::NodeExecutionResult,
        > {
            Ok(wf_execution_shared::context::NodeExecutionResult::simple(
                ctx.input.clone(),
            ))
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
            shapes:
                std::sync::Arc<std::sync::Mutex<Vec<wf_execution_shared::context::NodeInputShape>>>,
        }

        #[async_trait]
        impl NodeHandler for CaptureHandler {
            fn node_type(&self) -> StaticNodeType {
                StaticNodeType::Variable
            }

            async fn execute(
                &self,
                ctx: &mut wf_execution_shared::context::NodeExecutionContext,
            ) -> wf_execution_shared::error::ExecutionSharedResult<
                wf_execution_shared::context::NodeExecutionResult,
            > {
                self.shapes.lock().unwrap().push(ctx.input_shape);
                Ok(wf_execution_shared::context::NodeExecutionResult::simple(
                    ctx.input.clone(),
                ))
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
        assert_eq!(
            shapes.lock().unwrap().as_slice(),
            &[wf_execution_shared::context::NodeInputShape::Single]
        );

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
        assert_eq!(shape, wf_execution_shared::context::NodeInputShape::Merged);
        assert_eq!(input, serde_json::json!({"a": {"x": 1}, "b": {"y": 2}}));
    }
}
