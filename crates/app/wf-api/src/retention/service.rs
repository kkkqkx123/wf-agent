//! Retention sweep and user-facing archive operation.
//!
//! One sweep covers both execution checkpoints and the records that keep an
//! execution visible. Preserved executions are skipped before any destructive
//! call; archived executions keep their history until the history window
//! expires, then their records and bundles are removed together.

use serde::Serialize;
use wf_checkpoint::{
    AgentCheckpointStateManager, CheckpointStateManager, CleanupStrategy,
    WorkflowCheckpointStateManager,
};
use wf_storage::adapter::BaseStorageAdapter;
use wf_types::config::RetentionConfig;

use crate::checkpoint::record::{delete_checkpoints_by_entity, list_checkpoints_by_entity};
use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};
use crate::retention::lifecycle::{self, ExecutionLifecycle};

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct RetentionReport {
    pub scanned: usize,
    pub dry_run: bool,
    pub checkpoints_removed: u64,
    pub file_histories_discarded: u64,
    pub executions_archived: u64,
    pub executions_deleted: u64,
    pub tasks_removed: u64,
    pub trigger_executions_removed: u64,
    pub failures: Vec<String>,
}

#[derive(Debug, Clone)]
struct Candidate {
    id: String,
    started_at: i64,
    active: bool,
}

fn cutoffs(config: &RetentionConfig) -> (i64, i64, i64) {
    let now = crate::now();
    (
        now.saturating_sub(RetentionConfig::max_age_ms(config.checkpoints.max_age_days)),
        now.saturating_sub(RetentionConfig::max_age_ms(
            config.file_checkpoints.history_max_age_days,
        )),
        now.saturating_sub(RetentionConfig::max_age_ms(
            config.executions.history_retention_days,
        )),
    )
}

async fn candidates(ctx: &ApiContext, config: &RetentionConfig) -> ApiResult<Vec<Candidate>> {
    let mut rows = Vec::new();
    for record in crate::workflow::execution::list_executions(ctx, None).await? {
        rows.push(Candidate {
            id: record.id.to_string(),
            started_at: record.started_at,
            active: config.executions.protect_running
                && lifecycle::is_active_status(&record.status),
        });
    }
    for record in crate::agent::list_agent_loops(&ctx.storage, None).await? {
        rows.push(Candidate {
            id: record.id.to_string(),
            started_at: record.started_at,
            active: config.executions.protect_running
                && matches!(record.status.as_str(), "running" | "paused"),
        });
    }
    rows.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    Ok(rows)
}

async fn entity_type(ctx: &ApiContext, execution_id: &str) -> ApiResult<&'static str> {
    if ctx
        .storage
        .workflow_execution
        .load(execution_id)
        .await?
        .is_some()
    {
        Ok("workflow_execution")
    } else {
        Ok("agent_loop")
    }
}

/// Count and optionally execute the shared cleanup strategies. Both paths use
/// the state manager so delta-chain protection stays in one place.
async fn prune_checkpoints(
    ctx: &ApiContext,
    execution_id: &str,
    config: &RetentionConfig,
    checkpoint_cutoff: i64,
    dry_run: bool,
) -> ApiResult<u64> {
    let kind = entity_type(ctx, execution_id).await?;
    let rows = list_checkpoints_by_entity(&ctx.storage, execution_id, kind).await?;
    let expired = rows
        .iter()
        .filter(|row| row.timestamp < checkpoint_cutoff)
        .count() as u64;
    let over_count = rows
        .len()
        .saturating_sub(config.checkpoints.max_checkpoints as usize) as u64;
    if dry_run {
        return Ok(expired.max(over_count));
    }

    let max_age_secs = RetentionConfig::max_age_ms(config.checkpoints.max_age_days) / 1000;
    let strategies = [
        CleanupStrategy::CountBased {
            max_checkpoints: config.checkpoints.max_checkpoints as u64,
            min_retention: 1,
        },
        CleanupStrategy::TimeBased {
            max_age_seconds: max_age_secs as u64,
            min_retention: 1,
        },
    ];
    let mut removed = 0u64;
    if kind == "workflow_execution" {
        let manager = WorkflowCheckpointStateManager::new(ctx.checkpoint_store.clone());
        for strategy in &strategies {
            removed += manager
                .cleanup_with_strategy(execution_id, strategy)
                .await
                .map_err(|e| ApiError::execution(format!("checkpoint cleanup failed: {e}")))?;
        }
    } else {
        let manager = AgentCheckpointStateManager::new(ctx.checkpoint_store.clone());
        for strategy in &strategies {
            removed += manager
                .cleanup_with_strategy(execution_id, strategy)
                .await
                .map_err(|e| ApiError::execution(format!("checkpoint cleanup failed: {e}")))?;
        }
    }
    Ok(removed)
}

async fn delete_checkpoints_for_execution(ctx: &ApiContext, execution_id: &str) -> ApiResult<u64> {
    let mut removed = 0u64;
    for kind in ["workflow_execution", "agent_loop"] {
        removed += delete_checkpoints_by_entity(&ctx.storage, execution_id, kind).await?;
    }
    Ok(removed)
}

async fn discard_file_history(ctx: &ApiContext, execution_id: &str) -> ApiResult<bool> {
    let Some(manager) = ctx.file_checkpoint_manager() else {
        return Ok(false);
    };
    manager
        .discard_execution_history(execution_id)
        .map_err(|e| ApiError::execution(format!("file history cleanup failed: {e}")))?;
    Ok(true)
}

