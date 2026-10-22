
use wf_storage::adapter::base::BaseStorageAdapter;

use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

use super::*;

pub async fn get_node_execution_stats(
    ctx: &ApiContext,
    execution_id: &str,
    filter: Option<&ExtendedNodeExecutionFilter>,
) -> ApiResult<NodeExecutionStats> {
    let mut records = get_execution_node_analyses(ctx, execution_id).await?;
    if let Some(filter) = filter {
        records.retain(|r| super::records::filter_matches(r, filter));
    }

    let mut stats = NodeExecutionStats {
        total_executions: records.len(),
        success_count: 0,
        failure_count: 0,
        average_execution_time: 0,
        min_execution_time: i64::MAX,
        max_execution_time: 0,
        retry_total: 0,
    };

    let mut duration_sum = 0i64;
    for record in &records {
        match record.status.as_str() {
            "completed" => stats.success_count += 1,
            "failed" => stats.failure_count += 1,
            _ => {}
        }
        stats.retry_total += record.retry_count;
        if let Some(duration) = record.duration {
            duration_sum += duration;
            stats.min_execution_time = stats.min_execution_time.min(duration);
            stats.max_execution_time = stats.max_execution_time.max(duration);
        }
    }
    if stats.total_executions > 0 {
        stats.average_execution_time = duration_sum / stats.total_executions as i64;
    }
    if stats.min_execution_time == i64::MAX {
        stats.min_execution_time = 0;
    }
    Ok(stats)
}

/// Extended node execution records of a specific node type.
pub async fn get_node_executions_by_type(
    ctx: &ApiContext,
    execution_id: &str,
    node_type: &str,
) -> ApiResult<Vec<ExtendedNodeExecutionRecordView>> {
    Ok(get_execution_node_analyses(ctx, execution_id)
        .await?
        .into_iter()
        .filter(|r| r.node_type == node_type)
        .collect())
}

/// Failed node executions of a workflow execution, newest first.
pub async fn get_failed_nodes(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<ExtendedNodeExecutionRecordView>> {
    let mut records: Vec<ExtendedNodeExecutionRecordView> =
        get_execution_node_analyses(ctx, execution_id)
            .await?
            .into_iter()
            .filter(|r| r.status == "failed")
            .collect();
    records.sort_by_key(|r| std::cmp::Reverse(r.start_time));
    Ok(records)
}

/// Raw node attempts: live entity's node execution history first, then the
/// persisted record's `node_results`.
pub(super) async fn raw_attempts(ctx: &ApiContext, execution_id: &str) -> ApiResult<Vec<RawNodeAttempt>> {
    if let Some(entity) = ctx.workflow_execution(execution_id) {
        let state = entity.state.read().await;
        let mut records: Vec<RawNodeAttempt> = state
            .node_execution_history()
            .iter()
            .map(|r| RawNodeAttempt {
                node_id: r.node_id.clone(),
                node_name: r.node_name.clone(),
                node_type: r.node_type.clone(),
                start_time: r.start_time,
                end_time: r.end_time,
                success: r.success,
                error: r.error.clone(),
                input: None,
                output: None,
            })
            .collect();
        records.sort_by_key(|r| r.start_time);
        return Ok(records);
    }
    let (results, started_at) = match ctx.storage.workflow_execution.load(execution_id).await? {
        Some(record) => (record.node_results.unwrap_or_default(), record.started_at),
        None => (Vec::new(), 0),
    };
    let mut records: Vec<RawNodeAttempt> = results
        .into_iter()
        .map(|result| RawNodeAttempt {
            node_id: result.node_id.clone(),
            node_name: result.node_id,
            node_type: "unknown".to_string(),
            start_time: result.started_at.unwrap_or(started_at),
            end_time: result.completed_at,
            success: result.status == "completed",
            error: result.error.clone(),
            input: result.input,
            output: result.output,
        })
        .collect();
    records.sort_by_key(|r| r.start_time);
    Ok(records)
}

