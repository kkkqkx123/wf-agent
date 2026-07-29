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

fn resolve_parent_handlers(ctx: &NodeExecutionContext) -> Arc<HashMap<StaticNodeType, Arc<dyn NodeHandler>>> {
    match &ctx.handler_registry {
        Some(any) => {
            match any.clone().downcast::<HashMap<StaticNodeType, Arc<dyn NodeHandler>>>() {
                Ok(handlers) => handlers,
                Err(_) => Arc::new(HashMap::new()),
            }
        }
        None => Arc::new(HashMap::new()),
    }
}

pub struct SubgraphHandler;

#[async_trait]
impl NodeHandler for SubgraphHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::Subgraph
    }

    async fn execute(&self, ctx: &mut NodeExecutionContext) -> WorkflowResult<NodeExecutionResult> {
        let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);
        let subgraph_value = config.get("subgraph").or_else(|| config.get("graph"));

        let subgraph: WorkflowGraphStructure = match subgraph_value {
            Some(v) => serde_json::from_value(v.clone())
                .map_err(|e| WorkflowError::SubgraphError(format!("Invalid subgraph definition: {}", e)))?,
            None => return Err(WorkflowError::SubgraphError("No subgraph definition found".to_string())),
        };

        let options = WorkflowExecutionOptions {
            input: Some(ctx.input.clone()),
            max_steps: config.get("max_steps").and_then(|v| v.as_u64()).map(|v| v as u32),
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
        };

        let handlers = resolve_parent_handlers(ctx);

        let execution_id = wf_types::Id::new();
        let sub_workflow_id = wf_types::Id::new();

        let entity = WorkflowExecutionEntity::new(
            execution_id.clone(),
            sub_workflow_id.clone(),
        );

        let event_bus = ctx.event_bus.clone();
        let tool_registry = Arc::new(ToolRegistry::new());

        let exec_ctx = ExecutorContext::new(
            execution_id,
            sub_workflow_id,
            event_bus,
            tool_registry,
            options,
        );

        emit_subgraph_event(ctx.event_bus.as_ref(), EventType::SubgraphStarted, &ctx.execution_id, &ctx.node_id);

        let mut coordinator = WorkflowCoordinator::new(exec_ctx, subgraph, handlers)?
            .with_entity(entity);

        let output = coordinator.execute().await?;

        emit_subgraph_event(ctx.event_bus.as_ref(), EventType::SubgraphCompleted, &ctx.execution_id, &ctx.node_id);

        let mut metadata = HashMap::new();
        metadata.insert("node_count".to_string(), Value::Number(
            serde_json::Number::from(coordinator.completed_nodes().len() as u64)
        ));

        Ok(NodeExecutionResult {
            output,
            next_node_ids: Vec::new(),
            metadata,
        })
    }
}

fn emit_subgraph_event(event_bus: Option<&Arc<EventBus>>, event_type: EventType, execution_id: &wf_types::Id, node_id: &str) {
    let Some(bus) = event_bus else { return };
    let event = BaseEvent {
        id: wf_types::Id::new(),
        r#type: event_type,
        timestamp: wf_common::now(),
        workflow_id: None,
        execution_id: Some(execution_id.clone()),
        agent_loop_id: None,
        metadata: Some(HashMap::from([
            ("node_id".to_string(), Value::String(node_id.to_string())),
        ])),
    };
    let _ = bus.publish(event);
}
