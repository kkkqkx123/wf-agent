use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use wf_core::registry::{MutableRegistry, Registry};
use wf_core::EventBus;
use wf_execution_shared::types::execution_entity::IExecutionEntity;
use wf_metrics::MetricsRegistry;
use wf_storage::backend::StorageBackend;
use wf_tools::callback::{
    AgentLoopConfig, AgentLoopInput, AgentLoopOutput, ExecutionCallback, ExecutionStatus,
    WorkflowInput, WorkflowOutput,
};
use wf_tools::error::{ToolError, ToolResult};
use wf_tools::registry::ToolRegistry;
use wf_types::node::StaticNodeType;
use wf_types::workflow_execution::{WorkflowExecutionOptions, WorkflowGraphStructure};
use wf_types::Id;

use crate::checkpoint::NodeCheckpointStrategy;
use crate::coordinator::{WorkflowExecutionParams, WorkflowLifecycleCoordinator};
use crate::entity::WorkflowExecutionEntity;
use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::{HandlerRegistry, NodeHandler};
use crate::registry::{create_execution_registry, create_graph_registry, WorkflowExecutionRegistry};

/// ExecutionCallback implementation backed by the workflow engine.
///
/// Registers workflow templates (graph by id) and launches executions through
/// `WorkflowLifecycleCoordinator`. Running executions are tracked in an
/// execution registry so their status can be queried and cancelled.
pub struct WorkflowExecutionCallback {
    graphs: crate::registry::WorkflowGraphRegistry,
    executions: WorkflowExecutionRegistry,
    handlers: Option<Arc<HashMap<StaticNodeType, Arc<dyn NodeHandler>>>>,
    event_bus: Option<Arc<EventBus>>,
    tool_registry: Arc<ToolRegistry>,
    checkpoint_strategy: Option<NodeCheckpointStrategy>,
    store: Arc<StorageBackend>,
    metrics: Option<Arc<MetricsRegistry>>,
}

impl WorkflowExecutionCallback {
    pub fn new(tool_registry: Arc<ToolRegistry>) -> Self {
        Self {
            graphs: create_graph_registry(),
            executions: create_execution_registry(),
            handlers: None,
            event_bus: None,
            tool_registry,
            checkpoint_strategy: None,
            store: Arc::new(StorageBackend::new_memory()),
            metrics: None,
        }
    }

    pub fn with_handlers(mut self, handlers: Arc<HashMap<StaticNodeType, Arc<dyn NodeHandler>>>) -> Self {
        self.handlers = Some(handlers);
        self
    }

    pub fn with_event_bus(mut self, event_bus: Arc<EventBus>) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    pub fn with_checkpoint_strategy(mut self, strategy: NodeCheckpointStrategy) -> Self {
        self.checkpoint_strategy = Some(strategy);
        self
    }

    pub fn with_store(mut self, store: Arc<StorageBackend>) -> Self {
        self.store = store;
        self
    }

    pub fn with_metrics(mut self, metrics: Arc<MetricsRegistry>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    pub fn register_workflow(&self, workflow_id: Id, graph: WorkflowGraphStructure) -> bool {
        self.graphs
            .register(workflow_id.to_string(), Arc::new(graph))
            .is_ok()
    }

    pub fn unregister_workflow(&self, workflow_id: &Id) {
        self.graphs.unregister(&workflow_id.to_string());
    }

    pub fn has_workflow(&self, workflow_id: &str) -> bool {
        self.graphs.has(workflow_id)
    }

    pub fn registered_workflows(&self) -> Vec<String> {
        self.graphs.list()
    }

    pub fn executions(&self) -> &WorkflowExecutionRegistry {
        &self.executions
    }

    fn handlers(&self) -> Arc<HashMap<StaticNodeType, Arc<dyn NodeHandler>>> {
        match &self.handlers {
            Some(h) => h.clone(),
            None => {
                let mut registry = HandlerRegistry::new();
                registry.register_defaults();
                registry.into_arc()
            }
        }
    }

    async fn launch(
        &self,
        workflow_id: &str,
        options: WorkflowExecutionOptions,
    ) -> WorkflowResult<WorkflowOutput> {
        let graph = self.graphs.get(workflow_id).ok_or_else(|| {
            WorkflowError::CoordinatorError(format!(
                "Workflow {} not registered for execution",
                workflow_id
            ))
        })?;

        let execution_id = wf_common::generate_id();
        let entity =
            WorkflowExecutionEntity::new(execution_id.clone(), Id::from(workflow_id.to_string()));
        let _ = self
            .executions
            .register(execution_id.to_string(), Arc::new(entity));

        let params = WorkflowExecutionParams {
            execution_id: execution_id.clone(),
            workflow_id: Id::from(workflow_id.to_string()),
            graph: (*graph).clone(),
            options,
            handlers: self.handlers(),
            tool_registry: self.tool_registry.clone(),
            input: None,
        };

        let mut lifecycle = WorkflowLifecycleCoordinator::with_store(
            self.event_bus.clone(),
            self.store.clone(),
        );
        if let Some(ref strategy) = self.checkpoint_strategy {
            lifecycle = lifecycle.with_checkpoint_strategy(strategy.clone());
        }
        if let Some(ref metrics) = self.metrics {
            lifecycle = lifecycle.with_metrics(metrics.clone());
        }
        let result = lifecycle.execute_workflow(params).await;

        // The lifecycle coordinator owns its internal entity; mirror the
        // final status onto the entity exposed through the executions
        // registry so query_execution_status stays accurate.
        if let Some(entity) = self.executions.get(&execution_id.to_string()) {
            let mut state = entity.state.write().await;
            match &result {
                Ok(_) => state.complete(),
                Err(e) => state.fail(e.to_string()),
            }
        }
        result
    }
}

impl Default for WorkflowExecutionCallback {
    fn default() -> Self {
        Self::new(Arc::new(ToolRegistry::new()))
    }
}

fn default_options(input: Option<Value>) -> WorkflowExecutionOptions {
    WorkflowExecutionOptions {
        input,
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
    }
}

fn status_string(status: wf_execution_shared::types::execution_entity::ExecutionStatus) -> String {
    use wf_execution_shared::types::execution_entity::ExecutionStatus as S;
    match status {
        S::Created => "created".to_string(),
        S::Running => "running".to_string(),
        S::Paused => "paused".to_string(),
        S::Completed => "completed".to_string(),
        S::Failed => "failed".to_string(),
        S::Cancelled => "cancelled".to_string(),
        S::Stopped => "stopped".to_string(),
        S::Timeout => "timeout".to_string(),
    }
}

#[async_trait]
impl ExecutionCallback for WorkflowExecutionCallback {
    async fn execute_agent_loop(
        &self,
        _config: AgentLoopConfig,
        _input: AgentLoopInput,
    ) -> ToolResult<AgentLoopOutput> {
        Err(ToolError::ExecutionError(
            "agent loop execution is not supported by WorkflowExecutionCallback".to_string(),
        ))
    }

