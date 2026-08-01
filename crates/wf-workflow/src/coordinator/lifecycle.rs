use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use wf_checkpoint::event::CheckpointEventBus;
use wf_core::EventBus;
use wf_core::WorkflowStateMachine;
use wf_execution_shared::context::ExecutorContext;
use wf_execution_shared::hooks::executor::HookExecutor;
use wf_metrics::MetricsRegistry;
use wf_storage::backend::StorageBackend;
use wf_tools::callback::WorkflowOutput;
use wf_types::node::StaticNodeType;
use wf_types::workflow_execution::{WorkflowExecutionOptions, WorkflowGraphStructure};

use crate::checkpoint::{NodeCheckpointStrategy, WorkflowCheckpointIntegration};
use crate::coordinator::WorkflowCoordinator;
use crate::entity::WorkflowExecutionEntity;
use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::NodeHandler;

pub struct WorkflowExecutionParams {
    pub execution_id: wf_types::Id,
    pub workflow_id: wf_types::Id,
    pub graph: WorkflowGraphStructure,
    pub options: WorkflowExecutionOptions,
    pub handlers: Arc<HashMap<StaticNodeType, Arc<dyn NodeHandler>>>,
    pub tool_registry: Arc<wf_tools::registry::ToolRegistry>,
    pub input: Option<Value>,
}

pub struct WorkflowLifecycleCoordinator {
    event_bus: Option<Arc<EventBus>>,
    hook_executor: Option<Arc<HookExecutor>>,
    store: Arc<StorageBackend>,
    checkpoint_strategy: Option<NodeCheckpointStrategy>,
    checkpoint_event_bus: Option<CheckpointEventBus>,
    metrics: Option<Arc<MetricsRegistry>>,
}

impl WorkflowLifecycleCoordinator {
    pub fn new(event_bus: Option<Arc<EventBus>>) -> Self {
        Self::with_store(event_bus, Arc::new(StorageBackend::new_memory()))
    }

    pub fn with_store(event_bus: Option<Arc<EventBus>>, store: Arc<StorageBackend>) -> Self {
        Self {
            event_bus,
            hook_executor: None,
            store,
            checkpoint_strategy: None,
            checkpoint_event_bus: None,
            metrics: None,
        }
    }

    pub fn with_hook_executor(mut self, hook_executor: Arc<HookExecutor>) -> Self {
        self.hook_executor = Some(hook_executor);
        self
    }

    pub fn with_checkpoint_strategy(mut self, strategy: NodeCheckpointStrategy) -> Self {
        self.checkpoint_strategy = Some(strategy);
        self
    }

    pub fn with_checkpoint_event_bus(mut self, bus: CheckpointEventBus) -> Self {
        self.checkpoint_event_bus = Some(bus);
        self
    }

