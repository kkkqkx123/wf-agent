use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::Value;
use wf_core::condition::ConditionEvaluator;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_types::node::StaticNodeType;

use crate::error::WorkflowResult;
use crate::handler::NodeHandler;

pub struct RouteHandler;

#[async_trait]
impl NodeHandler for RouteHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::Route
    }

    async fn execute(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> wf_execution_shared::error::ExecutionSharedResult<NodeExecutionResult> {
        self.execute_inner(ctx).await.map_err(Into::into)
    }
}

impl RouteHandler {
    async fn execute_inner(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> WorkflowResult<NodeExecutionResult> {
        let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);
        let mut next_nodes: Vec<String> = Vec::new();

        let conditions = config.get("conditions").and_then(|c| c.as_array());
        if let Some(conditions) = conditions {
            let mut vars = HashMap::new();
            for entry in ctx.variables.iter() {
                vars.insert(entry.key().clone(), entry.value().clone());
            }

            for condition in conditions {
                let expression = condition.get("expression").and_then(|e| e.as_str());
                let target = condition.get("target_node_id").and_then(|t| t.as_str());
                let (Some(expression), Some(target)) = (expression, target) else {
                    continue;
                };
                match ConditionEvaluator::evaluate(expression, &vars) {
                    Ok(true) => {
                        next_nodes.push(target.to_string());
                        break;
                    }
                    _ => continue,
                }
            }
        }

        if next_nodes.is_empty() {
            if let Some(default) = config
                .get("default_target_node_id")
                .and_then(|v| v.as_str())
            {
                next_nodes.push(default.to_string());
            }
        }

        Ok(NodeExecutionResult::with_next_nodes(
            ctx.input.clone(),
            next_nodes,
        ))
    }
}