    async fn execute_workflow(
        &self,
        workflow_id: &str,
        input: WorkflowInput,
    ) -> ToolResult<WorkflowOutput> {
        if !self.graphs.has(workflow_id) {
            return Err(ToolError::NotFound(format!(
                "workflow {} is not registered",
                workflow_id
            )));
        }

        let variables = if input.variables.is_empty() {
            None
        } else {
            Some(Value::Object(input.variables.into_iter().collect()))
        };

        self.launch(workflow_id, default_options(variables))
            .await
            .map_err(|e| ToolError::ExecutionError(e.to_string()))
    }

    async fn query_execution_status(&self, execution_id: &str) -> ToolResult<ExecutionStatus> {
        match self.executions.get(execution_id) {
            Some(entity) => Ok(ExecutionStatus {
                execution_id: Id::from(execution_id.to_string()),
                status: status_string(entity.status()),
                progress: None,
            }),
            None => Err(ToolError::NotFound(format!(
                "execution {} not found",
                execution_id
            ))),
        }
    }

    async fn cancel_execution(&self, execution_id: &str) -> ToolResult<()> {
        match self.executions.get(execution_id) {
            Some(entity) => {
                entity
                    .stop()
                    .await
                    .map_err(|e| ToolError::ExecutionError(e.to_string()))
            }
            None => Err(ToolError::NotFound(format!(
                "execution {} not found",
                execution_id
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use wf_types::workflow::EdgeType;
    use wf_types::workflow_execution::{WorkflowEdge, WorkflowNode};

    fn make_graph(nodes: Vec<WorkflowNode>, edges: Vec<WorkflowEdge>) -> WorkflowGraphStructure {
        WorkflowGraphStructure {
            nodes,
            edges,
            adjacency_list: HashMap::new(),
            reverse_adjacency_list: HashMap::new(),
            start_node_id: Some("start".to_string()),
            end_node_ids: vec!["end".to_string()],
        }
    }

    fn make_node(id: &str, node_type: &str) -> WorkflowNode {
        WorkflowNode {
            id: id.to_string(),
            name: Some(id.to_string()),
            node_type: node_type.to_string(),
            inner: serde_json::json!({}),
        }
    }

    fn make_edge(source: &str, target: &str) -> WorkflowEdge {
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

    fn linear_graph() -> WorkflowGraphStructure {
        make_graph(
            vec![
                make_node("start", "START"),
                make_node("var", "VARIABLE"),
                make_node("end", "END"),
            ],
            vec![
                make_edge("start", "var"),
                make_edge("var", "end"),
            ],
        )
    }

    #[tokio::test]
    async fn test_execute_workflow_tool() {
        let callback = WorkflowExecutionCallback::default();
        let workflow_id = wf_common::generate_id();
        assert!(callback.register_workflow(workflow_id.clone(), linear_graph()));
        assert!(callback.has_workflow(&workflow_id.to_string()));

        let output = callback
            .execute_workflow(
                &workflow_id.to_string(),
                WorkflowInput {
                    variables: HashMap::from([(
                        "greeting".to_string(),
                        serde_json::json!("hello"),
                    )]),
                },
            )
            .await
            .expect("workflow should execute");
        assert!(!output.execution_id.is_empty());

        let status = callback
            .query_execution_status(&output.execution_id.to_string())
            .await
            .expect("status should be queryable");
        assert_eq!(status.status, "completed");

        let result = callback
            .execute_workflow(&workflow_id.to_string(), WorkflowInput { variables: HashMap::new() })
            .await
            .expect("second execution should succeed");
        let _ = result;

        callback
            .cancel_execution(&output.execution_id.to_string())
            .await
            .expect("cancel on completed execution should be idempotent");

        let err = callback
            .execute_workflow("missing-id", WorkflowInput { variables: HashMap::new() })
            .await
            .expect_err("unknown workflow must fail");
        assert!(matches!(err, ToolError::NotFound(_)));
    }
}
