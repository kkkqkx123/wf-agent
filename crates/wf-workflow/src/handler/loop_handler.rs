use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::Value;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_types::node::StaticNodeType;

use crate::error::WorkflowResult;
use crate::handler::NodeHandler;

pub struct LoopStartHandler;

#[async_trait]
impl NodeHandler for LoopStartHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::LoopStart
    }

    async fn execute(&self, ctx: &mut NodeExecutionContext) -> WorkflowResult<NodeExecutionResult> {
        let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);
        let loop_id = config.get("loop_id")
            .and_then(|v| v.as_str())
            .unwrap_or(&ctx.node_id)
            .to_string();
        let max_iterations = config.get("max_iterations")
            .and_then(|v| v.as_u64())
            .unwrap_or(100) as u32;

        let counter_var = format!("__loop_{}_counter", loop_id);
        let current = ctx.get_variable(&counter_var)
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        if current >= max_iterations {
            return Ok(NodeExecutionResult {
                output: ctx.input.clone(),
                next_node_ids: Vec::new(),
                metadata: HashMap::new(),
            });
        }

        ctx.set_variable(counter_var, Value::Number((current + 1).into()));

        Ok(NodeExecutionResult::simple(ctx.input.clone()))
    }
}

pub struct LoopEndHandler;

#[async_trait]
impl NodeHandler for LoopEndHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::LoopEnd
    }

    async fn execute(&self, ctx: &mut NodeExecutionContext) -> WorkflowResult<NodeExecutionResult> {
        let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);
        let loop_id = config.get("loop_id")
            .and_then(|v| v.as_str())
            .unwrap_or("default")
            .to_string();

        let condition = config.get("condition").and_then(|c| c.as_str());
        let target_node = config.get("target_node").and_then(|t| t.as_str());

        let counter_var = format!("__loop_{}_counter", loop_id);
        let current = ctx.get_variable(&counter_var)
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        let should_continue = if let Some(cond) = condition {
            let mut vars = HashMap::new();
            for entry in ctx.variables.iter() {
                vars.insert(entry.key().clone(), entry.value().clone());
            }
            wf_execution_shared::condition::ConditionEvaluator::evaluate(cond, &vars)
                .unwrap_or(false)
        } else {
            current > 0
        };

        let mut metadata = HashMap::new();
        metadata.insert("loop_id".to_string(), Value::String(loop_id));
        metadata.insert("iteration".to_string(), Value::Number(current.into()));
        metadata.insert("should_continue".to_string(), Value::Bool(should_continue));

        let next_node_ids = if should_continue {
            if let Some(target) = target_node {
                vec![target.to_string()]
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        Ok(NodeExecutionResult {
            output: ctx.input.clone(),
            next_node_ids,
            metadata,
        })
    }
}
