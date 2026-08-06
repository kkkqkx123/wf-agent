use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use wf_core::registry::MutableRegistry;
use wf_execution_shared::context::ExecutorContext;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_tools::callback::WorkflowOutput;
use wf_types::workflow_execution::{
    WorkflowEdge, WorkflowExecutionOptions, WorkflowGraphStructure, WorkflowNode,
};
use wf_workflow::entity::WorkflowExecutionEntity;
use wf_workflow::validation::GraphValidator;
use wf_workflow::WorkflowCoordinator;

use crate::context::ApiContext;
use crate::error::ApiError;
use crate::stream::{spawn_execution_stream, ExecutionEventStream};

/// Parameters for executing a stored workflow.
#[derive(Debug, Clone, Default)]
pub struct ExecuteWorkflowParams {
    pub workflow_id: String,
    /// Top-level execution input exposed as the `input` variable.
    pub input: Option<Value>,
    /// Execution options; `None` uses engine defaults.
    pub options: Option<WorkflowExecutionOptions>,
}

/// Application-facing workflow execution API.
///
/// Launches executions of stored workflows through the `wf-workflow` engine,
/// keeps a live entity handle in the context so `pause` / `resume` / `cancel`
/// and status queries work while the coordinator drives the same entity.
pub struct WorkflowApi {
    ctx: Arc<ApiContext>,
}

impl WorkflowApi {
    pub fn new(ctx: Arc<ApiContext>) -> Self {
        Self { ctx }
    }

    /// Load a stored workflow definition and convert it into an executable
    /// graph, running the full graph validator (start/end, fork-join pairs,
    /// loop pairs, subgraph, sync nodes, isolated nodes, cycles).
    pub async fn resolve_graph(
        &self,
        workflow_id: &str,
    ) -> crate::error::ApiResult<WorkflowGraphStructure> {
        let definition = self
            .ctx
            .storage
            .workflow
            .load(workflow_id)
            .await?
            .ok_or_else(|| ApiError::not_found("workflow", workflow_id))?;
        let graph = definition_to_graph(&definition);
        GraphValidator::validate(&graph).map_err(|errors| {
            let detail = errors
                .iter()
                .map(|e| format!("{}: {}", e.field, e.message))
                .collect::<Vec<_>>()
                .join("; ");
            ApiError::Validation(format!(
                "workflow graph validation failed ({} error(s)): {}",
                errors.len(),
                detail
            ))
        })?;
        Ok(graph)
    }

    /// Execute a workflow to completion and await its output.
    pub async fn execute(
        &self,
        params: ExecuteWorkflowParams,
    ) -> crate::error::ApiResult<WorkflowOutput> {
        let graph = self.resolve_graph(&params.workflow_id).await?;
        let entity = self.spawn_entity(&params.workflow_id);
        run_workflow(
            &self.ctx,
            entity.clone(),
            graph,
            params.input,
            params.options,
        )
        .await
        .inspect_err(|_| mark_failed(&entity))
    }

    /// Execute a workflow and stream engine events (`WorkflowExecutionStarted`,
    /// `NodeStarted`, `NodeCompleted`, `WorkflowExecutionCompleted`, ...)
    /// emitted for the execution, ending with `Completed` / `Failed`.
    pub async fn stream(
        &self,
        params: ExecuteWorkflowParams,
    ) -> crate::error::ApiResult<ExecutionEventStream> {
        let graph = self.resolve_graph(&params.workflow_id).await?;
        let entity = self.spawn_entity(&params.workflow_id);
        let execution_id = entity.id().clone();
        let (stream, sink) =
            spawn_execution_stream(Some(self.ctx.event_bus.clone()), execution_id.to_string());
        let ctx = self.ctx.clone();
        tokio::spawn(async move {
            let outcome =
                run_workflow(&ctx, entity.clone(), graph, params.input, params.options).await;
            match outcome {
                Ok(output) => {
                    sink.completed(output.result, 1).await;
                }
                Err(e) => {
                    mark_failed(&entity);
                    sink.failed(e.to_string()).await;
                }
            }
        });
        Ok(stream)
    }

    /// Pause a running workflow execution (checked between nodes).
    pub async fn pause(&self, execution_id: &str) -> crate::error::ApiResult<()> {
        let entity = self.live_entity(execution_id)?;
        entity.interruption().pause()?;
        entity.state.write().await.pause();
        Ok(())
    }

