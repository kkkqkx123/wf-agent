use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use wf_core::EventBus;
use wf_execution_shared::context::{ExecutorContext, NodeExecutionContext, NodeExecutionResult};
use wf_tools::registry::ToolRegistry;
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

        let handlers: Arc<HashMap<StaticNodeType, Arc<dyn NodeHandler>>> = Arc::new(HashMap::new());

        let execution_id = wf_types::Id::new();
        let sub_workflow_id = wf_types::Id::new();

        let entity = WorkflowExecutionEntity::new(
            execution_id.clone(),
            sub_workflow_id.clone(),
        );

        let event_bus = Arc::new(EventBus::new(1024));
        let tool_registry = Arc::new(ToolRegistry::new());

        let exec_ctx = ExecutorContext::new(
            execution_id,
            sub_workflow_id,
            event_bus,
            tool_registry,
            options,
        );

        let mut coordinator = WorkflowCoordinator::new(exec_ctx, subgraph, handlers)?
            .with_entity(entity);

        let output = coordinator.execute().await?;

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