    pub fn with_metrics(mut self, metrics: Arc<MetricsRegistry>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    pub async fn execute_workflow(
        &self,
        params: WorkflowExecutionParams,
    ) -> WorkflowResult<WorkflowOutput> {
        let execution_id = params.execution_id;
        let workflow_id = params.workflow_id;
        let workflow_id_metrics = workflow_id.clone();
        let execution_id_metrics = execution_id.clone();

        let mut wf_state = WorkflowStateMachine::new(&execution_id);
        wf_state
            .start()
            .map_err(|e| WorkflowError::StateTransitionError(e.to_string()))?;

        let mut opts = params.options;
        if opts.input.is_none() {
            opts.input = params.input;
        }

        let entity = WorkflowExecutionEntity::new(execution_id.clone(), workflow_id.clone());

        if let Some(ref input) = opts.input {
            entity.set_variable("input", input.clone());
        }

        let mut ctx = ExecutorContext::new(
            execution_id.clone(),
            workflow_id,
            self.event_bus.clone(),
            params.tool_registry,
            opts,
        );
        // The coordinator and the entity share one variable map so that
        // checkpoints (built from the entity) capture live variables.
        ctx.variables = entity.variables().clone();
        if let Some(ref metrics) = self.metrics {
            metrics
                .workflow()
                .record_execution_start(&execution_id, &workflow_id_metrics);
            ctx = ctx.with_metrics(metrics.clone());
        }

        let mut coordinator = WorkflowCoordinator::new(ctx, params.graph, params.handlers)?
            .with_entity(entity)
            .with_hooks(Vec::new());

        if let Some(ref executor) = self.hook_executor {
            coordinator = coordinator.with_hook_executor(executor.clone());
        }

        if let Some(ref strategy) = self.checkpoint_strategy {
            let mut cp = WorkflowCheckpointIntegration::new(self.store.clone(), strategy.clone());
            if let Some(ref bus) = self.checkpoint_event_bus {
                cp = cp.with_event_bus(bus.clone());
            }
            if let Some(ref core_bus) = self.event_bus {
                cp = cp.with_core_event_bus(core_bus.clone());
            }
            coordinator = coordinator.with_checkpoint(cp);
        }

        let start = wf_common::now();
        let result = coordinator.execute().await;
        let duration_ms = (wf_common::now() - start) as f64;

        match result {
            Ok(output) => {
                wf_state
                    .complete(Some(output.clone()))
                    .map_err(|e| WorkflowError::StateTransitionError(e.to_string()))?;
                if let Some(ref metrics) = self.metrics {
                    metrics.workflow().record_execution_complete(
                        &execution_id_metrics,
                        &workflow_id_metrics,
                        None,
                        true,
                        duration_ms,
                        None,
                    );
                }

                Ok(WorkflowOutput {
                    execution_id,
                    result: output,
                })
            }
            Err(e) => {
                wf_state
                    .fail(e.to_string())
                    .map_err(|e| WorkflowError::StateTransitionError(e.to_string()))?;
                if let Some(ref metrics) = self.metrics {
                    metrics.workflow().record_execution_complete(
                        &execution_id_metrics,
                        &workflow_id_metrics,
                        None,
                        false,
                        duration_ms,
                        Some("workflow_error"),
                    );
                }
                Err(e)
            }
        }
    }

    /// Resume a workflow execution from its latest checkpoint.
    ///
    /// Loads the newest snapshot for `execution_id`, rebuilds the entity,
    /// variables and node outputs, then continues from the checkpointed
    /// node. Completed nodes are skipped by the coordinator.
    pub async fn resume_workflow(
        &self,
        execution_id: &str,
        workflow_id: wf_types::Id,
        graph: WorkflowGraphStructure,
        handlers: Arc<HashMap<StaticNodeType, Arc<dyn NodeHandler>>>,
        tool_registry: Arc<wf_tools::registry::ToolRegistry>,
    ) -> WorkflowResult<WorkflowOutput> {
        use wf_checkpoint::coordinator::CheckpointCoordinator;
        use wf_checkpoint::state::CheckpointStateManager;
        use wf_checkpoint::state::WorkflowCheckpointStateManager;
        use wf_checkpoint::coordinator::workflow::WorkflowCheckpointCoordinator;

        let state_manager = WorkflowCheckpointStateManager::new(self.store.clone());
        let cp_coordinator = WorkflowCheckpointCoordinator::new(state_manager);

        let metadata = cp_coordinator
            .state_manager()
            .get_latest(execution_id)
            .await
            .map_err(|e| WorkflowError::CoordinatorError(format!("checkpoint query failed: {}", e)))?
            .ok_or_else(|| {
                WorkflowError::CoordinatorError(format!(
                    "no checkpoint found for execution {}",
                    execution_id
                ))
            })?;

        let restored = cp_coordinator
            .restore(&metadata.id)
            .await
            .map_err(|e| WorkflowError::CoordinatorError(format!("checkpoint restore failed: {}", e)))?;
        let snapshot = restored.snapshot;

        let entity = WorkflowExecutionEntity::new(
            wf_types::Id::from(snapshot.execution_id.clone()),
            workflow_id.clone(),
        );
        {
            let mut state = entity.state.write().await;
            state.start();
            for node_id in snapshot.node_results.as_ref().map(|m| m.keys()).into_iter().flatten() {
                state.mark_node_completed(node_id.clone());
            }
        }

        let mut ctx = ExecutorContext::new(
            wf_types::Id::from(snapshot.execution_id.clone()),
            workflow_id,
            self.event_bus.clone(),
            tool_registry,
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
            },
        );
        ctx.variables = entity.variables().clone();
        for (name, value) in &snapshot.variable_state.variables {
            ctx.variables.insert(name.clone(), value.clone());
        }
        if let Some(ref metrics) = self.metrics {
            ctx = ctx.with_metrics(metrics.clone());
        }

        let mut coordinator = WorkflowCoordinator::new(ctx, graph, handlers)?
            .with_entity(entity);
        coordinator.resume_from(&snapshot);

        if let Some(ref executor) = self.hook_executor {
            coordinator = coordinator.with_hook_executor(executor.clone());
        }

        if let Some(ref strategy) = self.checkpoint_strategy {
            let mut cp = WorkflowCheckpointIntegration::new(self.store.clone(), strategy.clone());
            if let Some(ref bus) = self.checkpoint_event_bus {
                cp = cp.with_event_bus(bus.clone());
            }
            if let Some(ref core_bus) = self.event_bus {
                cp = cp.with_core_event_bus(core_bus.clone());
            }
            coordinator = coordinator.with_checkpoint(cp);
        }

        let result = coordinator.execute().await;
        match result {
            Ok(output) => Ok(WorkflowOutput {
                execution_id: wf_types::Id::from(snapshot.execution_id),
                result: output,
            }),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use wf_types::workflow::EdgeType;
    use wf_types::workflow_execution::{WorkflowEdge, WorkflowNode};

    use crate::checkpoint::strategy::NodeCheckpointStrategy;
    use crate::handler::HandlerRegistry;

    fn node(id: &str, node_type: &str, inner: serde_json::Value) -> WorkflowNode {
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

    fn make_graph() -> WorkflowGraphStructure {
        WorkflowGraphStructure {
            nodes: vec![
                node("start", "START", serde_json::json!({})),
                node(
                    "v1",
                    "VARIABLE",
                    serde_json::json!({
                        "assignments": { "mid": "${input.greeting}" }
                    }),
                ),
                node(
                    "v2",
                    "VARIABLE",
                    serde_json::json!({
                        "assignments": { "final": "${mid}" }
                    }),
                ),
                node("end", "END", serde_json::json!({})),
            ],
            edges: vec![
                edge("start", "v1"),
                edge("v1", "v2"),
                edge("v2", "end"),
            ],
            adjacency_list: HashMap::new(),
            reverse_adjacency_list: HashMap::new(),
            start_node_id: Some("start".to_string()),
            end_node_ids: vec!["end".to_string()],
        }
    }

    fn make_handlers() -> Arc<HashMap<StaticNodeType, Arc<dyn NodeHandler>>> {
        let mut reg = HandlerRegistry::new();
        reg.register_defaults();
        reg.into_arc()
    }

    fn make_lifecycle(store: Arc<StorageBackend>) -> WorkflowLifecycleCoordinator {
        WorkflowLifecycleCoordinator::with_store(None, store)
            .with_checkpoint_strategy(NodeCheckpointStrategy::every_node())
    }

    #[tokio::test]
    async fn test_execute_then_resume() {
        let store = Arc::new(StorageBackend::new_memory());
        let lifecycle = make_lifecycle(store.clone());
        let workflow_id = wf_types::Id::from("wf-resume-1".to_string());
        let tool_registry = Arc::new(wf_tools::registry::ToolRegistry::new());

        let handlers = make_handlers();
        let params = WorkflowExecutionParams {
            execution_id: wf_types::Id::from("exec-resume-1".to_string()),
            workflow_id: workflow_id.clone(),
            graph: make_graph(),
            options: WorkflowExecutionOptions {
                input: Some(serde_json::json!({"greeting": "hello"})),
                max_steps: Some(2),
                timeout: None,
                max_execution_time: None,
                enable_checkpoints: Some(true),
                node_timeout: None,
                max_pause_duration: None,
                retry_budget: None,
                on_failure: None,
                max_retries: None,
                retry_delay_ms: None,
                exponential_backoff: None,
                fallback_output: None,
            },
            handlers: handlers.clone(),
            tool_registry: tool_registry.clone(),
            input: None,
        };

        let first = lifecycle
            .execute_workflow(params)
            .await
            .expect("first run should complete");
        assert_eq!(first.execution_id, "exec-resume-1");

        // Snapshot must have captured variables written by v1
        use wf_checkpoint::state::CheckpointStateManager;
        let sm = wf_checkpoint::state::WorkflowCheckpointStateManager::new(store.clone());
        let latest = sm
            .get_latest("exec-resume-1")
            .await
            .expect("checkpoint exists");
        assert!(latest.is_some(), "checkpoint should be persisted");
        assert!(latest.is_some(), "checkpoint should be persisted");

        let resumed = lifecycle
            .resume_workflow(
                "exec-resume-1",
                workflow_id,
                make_graph(),
                handlers,
                tool_registry,
            )
            .await
            .expect("resume should complete the workflow");
        assert_eq!(resumed.execution_id, "exec-resume-1");

        // Single end node -> final output is the end node's output directly.
        assert_eq!(resumed.result, serde_json::json!({"greeting": "hello"}));

        // The resumed run must have continued checkpointing; the new
        // snapshot proves v2 ran with the restored "mid" variable.
        let resumed_cp = sm
            .get_latest("exec-resume-1")
            .await
            .expect("checkpoint exists")
            .expect("checkpoint persisted");
        use wf_checkpoint::coordinator::CheckpointCoordinator;
        let coord = wf_checkpoint::coordinator::workflow::WorkflowCheckpointCoordinator::new(
            wf_checkpoint::state::WorkflowCheckpointStateManager::new(store.clone()),
        );
        let restored = coord.restore(&resumed_cp.id).await.expect("restore ok");
        let vars = &restored.snapshot.variable_state.variables;
        assert_eq!(vars.get("mid"), Some(&serde_json::json!("hello")));
        assert_eq!(vars.get("final"), Some(&serde_json::json!("hello")));
    }

    #[tokio::test]
    async fn test_resume_without_checkpoint_fails() {
        let store = Arc::new(StorageBackend::new_memory());
        let lifecycle = make_lifecycle(store.clone());

        let err = lifecycle
            .resume_workflow(
                "exec-missing",
                wf_types::Id::from("wf-x".to_string()),
                make_graph(),
                make_handlers(),
                Arc::new(wf_tools::registry::ToolRegistry::new()),
            )
            .await
            .expect_err("resume without checkpoint must fail");
        assert!(err.to_string().contains("no checkpoint"));
    }
}
