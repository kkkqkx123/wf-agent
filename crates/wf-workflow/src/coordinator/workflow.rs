use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use wf_common::error_chain::ErrorRecord;
use wf_common::now;
use wf_core::condition::ConditionEvaluator;
use wf_core::internal_signal::{InternalSignal, InternalSignalReceiver};
use wf_core::interruption::check_execution_interruption;
use wf_core::interruption::InterruptionSignal;
use wf_core::EventBus;
use wf_execution_shared::context::{
    ExecutorContext, NodeExecutionContext, NodeExecutionResult, NodeInputShape,
};
use wf_execution_shared::execution_state::ExecutionStateManager;
use wf_execution_shared::fork::ForkRegistry;
use wf_execution_shared::hooks::types::BaseHookDefinition;
use wf_execution_shared::types::execution_entity::IExecutionEntity;
use wf_execution_shared::types::state_manager::StateManager;
use wf_metrics::collectors::node::NodeExecutionRecord as MetricsNodeExecutionRecord;
use wf_metrics::collectors::node::NodeMetricsCollector;
use wf_types::checkpoint::NodeCheckpointConfig;
use wf_types::events::{BaseEvent, EventType};
use wf_types::node::StaticNodeType;
use wf_types::workflow_execution::WorkflowGraphStructure;

use crate::checkpoint::WorkflowCheckpointIntegration;
use crate::coordinator::NodeCoordinator;
use crate::entity::WorkflowExecutionEntity;
use crate::error::{WorkflowError, WorkflowResult};
use crate::error_analysis::workflow_error_record;
use crate::graph::GraphTraversal;
use crate::handler::NodeHandler;
use crate::hook::WorkflowHookHandler;
use crate::persistence::build_workflow_execution;
use crate::state::{NodeExecutionRecord, WorkflowExecutionStateSnapshot};
use crate::trigger_internal;

