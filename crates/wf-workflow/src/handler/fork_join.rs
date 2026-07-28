use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use futures::future::join_all;
use serde_json::Value;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_types::node::StaticNodeType;

use crate::barrier::{BranchResult, FailureStrategy, ForkOutcome, SyncBarrier};
use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::NodeHandler;

pub struct ForkHandler;

#[async_trait]
impl NodeHandler for ForkHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::Fork
    }

    async fn execute(&self, ctx: &mut NodeExecutionContext) -> WorkflowResult<NodeExecutionResult> {
        let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);
        let branches = config.get("branches")
            .and_then(|b| b.as_array())
            .cloned()
            .unwrap_or_default();

        if branches.is_empty() {
            return Err(WorkflowError::ForkJoinError("No branches defined for fork node".to_string()));
        }

        let failure_strategy = config.get("failure_strategy")
            .and_then(|s| s.as_str())
            .and_then(|s| match s {
                "fail_fast" => Some(FailureStrategy::FailFast),
                "continue_on_error" => Some(FailureStrategy::ContinueOnError),
                "fail_on_threshold" => {
                    let threshold = config.get("failure_threshold")
                        .and_then(|t| t.as_f64())
                        .unwrap_or(0.5);
                    Some(FailureStrategy::FailOnThreshold { threshold })
                }
                _ => None,
            })
            .unwrap_or(FailureStrategy::FailFast);

        let barrier = Arc::new(SyncBarrier::new(branches.len()));
        let mut handles = Vec::new();
        let vars = ctx.variables.clone();

        let branch_count = branches.len();
        for branch in &branches {
            let branch_id = branch.get("id")
                .and_then(|id| id.as_str())
                .unwrap_or("branch")
                .to_string();
            let branch_input = branch.get("input").cloned().unwrap_or(ctx.input.clone());
            let _vars_clone = vars.clone();
            let barrier_clone = barrier.clone();

            let handle = tokio::spawn(async move {
                let result = {
                    BranchResult::success(&branch_id, branch_input)
                };
                barrier_clone.notify_branch_completed(&branch_id).await;
                result
            });
            handles.push(handle);
        }

        let results: Vec<BranchResult> = join_all(handles)
            .await
            .into_iter()
            .filter_map(|r| r.ok())
            .collect();

        let outcome = failure_strategy.evaluate(&results);

        let mut metadata = HashMap::new();
        metadata.insert("branch_count".to_string(), Value::Number(serde_json::Number::from(branch_count as u64)));
        metadata.insert("success_count".to_string(), Value::Number(serde_json::Number::from(results.iter().filter(|r| r.success).count() as u64)));
        metadata.insert("outcome".to_string(), Value::String(format!("{:?}", outcome)));

        let mut next_nodes: Vec<String> = Vec::new();
        if outcome != ForkOutcome::Failed {
            let target = config.get("target_join").and_then(|t| t.as_str());
            if let Some(target) = target {
                next_nodes.push(target.to_string());
            }
        }

        let output = serde_json::json!({
            "results": results,
            "outcome": format!("{:?}", outcome),
        });

        Ok(NodeExecutionResult {
            output,
            next_node_ids: next_nodes,
            metadata,
        })
    }
}

pub struct JoinHandler;

#[async_trait]
impl NodeHandler for JoinHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::Join
    }

    async fn execute(&self, ctx: &mut NodeExecutionContext) -> WorkflowResult<NodeExecutionResult> {
        let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);
        let strategy = config.get("strategy")
            .and_then(|s| s.as_str())
            .unwrap_or("merge");

        let aggregated = match strategy {
            "first" => ctx.input.clone(),
            "last" => ctx.input.clone(),
            "merge" => {
                if let Value::Object(map) = &ctx.input {
                    Value::Object(map.clone())
                } else {
                    ctx.input.clone()
                }
            }
            "aggregate" => {
                let handler = config.get("handler").and_then(|h| h.as_str());
                if handler == Some("sum") {
                    if let Value::Array(items) = &ctx.input {
                        let sum: f64 = items.iter()
                            .filter_map(|v| v.as_f64())
                            .sum();
                        Value::Number(serde_json::Number::from_f64(sum).unwrap_or(serde_json::Number::from(0)))
                    } else {
                        ctx.input.clone()
                    }
                } else {
                    ctx.input.clone()
                }
            }
            _ => ctx.input.clone(),
        };

        Ok(NodeExecutionResult::simple(aggregated))
    }
}
