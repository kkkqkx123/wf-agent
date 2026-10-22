
use wf_execution_shared::types::state_manager::StateManager;
use wf_storage::adapter::base::BaseStorageAdapter;

use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};

use super::helpers::build_profile;
use super::types::*;

pub async fn profile(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<ExecutionPerformanceProfile> {
    if let Some(entity) = ctx.workflow_execution(execution_id) {
        let state = entity.state.read().await;
        let snapshot = state
            .create_snapshot()
            .await
            .map_err(|e| ApiError::execution(format!("state snapshot failed: {e}")))?;
        let mut records = snapshot.node_execution_history;
        records.sort_by_key(|r| r.start_time);

        // Execution span from the state boundaries; when the end is not
        // recorded yet, fall back to the latest node end time so the
        // timeline still yields a meaningful total.
        let total_duration_ms = snapshot
            .end_time
            .map(|end| (end - snapshot.start_time).max(0))
            .or_else(|| {
                records
                    .iter()
                    .filter_map(|r| r.end_time)
                    .max()
                    .map(|max_end| (max_end - snapshot.start_time).max(0))
            })
            .unwrap_or(0);
        let timeline: Vec<NodeTimelineEntry> = records
            .iter()
            .map(|record| {
                let duration = record
                    .end_time
                    .map(|end| (end - record.start_time).max(0))
                    .unwrap_or(0);
                let share = if total_duration_ms > 0 {
                    duration as f64 / total_duration_ms as f64
                } else {
                    0.0
                };
                NodeTimelineEntry {
                    node_id: record.node_id.clone(),
                    node_name: record.node_name.clone(),
                    node_type: record.node_type.clone(),
                    duration_ms: duration,
                    share,
                    success: record.success,
                }
            })
            .collect();

        let error_count = records.iter().filter(|r| !r.success).count() as u32;
        return Ok(build_profile(
            execution_id,
            Some(entity.workflow_id().to_string()),
            snapshot.status.as_str().to_string(),
            total_duration_ms,
            records.len() as u32,
            error_count,
            timeline,
        ));
    }

    let Some(record) = ctx.storage.workflow_execution.load(execution_id).await? else {
        return Ok(build_profile(
            execution_id,
            None,
            "unknown".to_string(),
            0,
            0,
            0,
            Vec::new(),
        ));
    };
    let total_duration_ms = record
        .completed_at
        .map(|end| (end - record.started_at).max(0))
        .unwrap_or(0);
    let node_count = record
        .node_results
        .as_ref()
        .map(|results| results.len() as u32)
        .unwrap_or(0);
    let error_count = record
        .errors
        .as_ref()
        .map(|errors| errors.len() as u32)
        .unwrap_or(0);
    Ok(build_profile(
        execution_id,
        Some(record.workflow_id.to_string()),
        record.status.as_str().to_string(),
        total_duration_ms,
        node_count,
        error_count,
        Vec::new(),
    ))
}

