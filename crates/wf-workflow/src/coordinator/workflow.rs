use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use wf_common::now;
use wf_core::condition::ConditionEvaluator;
use wf_core::interruption::check_execution_interruption;
use wf_core::EventBus;
use wf_execution_shared::context::{ExecutorContext, NodeExecutionContext, NodeExecutionResult};
use wf_execution_shared::hooks::executor::HookExecutor;
use wf_execution_shared::hooks::types::BaseHookDefinition;
use wf_execution_shared::types::state_manager::StateManager;
use wf_metrics::collectors::node::NodeExecutionRecord as MetricsNodeExecutionRecord;
use wf_types::events::{BaseEvent, EventType};
use wf_types::node::StaticNodeType;
use wf_types::workflow_execution::WorkflowGraphStructure;

use crate::checkpoint::WorkflowCheckpointIntegration;
use crate::coordinator::NodeCoordinator;
use crate::entity::WorkflowExecutionEntity;
use crate::error::{WorkflowError, WorkflowResult};
use crate::graph::GraphTraversal;
use crate::handler::NodeHandler;
use crate::state::{NodeExecutionRecord, WorkflowExecutionStateSnapshot};

/// Serialized size of a value in bytes, used for node input/output metrics.
fn json_size(value: &Value) -> u64 {
    serde_json::to_string(value)
        .map(|s| s.len() as u64)
        .unwrap_or(0)
}