/// Serialized size of a value in bytes, used for node input/output metrics.
fn json_size(value: &Value) -> u64 {
    serde_json::to_string(value)
        .map(|s| s.len() as u64)
        .unwrap_or(0)
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
        other => Err(WorkflowError::HandlerNotFound {
            node_type: other.to_string(),
        }),
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

/// Effective retry/timeout configuration for one node execution.
/// Resolution order: node-level config > type-based default > global options.
/// Node-level config uses the canonical fields `retry_policy`, `on_failure`
/// and `fallback_output`.
#[derive(Debug, Clone)]
struct NodeRetryConfig {
    on_failure: String,
    max_retries: u32,
    retry_delay_ms: u64,
    exponential_backoff: bool,
    fallback_output: Option<Value>,
}

impl NodeRetryConfig {
    /// Resolve from global options only (nodes without specific config).
    fn from_global(options: &wf_types::workflow_execution::WorkflowExecutionOptions) -> Self {
        Self {
            on_failure: options
                .on_failure
                .clone()
                .unwrap_or_else(|| "fail".to_string()),
            max_retries: options.max_retries.unwrap_or(0),
            retry_delay_ms: options.retry_delay_ms.unwrap_or(1000),
            exponential_backoff: options.exponential_backoff.unwrap_or(true),
            fallback_output: options.fallback_output.clone(),
        }
    }

    /// Merge a node's `inner` config over the global baseline. LLM and
    /// AGENT_LOOP nodes default to retry(3) with exponential backoff when no
    /// node-level or global setting is present. A malformed
    /// `retry_policy` is a user error: it fails with a structured
    /// `ConfigError` instead of silently degrading.
    fn resolve(
        node: &wf_types::workflow_execution::WorkflowNode,
        options: &wf_types::workflow_execution::WorkflowExecutionOptions,
    ) -> WorkflowResult<Self> {
        let base = Self::from_global(options);
        let cfg = &node.inner;

        let type_default = matches!(node.node_type.as_str(), "LLM" | "AGENT_LOOP");

        let retry_policy: Option<wf_types::execution::RetryPolicy> = match cfg.get("retry_policy") {
            None => None,
            Some(v) => Some(crate::config_parse::parse_node_config(
                node.id.as_str(),
                "inner.retry_policy",
                v,
            )?),
        };

        let on_failure = cfg
            .get("on_failure")
            .and_then(|v| v.as_str())
            .unwrap_or(if type_default {
                "retry"
            } else {
                &base.on_failure
            })
            .to_string();

        let max_retries = retry_policy
            .as_ref()
            .filter(|p| p.enabled)
            .map(|p| p.max_retries)
            .unwrap_or(if type_default { 3 } else { base.max_retries });

        let retry_delay_ms = retry_policy
            .as_ref()
            .filter(|p| p.enabled)
            .map(|p| p.base_delay_ms)
            .unwrap_or(if type_default {
                1000
            } else {
                base.retry_delay_ms
            });

        let exponential_backoff = retry_policy
            .as_ref()
            .filter(|p| p.enabled)
            .and_then(|p| p.backoff_multiplier)
            .map(|m| m > 1.0)
            .unwrap_or(if type_default {
                true
            } else {
                base.exponential_backoff
            });

        let fallback_output = cfg.get("fallback_output").cloned().or(base.fallback_output);

        Ok(Self {
            on_failure,
            max_retries,
            retry_delay_ms,
            exponential_backoff,
            fallback_output,
        })
    }

    fn retry_delay(&self, attempt: u32) -> std::time::Duration {
        let base = self.retry_delay_ms;
        let delay = if self.exponential_backoff && attempt > 0 {
            base.saturating_mul(2_u64.pow(attempt.min(10)))
        } else {
            base
        };
        std::time::Duration::from_millis(delay)
    }
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
    hooks: Vec<BaseHookDefinition>,
    navigation_count: u32,
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
    /// Receiver for typed internal signals (replaces the `__`-prefixed
    /// variable protocol).
    signal_receiver: Option<InternalSignalReceiver>,
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

        let signal_receiver = ctx.signal_bus.as_ref().map(|bus| bus.subscribe());

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
            total_node_count,
            max_navigation_multiplier,
            checkpoint: None,
            state_manager: None,
            fork_branch_progress: None,
            signal_receiver,
        })
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

    pub fn with_hooks(mut self, hooks: Vec<BaseHookDefinition>) -> Self {
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

    /// Persist a structured error record for a failed node attempt, linking
    /// it to its *true parent* (the record this failure is caused by): the
    /// node's first failure for retry attempts, `None` for the initiating
    /// failure. The chain is inherited from the parent (parent chain + own
    /// id, same root cause); without a parent the record is its own root.
    async fn record_workflow_error(
        entity: &WorkflowExecutionEntity,
        error: &WorkflowError,
        node_id: &str,
        retry_attempt: u32,
        parent: Option<&ErrorRecord>,
    ) {
        let mut state = entity.state.write().await;
        let mut record = workflow_error_record(error, entity.id(), node_id, retry_attempt);
        match parent {
            Some(prev) => {
                record.parent_error_id = Some(prev.id.clone());
                let mut chain = prev.error_chain.clone();
                chain.push(record.id.clone());
                record.error_chain = chain;
                record.root_cause_id = prev.root_cause_id.clone();
            }
            None => {
                record.error_chain = vec![record.id.clone()];
                record.root_cause_id = record.id.clone();
            }
        }
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
        WorkflowHookHandler::emit_workflow_hooks(
            entity,
            &self.hooks,
            hook_type,
            HashMap::new(),
            self.ctx.hook_registry.as_deref(),
            self.ctx.event_bus.as_deref(),
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
            let retry_config = NodeRetryConfig::resolve(node, &self.ctx.options)?;

            if let Some(ref mut cp) = self.checkpoint {
                cp.on_node_before(&entity, checkpoint_config.as_ref()).await;
            }

            let mut node_ctx = self.build_node_context(node_id, &node_type).await?;

            let metrics = self.ctx.metrics.clone();
            let node_metrics = metrics.as_ref().map(|m| m.node());
            if let Some(node_metrics) = &node_metrics {
                node_metrics.record_execution_start(node_id, &node_type_str);
            }
            let node_start = wf_common::now();

            let result = self
                .execute_node_once(
                    &entity,
                    &mut node_ctx,
                    event_bus_ref,
                    node_timeout,
                    node_id,
                    node,
                    &node_type,
                )
                .await;
            let node_duration_ms = (wf_common::now() - node_start) as f64;

            match result {
                Ok(output) => {
                    self.record_node_success(
                        &entity,
                        node_id,
                        &node_type_str,
                        &node_ctx,
                        &output,
                        node_metrics.as_deref(),
                        node_start,
                        node_duration_ms,
                        checkpoint_config,
                    )
                    .await;
                    self.current_node_id = self.determine_next_node(&output).await?;
                }
                Err(e) => {
                    self.record_node_failure(
                        &entity,
                        node_id,
                        &node_type_str,
                        &node_ctx,
                        &e,
                        node_metrics.as_deref(),
                        node_start,
                        node_duration_ms,
                        checkpoint_config,
                    )
                    .await;
                    self.handle_node_retry(
                        &entity,
                        node_id,
                        &node_type,
                        &node_type_str,
                        e,
                        &retry_config,
                        node_metrics.as_deref(),
                        node_duration_ms,
                    )
                    .await?;
                }
            }

            // Trigger actions (Stop/Pause/Resume) write marker variables;
            // translate them into entity interruption so the next iteration
            // of the loop handles them through the standard path.
            self.process_trigger_effects(&entity).await;
        }

        let result = self.compute_final_output();
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
            wf_core::types::interruption::ExecutionInterruptionCheckResult::Stopped { .. } => {
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
                return Err(WorkflowError::CoordinatorError(
                    "Execution stopped by interruption".to_string(),
                ));
            }
            wf_core::types::interruption::ExecutionInterruptionCheckResult::Paused { .. } => {
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
                    let mut state = entity.state.write().await;
                    state.increment_timeout_count();
                    state.record_interruption(
                        serde_json::json!({
                            "type": "timeout",
                            "reason": "max_execution_time",
                            "max_execution_time": max_execution_time,
                            "recovered": false,
                            "timestamp": now(),
                        }),
                    );
                    state.fail("Workflow execution exceeded max_execution_time".to_string())?;
                }
                if let Some(ref mut cp) = self.checkpoint {
                    cp.on_interruption(entity).await;
                }
                return Err(WorkflowError::CoordinatorError(format!(
                    "Workflow execution exceeded max_execution_time ({}ms)",
                    max_execution_time
                )));
            }
        }
        Ok(())
    }

    /// The fork branch id this coordinator runs under, if any. Branch retries
    /// charge their own allocated slice of the shared retry budget instead of
    /// the global pool (per-branch isolation, no cross-branch races).
    fn retry_branch_id(&self) -> Option<&str> {
        self.fork_branch_progress
            .as_ref()
            .map(|(_, id)| id.as_str())
    }

    /// Loop heads re-arm the navigation backstop: every iteration passes
    /// through LOOP_START (and its iteration count is bounded by the loop
    /// state), so legitimate loops never accumulate navigations here, while
    /// cycles that never pass through an active loop head keep counting and
    /// trip the detector. Returns `Err` when the heuristic budget is
    /// exhausted (a coarse infinite-loop guard; precise loop-convergence
    /// detection is driven by `LoopState`).
    fn check_navigation_backstop(&mut self, node_id: &str) -> WorkflowResult<()> {
        if self
            .traversal
            .get_node(node_id)
            .is_some_and(|n| n.node_type == "LOOP_START")
            && !crate::loop_state::stack(&self.ctx.variables).is_empty()
        {
            self.navigation_count = 0;
        }

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
    #[allow(clippy::too_many_arguments)]
    async fn execute_node_once(
        &self,
        entity: &WorkflowExecutionEntity,
        node_ctx: &mut NodeExecutionContext,
        event_bus: Option<&EventBus>,
        node_timeout: Option<u64>,
        node_id: &str,
        node: &wf_types::workflow_execution::WorkflowNode,
        node_type: &StaticNodeType,
    ) -> WorkflowResult<NodeExecutionResult> {
        let handler =
            self.handlers
                .get(node_type)
                .ok_or_else(|| WorkflowError::HandlerNotFound {
                    node_type: node.node_type.clone(),
                })?;

        let coordinator = NodeCoordinator::new();
        let node_timeout_ms =
            node_timeout.or_else(|| node.inner.get("timeout_seconds").and_then(|v| v.as_u64()));
        let timeout_dur = node_timeout_ms.map(std::time::Duration::from_millis);

        let fut = coordinator.execute_node(
            entity,
            handler.as_ref(),
            node_ctx,
            event_bus,
            &self.hooks,
            self.ctx.hook_registry.as_deref(),
        );

        match timeout_dur {
            Some(tout_dur) => tokio::time::timeout(tout_dur, fut).await.map_err(|_| {
                WorkflowError::CoordinatorError(format!(
                    "Node '{}' timed out after {:?}",
                    node_id, tout_dur
                ))
            })?,
            None => fut.await,
        }
    }

    /// Record a successful node execution: outputs, completion state, audit
    /// record, metrics and node-level checkpoint.
    #[allow(clippy::too_many_arguments)]
    async fn record_node_success(
        &mut self,
        entity: &WorkflowExecutionEntity,
        node_id: &str,
        node_type_str: &str,
        node_ctx: &NodeExecutionContext,
        output: &NodeExecutionResult,
        node_metrics: Option<&NodeMetricsCollector>,
        node_start: i64,
        node_duration_ms: f64,
        checkpoint_config: Option<NodeCheckpointConfig>,
    ) {
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
            cp.on_node_completed(entity, checkpoint_config.as_ref())
                .await;
        }

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
    }

    /// Record a failed node execution: error chain, audit record, metrics and
    /// node-level checkpoint.
    #[allow(clippy::too_many_arguments)]
    async fn record_node_failure(
        &mut self,
        entity: &WorkflowExecutionEntity,
        node_id: &str,
        node_type_str: &str,
        node_ctx: &NodeExecutionContext,
        error: &WorkflowError,
        node_metrics: Option<&NodeMetricsCollector>,
        node_start: i64,
        node_duration_ms: f64,
        checkpoint_config: Option<NodeCheckpointConfig>,
    ) {
        self.node_errors
            .push(format!("Node {}: {}", node_id, error));

        Self::record_workflow_error(entity, error, node_id, 0, None).await;

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

    /// Retry / continue / fallback handling after a node failure. On retry
    /// success the node result is recorded and the loop continues through the
    /// retried output; on `continue`/`fallback` the failure is absorbed; on
    /// `fail` (or retries exhausted without a fallback) the error propagates.
    #[allow(clippy::too_many_arguments)]
    async fn handle_node_retry(
        &mut self,
        entity: &WorkflowExecutionEntity,
        node_id: &str,
        node_type: &StaticNodeType,
        node_type_str: &str,
        error: WorkflowError,
        retry_config: &NodeRetryConfig,
        node_metrics: Option<&NodeMetricsCollector>,
        node_duration_ms: f64,
    ) -> WorkflowResult<()> {
        let handler =
            self.handlers
                .get(node_type)
                .ok_or_else(|| WorkflowError::HandlerNotFound {
                    node_type: node_type_str.to_string(),
                })?;

        match retry_config.on_failure.as_str() {
            "retry" | "continue" | "fallback" => {
                let branch_id = self.retry_branch_id();
                let mut retried = false;
                for attempt in 0..retry_config.max_retries {
                    let retry_delay = retry_config.retry_delay(attempt);
                    if let Some(budget) = self.ctx.retry_budget.as_ref() {
                        // Branch executions consume their own allocated slice
                        // of the shared budget: concurrent branches
                        // cannot starve each other through the global pool.
                        let check =
                            budget.consume_retry(retry_delay.as_millis() as u64, branch_id, 0);
                        if !check.allowed {
                            tracing::warn!(
                                branch_id = ?branch_id,
                                "Node '{}' retry budget exhausted: {}. Stopping retries.",
                                node_id,
                                check.reason.unwrap_or_default()
                            );
                            break;
                        }
                    }
                    tracing::warn!(
                        "Node '{}' failed (attempt {}/{}): {}. Retrying in {:?}...",
                        node_id,
                        attempt + 1,
                        retry_config.max_retries,
                        error,
                        retry_delay
                    );
                    if let Some(node_metrics) = node_metrics {
                        node_metrics.record_retry(node_id, node_type_str);
                    }
                    tokio::time::sleep(retry_delay).await;
                    let attempt_start = wf_common::now();
                    let mut retry_node_ctx = self.build_node_context(node_id, node_type).await?;
                    // The shared trait boundary returns `ExecutionSharedError`;
                    // map it back into the workflow error type.
                    let retry_ok: WorkflowResult<NodeExecutionResult> = handler
                        .execute(&mut retry_node_ctx)
                        .await
                        .map_err(WorkflowError::from);
                    match retry_ok {
                        Ok(retry_output) => {
                            self.node_outputs
                                .insert(node_id.to_string(), retry_output.output.clone());
                            self.completed_nodes.push(node_id.to_string());
                            self.record_loop_iteration_completion(node_id);
                            entity
                                .set_node_result(node_id.to_string(), retry_output.output.clone());
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
                                    start_time: attempt_start,
                                    success: true,
                                    error: None,
                                    input: Some(retry_node_ctx.input.clone()),
                                    result: Some(retry_output.output.clone()),
                                    branch_id: None,
                                },
                            )
                            .await;
                            if let Some(node_metrics) = node_metrics {
                                node_metrics.record_execution(MetricsNodeExecutionRecord {
                                    node_id,
                                    node_type: node_type_str,
                                    execution_id: &self.ctx.execution_id,
                                    success: true,
                                    duration_ms: node_duration_ms,
                                    input_size: json_size(&retry_node_ctx.input),
                                    output_size: json_size(&retry_output.output),
                                    error_type: None,
                                });
                            }
                            self.current_node_id = self.determine_next_node(&retry_output).await?;
                            retried = true;
                            break;
                        }
                        Err(retry_err) => {
                            // The true parent is the node's first failure
                            // record (the failure that started this retry
                            // sequence) — not the previously recorded entry.
                            let parent = {
                                let state = entity.state.read().await;
                                state
                                    .error_records()
                                    .iter()
                                    .find(|r| r.node_id.as_deref() == Some(node_id))
                                    .cloned()
                            };
                            Self::record_workflow_error(
                                entity,
                                &retry_err,
                                node_id,
                                attempt + 1,
                                parent.as_ref(),
                            )
                            .await;
                            self.record_node_execution(
                                entity,
                                ExecutionAttempt {
                                    node_id,
                                    node_type: node_type_str,
                                    start_time: attempt_start,
                                    success: false,
                                    error: Some(retry_err.to_string()),
                                    input: Some(retry_node_ctx.input.clone()),
                                    result: None,
                                    branch_id: None,
                                },
                            )
                            .await;
                        }
                    }
                }
                if !retried {
                    if retry_config.on_failure == "continue"
                        || retry_config.on_failure == "fallback"
                    {
                        // The failure is absorbed; if a loop is active, record
                        // it so the loop failure strategy can react at LOOP_END.
                        crate::loop_state::mark_iteration_failed(&self.ctx.variables);
                        if let Some(ref fallback) = retry_config.fallback_output {
                            tracing::warn!(
                                "Node '{}' failed after {} retries, using fallback_output",
                                node_id,
                                retry_config.max_retries
                            );
                            self.node_outputs
                                .insert(node_id.to_string(), fallback.clone());
                            self.completed_nodes.push(node_id.to_string());
                            self.record_loop_iteration_completion(node_id);
                            entity.set_node_result(node_id.to_string(), fallback.clone());
                            entity
                                .state
                                .write()
                                .await
                                .mark_node_completed(node_id.to_string());
                        } else {
                            tracing::warn!(
                                "Node '{}' failed after {} retries, continuing",
                                node_id,
                                retry_config.max_retries
                            );
                        }
                        self.current_node_id = self.determine_next_node_without_output().await?;
                    } else {
                        return Err(error);
                    }
                }
            }
            _ => {
                return Err(error);
            }
        }
        Ok(())
    }

    async fn process_trigger_effects(&mut self, entity: &WorkflowExecutionEntity) {
        // Legacy variable protocol (backward compatible).
        let stop = trigger_internal::read_flag(&self.ctx.variables, trigger_internal::TRIGGER_STOP);
        if stop {
            trigger_internal::clear_flag(&self.ctx.variables, trigger_internal::TRIGGER_STOP);
            let _ = entity.interruption().stop();
            return;
        }

        let pause =
            trigger_internal::read_flag(&self.ctx.variables, trigger_internal::TRIGGER_PAUSE);
        if pause {
            trigger_internal::clear_flag(&self.ctx.variables, trigger_internal::TRIGGER_PAUSE);
            let _ = entity.interruption().pause();
        }

        // Typed signal bus (new code path). Check signals that target this
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
                        // Store skip marker in variables for node handlers
                        // to check at execution time.
                        trigger_internal::set_flag(
                            &self.ctx.variables,
                            &trigger_internal::skip_marker(&node_id),
                        );
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
                    _ => continue,
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
        ctx.hook_registry = self.ctx.hook_registry.clone();
        ctx.tool_approval_handler = self.ctx.tool_approval_handler.clone();
        ctx.tool_approval_options = self.ctx.tool_approval_options.clone();
        ctx.fork_registries = self.ctx.fork_registries.clone();
        ctx.retry_budget = self.ctx.retry_budget.clone();
        ctx.signal_bus = self.ctx.signal_bus.clone();

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
                    Ok(false) => continue,
                    Err(_) => continue,
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
    use std::sync::atomic::{AtomicUsize, Ordering};
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
        }
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
            retry_budget: None,
            on_failure: None,
            max_retries: None,
            retry_delay_ms: None,
            exponential_backoff: None,
            fallback_output: None,
            max_navigation_multiplier: None,
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

    #[test]
    fn llm_defaults_to_retry_three() {
        let cfg = NodeRetryConfig::resolve(&node("llm1", "LLM", Value::Null), &options()).unwrap();
        assert_eq!(cfg.max_retries, 3);
        assert_eq!(cfg.on_failure, "retry");
        assert!(cfg.exponential_backoff);

        let agent =
            NodeRetryConfig::resolve(&node("ag1", "AGENT_LOOP", Value::Null), &options()).unwrap();
        assert_eq!(agent.max_retries, 3);
    }

    #[test]
    fn node_config_overrides_global_and_type_defaults() {
        let mut opts = options();
        opts.max_retries = Some(1);
        opts.on_failure = Some("fail".to_string());
        let n = node(
            "llm1",
            "LLM",
            serde_json::json!({
                "retry_policy": {
                    "enabled": true,
                    "max_retries": 5,
                    "base_delay_ms": 250,
                    "backoff_multiplier": 1.0
                },
                "on_failure": "continue"
            }),
        );
        let cfg = NodeRetryConfig::resolve(&n, &opts).unwrap();
        assert_eq!(cfg.max_retries, 5);
        assert_eq!(cfg.retry_delay_ms, 250);
        assert!(!cfg.exponential_backoff);
        assert_eq!(cfg.on_failure, "continue");
    }

    #[test]
    fn global_options_baseline_for_other_types() {
        let mut opts = options();
        opts.max_retries = Some(2);
        opts.on_failure = Some("continue".to_string());
        let cfg = NodeRetryConfig::resolve(&node("v1", "VARIABLE", Value::Null), &opts).unwrap();
        assert_eq!(cfg.max_retries, 2);
        assert_eq!(cfg.on_failure, "continue");
    }

    #[test]
    fn fallback_output_prefers_node_level() {
        let mut opts = options();
        opts.fallback_output = Some(Value::String("global".to_string()));
        let n = node(
            "s1",
            "SCRIPT",
            serde_json::json!({"fallback_output": {"safe": true}}),
        );
        let cfg = NodeRetryConfig::resolve(&n, &opts).unwrap();
        assert_eq!(cfg.fallback_output, Some(serde_json::json!({"safe": true})));
    }

    #[test]
    fn retry_delay_uses_exponential_backoff() {
        let cfg = NodeRetryConfig {
            on_failure: "retry".to_string(),
            max_retries: 3,
            retry_delay_ms: 100,
            exponential_backoff: true,
            fallback_output: None,
        };
        assert_eq!(cfg.retry_delay(0), std::time::Duration::from_millis(100));
        assert_eq!(cfg.retry_delay(1), std::time::Duration::from_millis(200));
        assert_eq!(cfg.retry_delay(2), std::time::Duration::from_millis(400));
    }

    struct FlakyHandler {
        failures: Arc<AtomicUsize>,
        fail_count: usize,
        label: &'static str,
    }

    #[async_trait]
    impl NodeHandler for FlakyHandler {
        fn node_type(&self) -> StaticNodeType {
            StaticNodeType::Variable
        }

        async fn execute(
            &self,
            _ctx: &mut NodeExecutionContext,
        ) -> wf_execution_shared::error::ExecutionSharedResult<NodeExecutionResult> {
            if self.failures.load(Ordering::SeqCst) < self.fail_count {
                self.failures.fetch_add(1, Ordering::SeqCst);
                return Err(
                    WorkflowError::OperationError(format!("{} failure", self.label)).into(),
                );
            }
            Ok(NodeExecutionResult::simple(Value::String(
                self.label.to_string(),
            )))
        }
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

    #[tokio::test]
    async fn retry_recovers_after_transient_failures() {
        let handlers = base_handlers(vec![(
            StaticNodeType::Variable,
            Box::new(FlakyHandler {
                failures: Arc::new(AtomicUsize::new(0)),
                fail_count: 2,
                label: "recovered",
            }) as Box<dyn NodeHandler>,
        )]);
        let g = graph(vec![
            node("start", "START", Value::Null),
            node(
                "flaky",
                "VARIABLE",
                serde_json::json!({
                    "on_failure": "retry",
                    "retry_policy": {
                        "enabled": true,
                        "max_retries": 3,
                        "base_delay_ms": 1
                    }
                }),
            ),
            node("end", "END", Value::Null),
        ]);
        let result = run(g, options(), handlers).await;
        assert!(result.is_ok(), "retry should recover: {:?}", result.err());
        assert_eq!(result.unwrap(), Value::String("recovered".to_string()));
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
    async fn fallback_output_used_when_retries_exhausted() {
        let handlers = base_handlers(vec![(
            StaticNodeType::Variable,
            Box::new(AlwaysFailingHandler) as Box<dyn NodeHandler>,
        )]);
        let g = graph(vec![
            node("start", "START", Value::Null),
            node(
                "flaky",
                "VARIABLE",
                serde_json::json!({
                    "on_failure": "continue",
                    "retry_policy": {
                        "enabled": true,
                        "max_retries": 2,
                        "base_delay_ms": 1
                    },
                    "fallback_output": {"fallback": true}
                }),
            ),
            node("end", "END", Value::Null),
        ]);
        let result = run(g, options(), handlers).await;
        assert!(
            result.is_ok(),
            "fallback should let the workflow continue: {:?}",
            result.err()
        );
        assert_eq!(result.unwrap(), serde_json::json!({"fallback": true}));
    }

    #[tokio::test]
    async fn failure_without_fallback_propagates() {
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
            node(
                "flaky",
                "VARIABLE",
                serde_json::json!({
                    "on_failure": "retry",
                    "retry_policy": {
                        "enabled": true,
                        "max_retries": 1,
                        "base_delay_ms": 1
                    }
                }),
            ),
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
        assert_eq!(records.len(), 2, "initial failure + retry failure");
        assert_eq!(records[0].node_id.as_deref(), Some("flaky"));
        assert_eq!(records[1].node_id.as_deref(), Some("flaky"));
        assert!(records[1].error.contains("retry attempt 1"));
        assert!(records[0].error_type.is_some());
        assert!(matches!(
            records[0].caused_by,
            Some(ref cause) if cause.handling_attempt.as_deref() == Some("retry_0")
        ));
        assert_eq!(
            records[1].parent_error_id.as_deref(),
            Some(records[0].id.as_str())
        );
        assert_eq!(records[1].root_cause_id, records[0].root_cause_id);
        assert_eq!(
            records[1].error_chain,
            vec![records[0].id.clone(), records[1].id.clone()]
        );
    }

    #[tokio::test]
    async fn error_chain_parents_are_per_node_not_global() {
        // Two nodes fail in sequence, each with one retry: every record must
        // chain to its own node's first failure — never to another node's
        // records (parent is the true parent, not "the previous entry").
        let handlers = base_handlers(vec![(
            StaticNodeType::Variable,
            Box::new(AlwaysFailingHandler) as Box<dyn NodeHandler>,
        )]);
        let g = graph(vec![
            node("start", "START", Value::Null),
            node(
                "a",
                "VARIABLE",
                serde_json::json!({
                    "on_failure": "continue",
                    "retry_policy": {
                        "enabled": true,
                        "max_retries": 1,
                        "base_delay_ms": 1
                    }
                }),
            ),
            node(
                "b",
                "VARIABLE",
                serde_json::json!({
                    "on_failure": "continue",
                    "retry_policy": {
                        "enabled": true,
                        "max_retries": 1,
                        "base_delay_ms": 1
                    }
                }),
            ),
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
        assert!(coordinator.execute().await.is_ok());

        let entity = coordinator.entity.as_ref().unwrap();
        let state = entity.state.read().await;
        let records = state.error_records();
        // Records per node: [a.0, a.retry1, b.0, b.retry1]
        assert_eq!(records.len(), 4);
        assert_eq!(records[0].node_id.as_deref(), Some("a"));
        assert_eq!(records[1].node_id.as_deref(), Some("a"));
        assert_eq!(records[2].node_id.as_deref(), Some("b"));
        assert_eq!(records[3].node_id.as_deref(), Some("b"));

        // a.retry1 parents to a.0; b.0 starts its own chain; b.retry1
        // parents to b.0 — never to the "previous record" (a.retry1).
        assert_eq!(
            records[1].parent_error_id.as_deref(),
            Some(records[0].id.as_str())
        );
        assert!(records[2].parent_error_id.is_none());
        assert_eq!(
            records[3].parent_error_id.as_deref(),
            Some(records[2].id.as_str())
        );
        assert_eq!(records[2].root_cause_id, records[2].id);
        assert_eq!(records[3].root_cause_id, records[2].root_cause_id);
        assert_eq!(records[0].root_cause_id, records[0].id);
        assert_eq!(
            records[1].error_chain,
            vec![records[0].id.clone(), records[1].id.clone()]
        );
        assert_eq!(
            records[3].error_chain,
            vec![records[2].id.clone(), records[3].id.clone()]
        );
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
    async fn invalid_retry_policy_fails_with_structured_error() {
        let handlers = base_handlers(vec![(
            StaticNodeType::Variable,
            Box::new(AlwaysFailingHandler) as Box<dyn NodeHandler>,
        )]);
        let g = graph(vec![
            node("start", "START", Value::Null),
            node(
                "flaky",
                "VARIABLE",
                serde_json::json!({
                    "retry_policy": {"enabled": "not-a-bool", "max_retries": "x"},
                }),
            ),
            node("end", "END", Value::Null),
        ]);
        let err = run(g, options(), handlers)
            .await
            .expect_err("invalid retry_policy must fail");
        let text = err.to_string();
        assert!(
            text.contains("Config error")
                && text.contains("flaky")
                && text.contains("retry_policy"),
            "error must carry node id and field path: {text}"
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
