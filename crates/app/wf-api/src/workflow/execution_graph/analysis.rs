use std::collections::{BTreeMap, HashSet};


use wf_storage::adapter::base::BaseStorageAdapter;
use wf_types::workflow_execution::WorkflowGraphStructure;

use crate::infra::context::ApiContext;
use crate::infra::error::{not_found, ApiResult};
use crate::workflow::workflow_execution::definition_to_graph;

use super::*;

pub async fn analyze(ctx: &ApiContext, execution_id: &str) -> ApiResult<ExecutionPathAnalysis> {
    let workflow_id = if let Some(entity) = ctx.workflow_execution(execution_id) {
        Some(entity.workflow_id().to_string())
    } else if let Some(record) = ctx.storage.workflow_execution.load(execution_id).await? {
        Some(record.workflow_id.to_string())
    } else {
        None
    };

    let graph = resolve_graph(ctx, execution_id)
        .await?
        .unwrap_or_else(<WorkflowGraphStructure as super::paths::EmptyGraph>::default_empty);
    let paths = super::paths::enumerate_paths(&graph);
    let critical_path = paths
        .iter()
        .max_by_key(|path| path.length)
        .cloned()
        .unwrap_or(ExecutionPath {
            nodes: Vec::new(),
            length: 0,
        });

    let mut node_type_distribution = BTreeMap::new();
    for node in &graph.nodes {
        *node_type_distribution
            .entry(node.node_type.clone())
            .or_insert(0) += 1;
    }

    let executed_nodes = executed_nodes(ctx, execution_id).await;
    let decision_points = super::paths::analyze_decision_points(&graph, &executed_nodes);

    let executed_set: HashSet<&str> = executed_nodes.iter().map(String::as_str).collect();
    let unexecuted_nodes = graph
        .nodes
        .iter()
        .map(|n| n.id.clone())
        .filter(|id| !executed_set.contains(id.as_str()))
        .collect();

    Ok(ExecutionPathAnalysis {
        execution_id: execution_id.to_string(),
        workflow_id,
        paths,
        critical_path,
        node_type_distribution,
        executed_nodes,
        decision_points,
        unexecuted_nodes,
    })
}

/// Resolve the graph of an execution: the live entity has none, the
/// persisted record may carry one, otherwise the workflow definition is
/// converted (flat template semantics).
pub(crate) async fn resolve_graph(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Option<WorkflowGraphStructure>> {
    if let Some(record) = ctx.storage.workflow_execution.load(execution_id).await? {
        if let Some(graph) = record.graph {
            return Ok(Some(graph));
        }
        let definition = ctx
            .storage
            .workflow
            .load(&record.workflow_id)
            .await?
            .ok_or_else(|| not_found("workflow", &record.workflow_id))?;
        return Ok(Some(definition_to_graph(&definition)));
    }
    Ok(None)
}

/// Actual execution order from the live entity's node execution history
/// (successful attempts only, in start-time order), falling back to the
/// persisted record's `node_results` after a restart.
pub(super) async fn executed_nodes(ctx: &ApiContext, execution_id: &str) -> Vec<String> {
    if let Some(entity) = ctx.workflow_execution(execution_id) {
        let state = entity.state.read().await;
        let mut records = state.node_execution_history().to_vec();
        records.sort_by_key(|r| r.start_time);
        return records
            .into_iter()
            .filter(|r| r.success)
            .map(|r| r.node_id)
            .collect();
    }
    let Ok(Some(record)) = ctx.storage.workflow_execution.load(execution_id).await else {
        return Vec::new();
    };
    let mut results = record.node_results.unwrap_or_default();
    results.sort_by_key(|r| r.started_at.unwrap_or(0));
    results
        .into_iter()
        .filter(|r| r.status == "completed")
        .map(|r| r.node_id)
        .collect()
}

/// Node timing records of an execution: the live entity's node execution
/// history, otherwise the persisted record's `node_results`.
pub(super) async fn node_timings(ctx: &ApiContext, execution_id: &str) -> Vec<NodeTiming> {
    if let Some(entity) = ctx.workflow_execution(execution_id) {
        let state = entity.state.read().await;
        let mut records: Vec<_> = state
            .node_execution_history()
            .iter()
            .map(|r| NodeTiming {
                node_id: r.node_id.clone(),
                node_name: r.node_name.clone(),
                node_type: r.node_type.clone(),
                duration_ms: r
                    .end_time
                    .map(|end| (end - r.start_time).max(0))
                    .unwrap_or(0),
                success: r.success,
            })
            .collect();
        records.sort_by_key(|r| r.node_id.clone());
        return records;
    }
    let Ok(Some(record)) = ctx.storage.workflow_execution.load(execution_id).await else {
        return Vec::new();
    };
    record
        .node_results
        .unwrap_or_default()
        .into_iter()
        .map(|result| NodeTiming {
            node_id: result.node_id.clone(),
            node_name: result.node_id.clone(),
            node_type: "unknown".to_string(),
            duration_ms: result
                .completed_at
                .zip(result.started_at)
                .map(|(end, start)| (end - start).max(0))
                .unwrap_or(0),
            success: result.status == "completed",
        })
        .collect()
}

