use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::Value;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult, NodeInputShape};
use wf_execution_shared::fork::BranchStatus;
use wf_types::node::StaticNodeType;

use super::events::{publish_join_completed, publish_join_started};
use super::graph::find_fork_node;
use super::merge::{
    aggregate_branch_data_outputs, aggregate_branch_messages, aggregate_branch_variables,
    collect_branch_records, merge_outputs,
};
use crate::barrier::BranchResult;
use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::NodeHandler;

pub struct JoinHandler;

#[async_trait]
impl NodeHandler for JoinHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::Join
    }

    async fn execute(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> wf_execution_shared::error::ExecutionSharedResult<NodeExecutionResult> {
        self.execute_inner(ctx).await.map_err(Into::into)
    }
}

impl JoinHandler {
    async fn execute_inner(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> WorkflowResult<NodeExecutionResult> {
        let config: Value = ctx.node_config.clone().unwrap_or(Value::Null);
        let strategy = config
            .get("join_strategy")
            .and_then(|s| s.as_str())
            .unwrap_or("wait_for_all");

        let event_bus = ctx.event_bus.clone();
        publish_join_started(event_bus.as_ref(), ctx, strategy);

        // Prefer the fork registry (live branch records keyed by path id),
        // falling back to the recorded `__fork_outputs_<fork_id>` variable
        // and then to the local input when the JOIN was reached without a
        // fork (e.g. resumed from a checkpoint). The local input is only
        // interpretable as a fork output shape when it arrived as a merged
        // multi-edge object (`Merged`); a bare single value is treated as a
        // pass-through payload.
        let fork_id = ctx
            .graph_structure
            .as_ref()
            .and_then(|g| find_fork_node(ctx, g));
        let registry = fork_id
            .as_ref()
            .and_then(|id| ctx.fork_registries.get(id))
            .cloned();
        let mut path_ids: Vec<String> = config
            .get("fork_path_ids")
            .and_then(|v| v.as_array())
            .map(|ids| {
                ids.iter()
                    .filter_map(|v| v.as_str())
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();
        if path_ids.is_empty() {
            if let Some(registry) = &registry {
                path_ids = registry.path_ids();
            }
        }

        let raw_results = if let Some(registry) = &registry {
            collect_from_registry(ctx, &config, registry, &path_ids, strategy).await?
        } else {
            let fork_output = fork_id
                .as_ref()
                .and_then(|id| ctx.get_variable(&format!("__fork_outputs_{}", id)));
            let (records, dropped) = if let Some(output) = &fork_output {
                collect_branch_records(output)
            } else if ctx.input_shape == NodeInputShape::Merged {
                collect_branch_records(&ctx.input)
            } else {
                (Vec::new(), Vec::new())
            };
            if !dropped.is_empty() {
                crate::degradation::emit_data_degradation(
                    ctx.event_bus.as_deref(),
                    None,
                    &ctx.execution_id,
                    "join_branch_records",
                    &dropped.join("; "),
                );
            }
            records
        };

        let success_records: Vec<BranchResult> =
            raw_results.iter().filter(|r| r.success).cloned().collect();
        let failed_records: Vec<BranchResult> =
            raw_results.iter().filter(|r| !r.success).cloned().collect();

        // Branch-level timeout: a positive `timeout` bounds the JOIN's wait for
        // the branches (relevant for non-blocking forks); joined-late or
        // aborted branches are treated as failures.
        let expected = path_ids.len().max(raw_results.len());

        let mut aggregated = if raw_results.is_empty() {
            ctx.input.clone()
        } else {
            match strategy {
                // wait_for_any returns the first successful branch output.
                "wait_for_any" | "WaitForAny" => success_records
                    .first()
                    .map(|r| r.output.clone())
                    .unwrap_or(ctx.input.clone()),
                // wait_for_n merges the first `threshold` successful outputs.
                "wait_for_n" | "WaitForN" => {
                    let threshold = config
                        .get("threshold")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(success_records.len() as u64)
                        as usize;
                    let taken: Vec<Value> = success_records
                        .iter()
                        .take(threshold.max(1))
                        .map(|r| r.output.clone())
                        .collect();
                    merge_outputs(&taken)
                }
                // wait_for_all merges every successful branch output.
                _ => merge_outputs(
                    &success_records
                        .iter()
                        .map(|r| r.output.clone())
                        .collect::<Vec<_>>(),
                ),
            }
        };

        // JOIN aggregation: merge variables (by mapping), message contexts and
        // data outputs from the successful branches into the parent scope.
        aggregate_branch_variables(&config, &success_records, ctx)?;
        aggregate_branch_messages(ctx, &success_records);
        aggregate_branch_data_outputs(&config, &success_records, &mut aggregated);

        publish_join_completed(
            event_bus.as_ref(),
            ctx,
            strategy,
            success_records.len(),
            failed_records.len(),
            expected,
        );

        let mut metadata = HashMap::new();
        metadata.insert(
            "failed_branches".to_string(),
            Value::Array(
                failed_records
                    .iter()
                    .map(|r| {
                        serde_json::json!({
                            "branch_id": r.branch_id,
                            "error": r.error.clone().unwrap_or_default(),
                        })
                    })
                    .collect(),
            ),
        );

        Ok(NodeExecutionResult {
            output: aggregated,
            next_node_ids: Vec::new(),
            metadata,
        })
    }
}

/// Wait through the fork registry until the strategy's required number of
/// branches settle (bounded by the JOIN `timeout`), then convert the live
/// records into `BranchResult`s. On timeout or parent cancellation the JOIN
/// fails.
async fn collect_from_registry(
    ctx: &NodeExecutionContext,
    config: &Value,
    registry: &std::sync::Arc<wf_execution_shared::fork::ForkRegistry>,
    path_ids: &[String],
    strategy: &str,
) -> WorkflowResult<Vec<BranchResult>> {
    let join_timeout = config.get("timeout").and_then(|v| v.as_u64());
    let required = match strategy {
        "wait_for_any" | "WaitForAny" => 1,
        "wait_for_n" | "WaitForN" => config
            .get("threshold")
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as usize,
        _ => path_ids.len(),
    };
    let wait = registry.wait_for_count(path_ids, required, join_timeout);
    let ok = match &ctx.cancellation {
        Some(token) => tokio::select! {
            ok = wait => ok,
            _ = token.cancelled() => {
                registry.abort_all();
                return Err(WorkflowError::NodeFailure {
                    node_id: ctx.node_id.clone(),
                    category: wf_types::workflow::error_branch::NodeErrorCategory::CancelledInterrupted,
                    detail: format!("JOIN node '{}' wait cancelled", ctx.node_id),
                });
            }
        },
        None => wait.await,
    };
    if !ok {
        return Err(WorkflowError::NodeFailure {
            node_id: ctx.node_id.clone(),
            category: wf_types::workflow::error_branch::NodeErrorCategory::TransportTimeout,
            detail: format!(
                "JOIN node '{}' timed out waiting for fork branches to settle",
                ctx.node_id
            ),
        });
    }
    Ok(registry
        .records(path_ids)
        .into_iter()
        .map(|(path_id, record)| match record.status {
            BranchStatus::Completed => BranchResult::success_with_variables(
                path_id,
                record.output.clone().unwrap_or(Value::Null),
                record.variables.clone(),
            ),
            BranchStatus::Failed => BranchResult::failure(
                path_id,
                record
                    .error
                    .clone()
                    .unwrap_or_else(|| "branch failed".to_string()),
            ),
            BranchStatus::Cancelled => BranchResult::failure(path_id, "branch cancelled"),
            BranchStatus::Running => BranchResult::failure(path_id, "branch still running"),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn join_strategies_aggregate_branch_outputs() {
        let input = serde_json::json!({
            "outputs": [
                {"branch_id": "b1", "output": {"x": 1}, "success": true},
                {"branch_id": "b2", "output": {"y": 2}, "success": true}
            ]
        });

        let mut ctx = NodeExecutionContext::new(
            wf_types::Id::new(),
            "join1".to_string(),
            StaticNodeType::Join,
            input,
            Arc::new(dashmap::DashMap::new()),
        )
        .with_node_config(serde_json::json!({"join_strategy": "wait_for_all"}));
        ctx.input_shape = NodeInputShape::Merged;
        let result = JoinHandler
            .execute(&mut ctx)
            .await
            .expect("wait_for_all should merge all outputs");
        assert_eq!(result.output, serde_json::json!({"x": 1, "y": 2}));

        let mut ctx = NodeExecutionContext::new(
            wf_types::Id::new(),
            "join1".to_string(),
            StaticNodeType::Join,
            serde_json::json!({
                "outputs": [
                    {"branch_id": "b1", "output": {"x": 1}, "success": true},
                    {"branch_id": "b2", "output": {"y": 2}, "success": true}
                ]
            }),
            Arc::new(dashmap::DashMap::new()),
        )
        .with_node_config(serde_json::json!({"join_strategy": "wait_for_any"}));
        ctx.input_shape = NodeInputShape::Merged;
        let result = JoinHandler
            .execute(&mut ctx)
            .await
            .expect("wait_for_any should return the first output");
        assert_eq!(result.output, serde_json::json!({"x": 1}));

        let mut ctx = NodeExecutionContext::new(
            wf_types::Id::new(),
            "join1".to_string(),
            StaticNodeType::Join,
            serde_json::json!({
                "outputs": [
                    {"branch_id": "b1", "output": {"x": 1}, "success": true},
                    {"branch_id": "b2", "output": {"y": 2}, "success": true}
                ]
            }),
            Arc::new(dashmap::DashMap::new()),
        )
        .with_node_config(serde_json::json!({
            "join_strategy": "wait_for_n",
            "threshold": 1
        }));
        ctx.input_shape = NodeInputShape::Merged;
        let result = JoinHandler
            .execute(&mut ctx)
            .await
            .expect("wait_for_n should merge up to the threshold");
        assert_eq!(result.output, serde_json::json!({"x": 1}));
    }
}