    /// Resume a paused workflow execution.
    pub async fn resume(&self, execution_id: &str) -> crate::error::ApiResult<()> {
        let entity = self.live_entity(execution_id)?;
        entity.interruption().resume()?;
        entity.state.write().await.resume();
        Ok(())
    }

    /// Cancel (stop) a running workflow execution.
    pub async fn cancel(&self, execution_id: &str) -> crate::error::ApiResult<()> {
        let entity = self.live_entity(execution_id)?;
        entity.interruption().stop()?;
        entity.state.write().await.cancel();
        Ok(())
    }

    /// Query the live status of a workflow execution.
    pub async fn status(&self, execution_id: &str) -> crate::error::ApiResult<String> {
        let entity = self.live_entity(execution_id)?;
        Ok(format!("{:?}", entity.state.read().await.status()))
    }

    fn live_entity(
        &self,
        execution_id: &str,
    ) -> crate::error::ApiResult<Arc<WorkflowExecutionEntity>> {
        self.ctx
            .workflow_execution(execution_id)
            .ok_or_else(|| ApiError::execution_not_found(execution_id))
    }

    fn spawn_entity(&self, workflow_id: &str) -> Arc<WorkflowExecutionEntity> {
        let execution_id = wf_common::generate_id();
        let entity = Arc::new(WorkflowExecutionEntity::new(
            wf_types::Id::from(execution_id.clone()),
            wf_types::Id::from(workflow_id.to_string()),
        ));
        let _ = self
            .ctx
            .workflow_executions
            .register(execution_id, entity.clone());
        entity
    }
}

/// Run a workflow against the shared context, driving the given entity so
/// external `pause` / `resume` / `cancel` calls apply to the live execution.
async fn run_workflow(
    ctx: &ApiContext,
    entity: Arc<WorkflowExecutionEntity>,
    graph: WorkflowGraphStructure,
    input: Option<Value>,
    options: Option<WorkflowExecutionOptions>,
) -> crate::error::ApiResult<WorkflowOutput> {
    let mut options = options.unwrap_or_else(default_options);
    if options.input.is_none() {
        options.input = input;
    }

    let mut exec_ctx = ExecutorContext::new(
        entity.id().clone(),
        entity.workflow_id().clone(),
        Some(ctx.event_bus.clone()),
        ctx.tool_registry.clone(),
        options,
    );
    exec_ctx.variables = entity.variables().clone();
    if let Some(ref metrics) = ctx.metrics {
        metrics
            .workflow()
            .record_execution_start(entity.id(), entity.workflow_id());
        exec_ctx = exec_ctx.with_metrics(metrics.clone());
    }

    entity.state.write().await.start();

    let mut coordinator =
        WorkflowCoordinator::new(exec_ctx, graph, ctx.handlers())?.with_entity_arc(entity.clone());

    let result = coordinator.execute().await;
    match result {
        Ok(output) => Ok(WorkflowOutput {
            execution_id: entity.id().clone(),
            result: output,
        }),
        Err(e) => Err(e.into()),
    }
}

fn default_options() -> WorkflowExecutionOptions {
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
    }
}

fn mark_failed(entity: &WorkflowExecutionEntity) {
    if let Ok(mut state) = entity.state.try_write() {
        state.fail("execution failed".to_string());
    }
}

/// Convert a stored [`WorkflowDefinition`] into an executable graph.
///
/// Nodes map their `config` onto the flattened `inner` field consumed by the
/// node handlers; edges map directly. The first node is the start and the
/// last node the end (flat template semantics).
pub fn definition_to_graph(
    definition: &wf_types::workflow::WorkflowDefinition,
) -> WorkflowGraphStructure {
    let nodes: Vec<WorkflowNode> = definition
        .nodes
        .iter()
        .map(|node| WorkflowNode {
            id: node.id.clone(),
            name: node.name.clone(),
            node_type: node_type_string(&node.node_type),
            inner: node.config.clone().unwrap_or(Value::Null),
        })
        .collect();
    let edges: Vec<WorkflowEdge> = definition
        .edges
        .iter()
        .map(|edge| WorkflowEdge {
            id: edge.id.clone(),
            source_node_id: edge.source_node_id.clone(),
            target_node_id: edge.target_node_id.clone(),
            r#type: edge.r#type.clone(),
            condition: edge.condition.clone(),
            label: edge.label.clone(),
            description: edge.description.clone(),
        })
        .collect();
    WorkflowGraphStructure {
        start_node_id: nodes.first().map(|node| node.id.clone()),
        end_node_ids: nodes
            .last()
            .map(|node| vec![node.id.clone()])
            .unwrap_or_default(),
        nodes,
        edges,
        adjacency_list: HashMap::new(),
        reverse_adjacency_list: HashMap::new(),
    }
}

