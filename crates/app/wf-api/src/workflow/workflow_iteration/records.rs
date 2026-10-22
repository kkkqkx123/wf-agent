use std::collections::BTreeMap;


use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

use super::*;

pub async fn get_execution_node_analyses(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<ExtendedNodeExecutionRecordView>> {
    let mut by_node: BTreeMap<String, Vec<RawNodeAttempt>> = BTreeMap::new();
    for attempt in super::stats::raw_attempts(ctx, execution_id).await? {
        by_node
            .entry(attempt.node_id.clone())
            .or_default()
            .push(attempt);
    }

    let mut records: Vec<ExtendedNodeExecutionRecordView> = by_node
        .into_iter()
        .map(|(node_id, attempts)| collapse_node(execution_id, node_id, attempts))
        .collect();
    records.sort_by_key(|r| r.start_time);
    Ok(records)
}

/// Extended record of a specific node of an execution, or `None` when the
/// node was never executed.
pub async fn get_node_analysis(
    ctx: &ApiContext,
    execution_id: &str,
    node_id: &str,
) -> ApiResult<Option<ExtendedNodeExecutionRecordView>> {
    Ok(get_execution_node_analyses(ctx, execution_id)
        .await?
        .into_iter()
        .find(|record| record.node_id == node_id))
}

/// Tool dependencies of a node (inferred from the node type).
fn collapse_node(
    execution_id: &str,
    node_id: String,
    mut attempts: Vec<RawNodeAttempt>,
) -> ExtendedNodeExecutionRecordView {
    attempts.sort_by_key(|a| a.start_time);
    let retry_count = attempts.len().saturating_sub(1) as u32;
    let first = &attempts[0];
    let latest = attempts.last().expect("attempts is never empty");
    let status = if latest.end_time.is_none() && !latest.success {
        "running".to_string()
    } else if latest.success {
        "completed".to_string()
    } else if latest.error.is_some() {
        "failed".to_string()
    } else {
        "cancelled".to_string()
    };
    let end_time = attempts.iter().filter_map(|a| a.end_time).max();
    let duration = end_time
        .map(|end| (end - first.start_time).max(0))
        .or_else(|| latest.end_time.map(|end| (end - latest.start_time).max(0)));

    let tool_dependencies = if super::tool_dependencies::is_tool_node(&latest.node_type) {
        vec![ToolDependencyView {
            tool_name: latest.node_name.clone(),
            call_count: attempts.len() as u32,
        }]
    } else {
        Vec::new()
    };

    ExtendedNodeExecutionRecordView {
        execution_id: execution_id.to_string(),
        node_id,
        node_name: latest.node_name.clone(),
        node_type: latest.node_type.clone(),
        status,
        start_time: first.start_time,
        end_time,
        duration,
        input: latest.input.clone(),
        output: latest.output.clone(),
        retry_count,
        error: latest.error.clone(),
        tool_dependencies,
    }
}

/// Node types that carry an implicit tool execution dependency.
pub(super) fn filter_matches(
    record: &ExtendedNodeExecutionRecordView,
    filter: &ExtendedNodeExecutionFilter,
) -> bool {
    if let Some(node_id) = &filter.node_id {
        if &record.node_id != node_id {
            return false;
        }
    }
    if let Some(node_type) = &filter.node_type {
        if &record.node_type != node_type {
            return false;
        }
    }
    if let Some(status) = &filter.status {
        if &record.status != status {
            return false;
        }
    }
    if filter.has_errors && record.error.is_none() {
        return false;
    }
    if let Some(min_duration) = filter.min_duration {
        if record.duration.map(|d| d < min_duration).unwrap_or(true) {
            return false;
        }
    }
    if let Some((start, end)) = filter.time_range {
        if let Some(start) = start {
            if record.start_time < start {
                return false;
            }
        }
        if let Some(end) = end {
            if record.start_time > end {
                return false;
            }
        }
    }
    true
}