fn parse_node_type(node_type_str: &str) -> WorkflowResult<StaticNodeType> {
    match node_type_str {
        "START" => Ok(StaticNodeType::Start),
        "END" => Ok(StaticNodeType::End),
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
        "START_FROM_TRIGGER" => Ok(StaticNodeType::StartFromTrigger),
        "CONTINUE_FROM_TRIGGER" => Ok(StaticNodeType::ContinueFromTrigger),
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
}

pub struct WorkflowCoordinator {
    ctx: ExecutorContext,
    entity: Option<WorkflowExecutionEntity>,
    traversal: GraphTraversal,
    handlers: Arc<HashMap<StaticNodeType, Arc<dyn NodeHandler>>>,
    current_node_id: Option<String>,
    completed_nodes: Vec<String>,
    node_outputs: HashMap<String, Value>,
    node_errors: Vec<String>,
    start_time: i64,
    hooks: Vec<BaseHookDefinition>,
    hook_executor: Option<Arc<HookExecutor>>,
    navigation_count: u32,
    total_node_count: u32,
    max_navigation_multiplier: u32,
    checkpoint: Option<WorkflowCheckpointIntegration>,
}

impl WorkflowCoordinator {
    pub fn new(
        ctx: ExecutorContext,
        graph: WorkflowGraphStructure,
        handlers: Arc<HashMap<StaticNodeType, Arc<dyn NodeHandler>>>,
    ) -> WorkflowResult<Self> {
        let traversal = GraphTraversal::new(graph)?;
        let start_node_id = traversal
            .start_node_id()
            .ok_or_else(|| WorkflowError::GraphError("Start node not found".to_string()))?
            .to_string();

        let total_node_count = traversal.node_count() as u32;

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
            hook_executor: None,
            navigation_count: 0,
            total_node_count,
            max_navigation_multiplier: 5,
            checkpoint: None,
        })
    }

    pub fn with_entity(mut self, entity: WorkflowExecutionEntity) -> Self {
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

    pub fn with_hook_executor(mut self, hook_executor: Arc<HookExecutor>) -> Self {
        self.hook_executor = Some(hook_executor);
        self
    }

    pub fn with_checkpoint(mut self, checkpoint: WorkflowCheckpointIntegration) -> Self {
        self.checkpoint = Some(checkpoint);
        self
    }

    pub fn completed_nodes(&self) -> &[String] {
        &self.completed_nodes
    }

    /// Resume from a restored checkpoint snapshot: seed completed node
    /// outputs and restart at the checkpointed node. Completed nodes are
    /// skipped by the main loop; their outputs feed the edges downstream.
    pub fn resume_from(&mut self, snapshot: &wf_types::checkpoint::workflow::WorkflowExecutionStateSnapshot) {
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
            });
    }

    pub async fn execute(&mut self) -> WorkflowResult<Value> {
        let entity = self.entity.as_ref().ok_or_else(|| {
            WorkflowError::CoordinatorError("Entity not set on WorkflowCoordinator".to_string())
        })?;

        let event_bus: Option<&EventBus> = self.ctx.event_bus.as_deref();

        self.emit_event(
            event_bus,
            EventType::WorkflowExecutionStarted,
            entity,
            &serde_json::json!({
                "workflow_id": self.ctx.workflow_id,
            }),
        )
        .await;

        if let Some(ref mut cp) = self.checkpoint {
            cp.on_workflow_start(entity).await;
        }

        let node_timeout = self.ctx.options.node_timeout;

        while let Some(node_id) = &self.current_node_id.clone() {
            let interruption_check = check_execution_interruption(entity.interruption(), None);
            match interruption_check {
                wf_core::types::interruption::ExecutionInterruptionCheckResult::Stopped {
                    ..
                } => {
                    self.emit_event(
                        event_bus,
                        EventType::WorkflowExecutionCancelled,
                        entity,
                        &serde_json::json!({
                            "reason": "interrupted",
                        }),
                    )
                    .await;
                    return Err(WorkflowError::CoordinatorError(
                        "Execution stopped by interruption".to_string(),
                    ));
                }
                wf_core::types::interruption::ExecutionInterruptionCheckResult::Paused {
                    ..
                } => {
                    self.emit_event(
                        event_bus,
                        EventType::WorkflowExecutionPaused,
                        entity,
                        &serde_json::json!({
                            "node_id": node_id,
                        }),
                    )
                    .await;
                    return Err(WorkflowError::CoordinatorError(
                        "Execution paused".to_string(),
                    ));
                }
                _ => {}
            }

            if self
                .ctx
                .options
                .max_steps
                .is_some_and(|max| self.completed_nodes.len() as u32 >= max)
            {
                break;
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

            if self.completed_nodes.contains(node_id) {
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

            let mut node_ctx = self.build_node_context(node_id, &node_type).await?;

            let handler =
                self.handlers
                    .get(&node_type)
                    .ok_or_else(|| WorkflowError::HandlerNotFound {
                        node_type: node.node_type.clone(),
                    })?;

            let coordinator = NodeCoordinator::new();
            let node_timeout_ms =
                node_timeout.or_else(|| node.inner.get("timeout").and_then(|v| v.as_u64()));
            let timeout_dur = node_timeout_ms.map(std::time::Duration::from_millis);

            let metrics = self.ctx.metrics.clone();
            let node_metrics = metrics.as_ref().map(|m| m.node());
            if let Some(node_metrics) = &node_metrics {
                node_metrics.record_execution_start(node_id, &node_type_str);
            }
            let node_start = wf_common::now();

            let result = if let Some(tout_dur) = timeout_dur {
                let fut = coordinator.execute_node(
                    entity,
                    handler.as_ref(),
                    &mut node_ctx,
                    event_bus,
                    &self.hooks,
                    self.hook_executor.as_deref(),
                );
                tokio::time::timeout(tout_dur, fut).await.map_err(|_| {
                    WorkflowError::CoordinatorError(format!(
                        "Node '{}' timed out after {:?}",
                        node_id, tout_dur
                    ))
                })?
            } else {
                coordinator
                    .execute_node(
                        entity,
                        handler.as_ref(),
                        &mut node_ctx,
                        event_bus,
                        &self.hooks,
                        self.hook_executor.as_deref(),
                    )
                    .await
            };
            let node_duration_ms = (wf_common::now() - node_start) as f64;

            let on_failure = self.ctx.options.on_failure.as_deref().unwrap_or("fail");
            let max_retries = self.ctx.options.max_retries.unwrap_or(0);
            let retry_delay =
                std::time::Duration::from_millis(self.ctx.options.retry_delay_ms.unwrap_or(1000));

            match result {
                Ok(output) => {
                    self.node_outputs
                        .insert(node_id.clone(), output.output.clone());
                    self.completed_nodes.push(node_id.clone());
                    entity.set_node_result(node_id.clone(), output.output.clone());

                    for (k, v) in &output.metadata {
                        self.ctx.variables.insert(k.clone(), v.clone());
                    }

                    entity
                        .state
                        .write()
                        .await
                        .mark_node_completed(node_id.clone());

                    self.record_node_execution(
                        entity,
                        ExecutionAttempt {
                            node_id,
                            node_type: &node_type_str,
                            start_time: node_start,
                            success: true,
                            error: None,
                        },
                    )
                    .await;

                    if let Some(ref mut cp) = self.checkpoint {
                        cp.on_node_completed(entity).await;
                    }

                    if let Some(node_metrics) = &node_metrics {
                        node_metrics.record_execution(MetricsNodeExecutionRecord {
                            node_id,
                            node_type: &node_type_str,
                            execution_id: &self.ctx.execution_id,
                            success: true,
                            duration_ms: node_duration_ms,
                            input_size: json_size(&node_ctx.input),
                            output_size: json_size(&output.output),
                            error_type: None,
                        });
                    }

                    self.current_node_id = self.determine_next_node(&output).await?;
                }
                Err(e) => {
                    self.node_errors.push(format!("Node {}: {}", node_id, e));

                    self.record_node_execution(
                        entity,
                        ExecutionAttempt {
                            node_id,
                            node_type: &node_type_str,
                            start_time: node_start,
                            success: false,
                            error: Some(e.to_string()),
                        },
                    )
                    .await;

                    if let Some(ref mut cp) = self.checkpoint {
                        cp.on_node_failed(entity).await;
                    }

                    if let Some(node_metrics) = &node_metrics {
                        node_metrics.record_execution(MetricsNodeExecutionRecord {
                            node_id,
                            node_type: &node_type_str,
                            execution_id: &self.ctx.execution_id,
                            success: false,
                            duration_ms: node_duration_ms,
                            input_size: json_size(&node_ctx.input),
                            output_size: 0,
                            error_type: Some("node_failed"),
                        });
                    }

                    match on_failure {
                        "retry" | "continue" => {
                            let mut retried = false;
                            for attempt in 0..max_retries {
                                tracing::warn!(
                                    "Node '{}' failed (attempt {}/{}): {}. Retrying in {:?}...",
                                    node_id,
                                    attempt + 1,
                                    max_retries,
                                    e,
                                    retry_delay
                                );
                                if let Some(node_metrics) = &node_metrics {
                                    node_metrics.record_retry(node_id, &node_type_str);
                                }
                                tokio::time::sleep(retry_delay).await;
                                let attempt_start = wf_common::now();
                                let mut retry_node_ctx =
                                    self.build_node_context(node_id, &node_type).await?;
                                let retry_ok = handler.execute(&mut retry_node_ctx).await;
                                match retry_ok {
                                    Ok(retry_output) => {
                                        self.node_outputs
                                            .insert(node_id.clone(), retry_output.output.clone());
                                        self.completed_nodes.push(node_id.clone());
                                        entity
                                            .set_node_result(node_id.clone(), retry_output.output.clone());
                                        entity
                                            .state
                                            .write()
                                            .await
                                            .mark_node_completed(node_id.clone());
                                        self.record_node_execution(
                                            entity,
                                            ExecutionAttempt {
                                                node_id,
                                                node_type: &node_type_str,
                                                start_time: attempt_start,
                                                success: true,
                                                error: None,
                                            },
                                        )
                                        .await;
                                        if let Some(node_metrics) = &node_metrics {
                                            node_metrics.record_execution(
                                                MetricsNodeExecutionRecord {
                                                    node_id,
                                                    node_type: &node_type_str,
                                                    execution_id: &self.ctx.execution_id,
                                                    success: true,
                                                    duration_ms: node_duration_ms,
                                                    input_size: json_size(&retry_node_ctx.input),
                                                    output_size: json_size(&retry_output.output),
                                                    error_type: None,
                                                },
                                            );
                                        }
                                        self.current_node_id =
                                            self.determine_next_node(&retry_output).await?;
                                        retried = true;
                                        break;
                                    }
                                    Err(retry_err) => {
                                        self.record_node_execution(
                                            entity,
                                            ExecutionAttempt {
                                                node_id,
                                                node_type: &node_type_str,
                                                start_time: attempt_start,
                                                success: false,
                                                error: Some(retry_err.to_string()),
                                            },
                                        )
                                        .await;
                                    }
                                }
                            }
                            if !retried {
                                if on_failure == "continue" {
                                    tracing::warn!(
                                        "Node '{}' failed after {} retries, continuing",
                                        node_id,
                                        max_retries
                                    );
                                    self.current_node_id =
                                        self.determine_next_node_without_output().await?;
                                } else {
                                    return Err(e);
                                }
                            }
                        }
                        "skip" | "skipped" => {
                            tracing::warn!("Skipping failed node '{}': {}", node_id, e);
                            entity
                                .state
                                .write()
                                .await
                                .mark_node_completed(node_id.clone());
                            self.current_node_id =
                                self.determine_next_node_without_output().await?;
                        }
                        _ => {
                            return Err(e);
                        }
                    }
                }
            }

            // Trigger actions (Stop/Pause/Resume) write marker variables;
            // translate them into entity interruption so the next iteration
            // of the loop handles them through the standard path.
            self.process_trigger_effects(entity).await;
        }

        let result = self.compute_final_output();
        let execution_time = now() - self.start_time;

        entity.state.write().await.complete();

        self.emit_event(
            event_bus,
            EventType::WorkflowExecutionCompleted,
            entity,
            &serde_json::json!({
                "execution_time": execution_time,
                "node_count": self.completed_nodes.len(),
            }),
        )
        .await;

        if let Some(ref mut cp) = self.checkpoint {
            cp.on_workflow_end(entity).await;
        }

        Ok(result)
    }

    async fn process_trigger_effects(&self, entity: &WorkflowExecutionEntity) {
        let stop = self
            .ctx
            .variables
            .get("__trigger_stop")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if stop {
            self.ctx.variables.remove("__trigger_stop");
            let _ = entity.interruption().stop();
            return;
        }

        let pause = self
            .ctx
            .variables
            .get("__trigger_pause")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if pause {
            self.ctx.variables.remove("__trigger_pause");
            let _ = entity.interruption().pause();
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
        let input = self.compute_node_input(node_id);

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
        ctx.metrics = self.ctx.metrics.clone();

        Ok(ctx)
    }

    fn compute_node_input(&self, node_id: &str) -> Value {
        let incoming_edges = self.traversal.get_incoming_edges(node_id);

        if incoming_edges.is_empty() {
            return self.ctx.options.input.clone().unwrap_or(Value::Null);
        }

        let mut inputs = serde_json::Map::new();
        for edge in incoming_edges {
            if let Some(output) = self.node_outputs.get(&edge.source_node_id) {
                let key = edge.label.as_deref().unwrap_or(&edge.source_node_id);
                inputs.insert(key.to_string(), output.clone());
            }
        }

        if inputs.len() == 1 {
            inputs.values().next().cloned().unwrap_or(Value::Null)
        } else {
            Value::Object(inputs)
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
        let Some(bus) = event_bus else { return };
        let metadata = data.as_object().map(|obj| {
            obj.iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<HashMap<_, _>>()
        });

        let event = BaseEvent {
            id: wf_types::Id::new(),
            r#type: event_type,
            timestamp: now(),
            workflow_id: Some(entity.workflow_id().clone()),
            execution_id: Some(entity.id().clone()),
            agent_loop_id: None,
            metadata,
        };
        let _ = bus.publish(event);
    }
}

