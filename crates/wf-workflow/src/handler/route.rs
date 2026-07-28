use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::Value;
use wf_execution_shared::condition::ConditionEvaluator;
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

    async fn execute(&self, ctx: &mut NodeExecutionContext) -> WorkflowResult<NodeExecutionResult> {
        let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);

        let branches = config.get("branches").and_then(|b| b.as_array());
        let mut next_nodes: Vec<String> = Vec::new();

        if let Some(branches) = branches {
            for branch in branches {
                let condition = branch.get("condition").and_then(|c| c.as_str());
                let target = branch.get("next_node").and_then(|n| n.as_str());

                if let (Some(cond), Some(target)) = (condition, target) {
                    let mut vars = HashMap::new();
                    for entry in ctx.variables.iter() {
                        vars.insert(entry.key().clone(), entry.value().clone());
                    }

                    match ConditionEvaluator::evaluate(cond, &vars) {
                        Ok(true) => {
                            next_nodes.push(target.to_string());
                            break;
                        }
                        _ => continue,
                    }
                }
            }
        }

        Ok(NodeExecutionResult::with_next_nodes(ctx.input.clone(), next_nodes))
    }
}