fn node_type_string(node_type: &wf_types::node::StaticNodeType) -> String {
    serde_json::to_string(node_type)
        .map(|s| s.trim_matches('"').to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_resource::registrar::Registries;
    use wf_resource::starter::BundleRegistry;
    use wf_storage::context::StorageContext;
    use wf_types::node::BaseStaticNode;
    use wf_types::node::StaticNodeType;
    use wf_types::workflow::edge::EdgeType;
    use wf_types::workflow::WorkflowDefinition;

    fn make_definition(id: &str) -> WorkflowDefinition {
        WorkflowDefinition {
            id: id.into(),
            name: format!("Workflow {}", id),
            description: None,
            r#type: None,
            version: Some("1.0.0".into()),
            nodes: vec![
                BaseStaticNode {
                    id: "start".into(),
                    node_type: StaticNodeType::Start,
                    name: Some("start".into()),
                    description: None,
                    config: None,
                    execution_config: None,
                },
                BaseStaticNode {
                    id: "v1".into(),
                    node_type: StaticNodeType::Variable,
                    name: Some("v1".into()),
                    description: None,
                    config: Some(serde_json::json!({
                        "variable_name": "final",
                        "expression": "${input.greeting}",
                    })),
                    execution_config: None,
                },
                BaseStaticNode {
                    id: "end".into(),
                    node_type: StaticNodeType::End,
                    name: Some("end".into()),
                    description: None,
                    config: None,
                    execution_config: None,
                },
            ],
            edges: vec![
                wf_types::workflow::Edge {
                    id: "e1".into(),
                    source_node_id: "start".into(),
                    target_node_id: "v1".into(),
                    r#type: EdgeType::Default,
                    condition: None,
                    label: None,
                    description: None,
                    weight: None,
                    metadata: None,
                },
                wf_types::workflow::Edge {
                    id: "e2".into(),
                    source_node_id: "v1".into(),
                    target_node_id: "end".into(),
                    r#type: EdgeType::Default,
                    condition: None,
                    label: None,
                    description: None,
                    weight: None,
                    metadata: None,
                },
            ],
            config: None,
            variables: None,
            triggers: None,
            triggered_subworkflow_config: None,
            metadata: None,
            available_tools: None,
            created_at: wf_common::now(),
            updated_at: wf_common::now(),
        }
    }

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(Registries::new()),
            Arc::new(BundleRegistry::new()),
        ))
    }

    #[test]
    fn converts_definition_to_graph() {
        let graph = definition_to_graph(&make_definition("wf-graph"));
        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.start_node_id.as_deref(), Some("start"));
        assert_eq!(graph.end_node_ids, vec!["end".to_string()]);
        assert_eq!(
            graph.nodes[1]
                .inner
                .get("variable_name")
                .and_then(|v| v.as_str()),
            Some("final")
        );
        assert_eq!(graph.nodes[1].node_type, "VARIABLE");
    }

    #[tokio::test]
    async fn executes_workflow_and_queries_status() {
        let ctx = make_ctx();
        let definition = make_definition("wf-exec-1");
        ctx.storage.workflow.save(&definition).await.unwrap();

        let api = WorkflowApi::new(ctx.clone());
        let output = api
            .execute(ExecuteWorkflowParams {
                workflow_id: "wf-exec-1".into(),
                input: Some(serde_json::json!({"greeting": "hello"})),
                options: None,
            })
            .await
            .expect("workflow should complete");
        assert!(!output.execution_id.is_empty());
        assert_eq!(output.result, serde_json::json!({"greeting": "hello"}));

        let status = api
            .status(&output.execution_id.to_string())
            .await
            .expect("status query");
        assert_eq!(status, "Completed");
    }

    #[tokio::test]
    async fn rejects_unknown_workflow() {
        let ctx = make_ctx();
        let api = WorkflowApi::new(ctx);
        let err = api
            .execute(ExecuteWorkflowParams {
                workflow_id: "missing".into(),
                input: None,
                options: None,
            })
            .await
            .expect_err("unknown workflow must fail");
        assert!(matches!(err, ApiError::NotFound { .. }));
    }
}
