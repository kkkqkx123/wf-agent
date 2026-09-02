use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use wf_core::EventBus;
use wf_execution_shared::context::{ExecutorContext, NodeExecutionContext, NodeExecutionResult};
use wf_tools::registry::ToolRegistry;
use wf_types::events::{BaseEvent, EventType};
use wf_types::node::StaticNodeType;
use wf_types::workflow_execution::{WorkflowExecutionOptions, WorkflowGraphStructure};

use crate::coordinator::WorkflowCoordinator;
use crate::entity::WorkflowExecutionEntity;
use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::NodeHandler;

pub struct SubgraphHandler;

#[async_trait]
impl NodeHandler for SubgraphHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::Subgraph
    }

    async fn execute(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> wf_execution_shared::error::ExecutionSharedResult<NodeExecutionResult> {
        self.execute_inner(ctx).await.map_err(Into::into)
    }
}

impl SubgraphHandler {
    async fn execute_inner(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> WorkflowResult<NodeExecutionResult> {
        let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);
        let subgraph_id = config
            .get("subgraph_id")
            .or_else(|| config.get("embed_id"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty());

        let subgraph: WorkflowGraphStructure = match subgraph_id {
            Some(id) => crate::registry::lookup_graph(id).ok_or_else(|| {
                WorkflowError::SubgraphError(format!("Subgraph '{}' is not registered", id))
            })?,
            None => {
                return Err(WorkflowError::SubgraphError(
                    "SUBGRAPH node requires a subgraph_id (or embed_id) config".to_string(),
                ))
            }
        };

        execute_subgraph(ctx, config, subgraph).await
    }
}

/// Shared subgraph execution helper: builds a child `ExecutorContext` and a
/// `WorkflowCoordinator`, applies variable mappings, emits the subgraph
/// start/complete events and records subgraph metrics. SUBGRAPH keeps its
/// independent execution_id semantics (a sub-workflow entity of its own).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn execute_subgraph(
    ctx: &NodeExecutionContext,
    config: &Value,
    subgraph: WorkflowGraphStructure,
) -> WorkflowResult<NodeExecutionResult> {
    let handlers = crate::handler::resolve_handler_registry(ctx)
        .map_err(|e| WorkflowError::SubgraphError(e.to_string()))?;

    let options = WorkflowExecutionOptions {
        input: Some(ctx.input.clone()),
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
        loop_max_iterations_cap: None,
    };

    // Subgraphs are independent workflow entities with their own
    // execution_id, and thus their own variable store.
    let execution_id = wf_common::generate_id();
    let sub_workflow_id = wf_common::generate_id();

    let entity = WorkflowExecutionEntity::new(execution_id.clone(), sub_workflow_id.clone())
        .with_hierarchy_depth(ctx.depth + 1);

    let event_bus = ctx.event_bus.clone();
    let tool_registry = ctx
        .tool_registry
        .clone()
        .unwrap_or_else(|| Arc::new(ToolRegistry::new()));

    let subgraph_metrics = ctx.metrics.as_ref().map(|m| m.subgraph());
    let depth = ctx.depth + 1;
    if let Some(metrics) = &subgraph_metrics {
        metrics.record_execution_start(&ctx.node_id, &ctx.execution_id, depth);
    }
    let start = wf_common::now();

    let exec_ctx = ExecutorContext::new(
        execution_id,
        sub_workflow_id,
        event_bus,
        tool_registry,
        options,
    )
    .with_parent_execution(ctx.execution_id.clone());
    let exec_ctx = match &ctx.metrics {
        Some(metrics) => exec_ctx.with_metrics(metrics.clone()),
        None => exec_ctx,
    };
    let exec_ctx = match &ctx.resource_registries {
        Some(regs) => exec_ctx.with_resource_registries(regs.clone()),
        None => exec_ctx,
    };
    // Subgraphs inherit the parent's tool-level approval config.
    let exec_ctx = match (
        ctx.tool_approval_options.clone(),
        ctx.tool_approval_handler.clone(),
    ) {
        (None, None) => exec_ctx,
        (options, handler) => exec_ctx.with_tool_approval(options, handler),
    };

    crate::handler::variable_mapping::apply_variable_inputs(
        config,
        &ctx.variables,
        &exec_ctx.variables,
    )?;
    let sub_variables = exec_ctx.variables.clone();

    emit_subgraph_event(
        ctx.event_bus.as_ref(),
        EventType::SubgraphStarted,
        &ctx.execution_id,
        &ctx.node_id,
    );

    let mut coordinator: WorkflowCoordinator =
        match WorkflowCoordinator::new(exec_ctx, subgraph, handlers) {
            Ok(coordinator) => coordinator.with_entity(entity),
            Err(err) => {
                if let Some(metrics) = &subgraph_metrics {
                    metrics.record_execution_complete(
                        &ctx.node_id,
                        &ctx.execution_id,
                        false,
                        (wf_common::now() - start) as f64,
                        Some("subgraph_failed"),
                    );
                }
                return Err(err);
            }
        };

    let output = match coordinator.execute().await {
        Ok(output) => {
            crate::handler::variable_mapping::apply_variable_outputs(
                config,
                &sub_variables,
                &ctx.variables,
            )?;
            emit_subgraph_event(
                ctx.event_bus.as_ref(),
                EventType::SubgraphCompleted,
                &ctx.execution_id,
                &ctx.node_id,
            );
            if let Some(metrics) = &subgraph_metrics {
                metrics.record_execution_complete(
                    &ctx.node_id,
                    &ctx.execution_id,
                    true,
                    (wf_common::now() - start) as f64,
                    None,
                );
            }
            output
        }
        Err(err) => {
            if let Some(metrics) = &subgraph_metrics {
                metrics.record_execution_complete(
                    &ctx.node_id,
                    &ctx.execution_id,
                    false,
                    (wf_common::now() - start) as f64,
                    Some("subgraph_failed"),
                );
            }
            return Err(err);
        }
    };

    let mut metadata = HashMap::new();
    metadata.insert(
        "node_count".to_string(),
        Value::Number(serde_json::Number::from(
            coordinator.completed_nodes().len() as u64,
        )),
    );

    Ok(NodeExecutionResult {
        output,
        next_node_ids: Vec::new(),
        metadata,
    })
}

fn emit_subgraph_event(
    event_bus: Option<&Arc<EventBus>>,
    event_type: EventType,
    execution_id: &wf_types::Id,
    node_id: &str,
) {
    let Some(bus) = event_bus else {
        tracing::debug!(execution_id = %execution_id, node_id, ?event_type, "no event bus, skipping subgraph event");
        return;
    };
    let event = BaseEvent {
        id: wf_types::Id::new(),
        r#type: event_type,
        timestamp: wf_common::now(),
        workflow_id: None,
        execution_id: Some(execution_id.clone()),
        agent_loop_id: None,

        event_name: None,
        metadata: Some(HashMap::from([(
            "node_id".to_string(),
            Value::String(node_id.to_string()),
        )])),
    };
    bus.publish_logged(
        event,
        &format!("workflow={} subgraph={}", execution_id, node_id),
    )
    .ok();
}