/// Export a full snapshot, remove every checkpoint chain and release file
/// history refs while keeping the execution record and its history.
pub async fn archive_execution(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<ExecutionLifecycle> {
    if let Ok(status) = ctx.live_execution_status(execution_id) {
        let active = wf_types::ExecutionStatus::from_wire(status.as_str())
            .is_ok_and(|status| lifecycle::is_active_status(&status));
        if active {
            return Err(ApiError::Conflict(format!(
                "execution '{execution_id}' is live; cancel it before archiving"
            )));
        }
    }
    let digest = lifecycle::export_snapshot(ctx, execution_id).await?;
    delete_checkpoints_for_execution(ctx, execution_id).await?;
    let discarded = discard_file_history(ctx, execution_id).await?;
    if discarded {
        crate::checkpoint::provenance::run_gc(ctx, 0)?;
    }
    lifecycle::mark_archived(ctx, execution_id, digest).await
}

async fn delete_execution_data(ctx: &ApiContext, execution_id: &str) -> ApiResult<()> {
    if ctx
        .storage
        .workflow_execution
        .load(execution_id)
        .await?
        .is_some()
    {
        crate::workflow::delete_execution_full(ctx, execution_id).await?;
    } else {
        crate::agent::delete_agent_loop(&ctx.storage, execution_id).await?;
        crate::agent::delete_agent_execution(&ctx.storage, execution_id).await?;
    }
    delete_checkpoints_for_execution(ctx, execution_id).await?;
    discard_file_history(ctx, execution_id).await?;
    lifecycle::remove(ctx, execution_id).await
}

/// Run one retention sweep. `dry_run` computes the same candidates without
/// writing snapshots or deleting records.
pub async fn scan(ctx: &ApiContext, dry_run: bool) -> ApiResult<RetentionReport> {
    let config = super::settings::current(ctx).await?;
    if !config.enabled && !dry_run {
        return Err(ApiError::Validation(
            "automatic retention is disabled; enable it before applying cleanup".to_string(),
        ));
    }
    let (checkpoint_cutoff, file_cutoff, history_cutoff) = cutoffs(&config);
    // Checkpoint rows and file history are released together; the earlier
    // window decides when an execution becomes archived.
    let archive_cutoff = checkpoint_cutoff.min(file_cutoff);
    let all = candidates(ctx, &config).await?;
    let mut report = RetentionReport {
        scanned: all.len(),
        dry_run,
        ..RetentionReport::default()
    };
    let mut file_histories_dirty = false;

    for candidate in all.into_iter().take(config.batch_size) {
        let lifecycle = match lifecycle::get(ctx, &candidate.id).await {
            Ok(entry) => entry,
            Err(err) => {
                report.failures.push(format!("{}: {err}", candidate.id));
                continue;
            }
        };
        if config.checkpoints.protect_favorites
            && lifecycle
                .as_ref()
                .is_some_and(|entry| entry.state == lifecycle::ExecutionLifecycleState::Preserved)
        {
            continue;
        }
        if candidate.active {
            continue;
        }

        if lifecycle
            .as_ref()
            .is_some_and(|entry| entry.state == lifecycle::ExecutionLifecycleState::Archived)
        {
            let archived_at = lifecycle
                .as_ref()
                .and_then(|entry| entry.archived_at_ms)
                .unwrap_or(candidate.started_at);
            if archived_at >= history_cutoff {
                continue;
            }
            report.executions_deleted += 1;
            if !dry_run {
                if let Err(err) = delete_execution_data(ctx, &candidate.id).await {
                    report.failures.push(format!("{}: {err}", candidate.id));
                }
            }
            continue;
        }

        if candidate.started_at < archive_cutoff {
            report.executions_archived += 1;
            if !dry_run {
                match archive_execution(ctx, &candidate.id).await {
                    Ok(_) => {
                        file_histories_dirty = true;
                    }
                    Err(err) => report.failures.push(format!("{}: {err}", candidate.id)),
                }
            }
            continue;
        }

        match prune_checkpoints(ctx, &candidate.id, &config, checkpoint_cutoff, dry_run).await {
            Ok(removed) => report.checkpoints_removed += removed,
            Err(err) => report.failures.push(format!("{}: {err}", candidate.id)),
        }
    }

    if file_histories_dirty {
        crate::checkpoint::provenance::run_gc(ctx, config.file_checkpoints.keep_recent_heads)?;
        report.file_histories_discarded = report.executions_archived;
    }

    let task_cutoff =
        crate::now().saturating_sub(RetentionConfig::max_age_ms(config.tasks.retention_days));
    let trigger_cutoff = crate::now().saturating_sub(RetentionConfig::max_age_ms(
        config.tasks.trigger_retention_days,
    ));
    if dry_run {
        report.tasks_removed = crate::entity::task::list_tasks(&ctx.storage, None)
            .await?
            .iter()
            .filter(|task| task.created_at < task_cutoff)
            .count() as u64;
        report.trigger_executions_removed =
            crate::trigger::execution::list_trigger_executions(&ctx.storage, None)
                .await?
                .iter()
                .filter(|row| row.triggered_at < trigger_cutoff)
                .count() as u64;
    } else {
        report.tasks_removed =
            crate::entity::task::cleanup_tasks(&ctx.storage, task_cutoff).await?;
        report.trigger_executions_removed =
            crate::trigger::execution::cleanup_old_trigger_executions(&ctx.storage, trigger_cutoff)
                .await?;
    }
    Ok(report)
}
