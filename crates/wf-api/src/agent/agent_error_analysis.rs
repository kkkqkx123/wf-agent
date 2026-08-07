//! Error analysis scoped to agent loop executions (TS `AgentErrorAnalysisAPI`
//! counterpart). Reads the live agent loop error chains first and degrades to
//! the persisted `AgentExecution.error` text after a restart.

use std::collections::BTreeMap;

use serde::Serialize;

use wf_common::error_chain::ErrorRecord;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_types::enums::ErrorSeverity;
use wf_types::errors::RecoveryAction;
use wf_types::ExecutionStatus;

use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};

/// Serializable view of an error record (TS `ExecutionErrorRecord`).
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionErrorRecord {
    pub id: String,
    pub execution_id: String,
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_type: Option<String>,
    pub timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_error_id: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub error_chain: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub root_cause_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caused_by: Option<String>,
    pub is_recoverable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery_action: Option<String>,
}

/// Root cause analysis (TS `RootCauseAnalysis`).
#[derive(Debug, Clone, Serialize)]
pub struct RootCauseAnalysis {
    pub root_cause_id: String,
    pub error: String,
    pub chain_length: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_action: Option<String>,
}

/// Error statistics of an agent loop (TS `ErrorStatistics`).
#[derive(Debug, Clone, Default, Serialize)]
pub struct AgentErrorStatistics {
    pub execution_id: String,
    pub total: u32,
    pub by_type: BTreeMap<String, u64>,
    pub by_severity: BTreeMap<ErrorSeverity, u64>,
    pub recoverable: u64,
    pub root_cause: Option<String>,
}

/// Advanced error analysis of an agent loop (TS `AdvancedErrorAnalysis`).
#[derive(Debug, Clone, Serialize)]
pub struct AdvancedErrorAnalysis {
    pub execution_id: String,
    pub total_errors: u32,
    pub root_cause: Option<String>,
    pub first_error_timestamp: Option<i64>,
    pub last_error_timestamp: Option<i64>,
    pub error_types: BTreeMap<String, u64>,
    pub recurring: bool,
}

/// Recovery proposal for a specific error (TS `ErrorRecoveryProposal`).
#[derive(Debug, Clone, Serialize)]
pub struct ErrorRecoveryProposal {
    pub error_id: String,
    pub recovery_action: String,
    pub is_recoverable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caused_by: Option<String>,
}

/// Error records of an agent loop, oldest first.
pub async fn get_execution_error_records(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<ExecutionErrorRecord>> {
    let records = error_records(ctx, execution_id).await?;
    Ok(records.iter().map(record_view).collect())
}

/// Error chain of an agent loop, optionally starting from a specific
/// error id. `None` selects the full chain.
pub async fn get_error_chain(
    ctx: &ApiContext,
    execution_id: &str,
    from_error_id: Option<&str>,
) -> ApiResult<Vec<ExecutionErrorRecord>> {
    let records = error_records(ctx, execution_id).await?;
    let start = from_error_id
        .and_then(|id| records.iter().position(|r| r.id == id))
        .unwrap_or(0);
    Ok(records[start..].iter().map(record_view).collect())
}

/// Root cause analysis of an agent loop's error chain.
pub async fn analyze_root_cause(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<RootCauseAnalysis> {
    let records = error_records(ctx, execution_id).await?;
    if records.is_empty() {
        return Err(ApiError::execution(format!(
            "no error records for agent loop {execution_id}"
        )));
    }
    let root = records
        .iter()
        .find(|r| r.root_cause_id == r.id || r.parent_error_id.is_none())
        .unwrap_or(&records[0]);
    Ok(RootCauseAnalysis {
        root_cause_id: root.id.clone(),
        error: root.error.clone(),
        chain_length: records.len(),
        suggested_action: root
            .recovery_action
            .as_ref()
            .map(action_name)
            .or_else(|| root.is_recoverable.then(|| "retry".to_string())),
    })
}

/// Error statistics of an agent loop.
pub async fn get_error_statistics(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<AgentErrorStatistics> {
    let records = error_records(ctx, execution_id).await?;
    let mut stats = AgentErrorStatistics {
        execution_id: execution_id.to_string(),
        ..AgentErrorStatistics::default()
    };
    stats.total = records.len() as u32;
    for record in &records {
        let type_name = record
            .error_type
            .as_ref()
            .map(|t| format!("{t:?}"))
            .unwrap_or_else(|| "Unknown".to_string());
        *stats.by_type.entry(type_name).or_insert(0) += 1;
        let severity = if record.is_recoverable {
            ErrorSeverity::Warning
        } else {
            ErrorSeverity::Critical
        };
        *stats.by_severity.entry(severity).or_insert(0) += 1;
        if record.is_recoverable {
            stats.recoverable += 1;
        }
    }
    stats.root_cause = records
        .iter()
        .find(|r| r.root_cause_id == r.id || r.parent_error_id.is_none())
        .map(|r| r.error.clone())
        .or_else(|| records.first().map(|r| r.error.clone()));
    Ok(stats)
}

/// Advanced error analysis of an agent loop.
pub async fn get_advanced_error_analysis(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<AdvancedErrorAnalysis> {
    let records = error_records(ctx, execution_id).await?;
    let mut error_types: BTreeMap<String, u64> = BTreeMap::new();
    let mut first = None;
    let mut last = None;
    for record in &records {
        let type_name = record
            .error_type
            .as_ref()
            .map(|t| format!("{t:?}"))
            .unwrap_or_else(|| "Unknown".to_string());
        *error_types.entry(type_name).or_insert(0) += 1;
        first = Some(first.map_or(record.timestamp, |f: i64| f.min(record.timestamp)));
        last = Some(last.map_or(record.timestamp, |l: i64| l.max(record.timestamp)));
    }
    let recurring = records.len() > 1;
    Ok(AdvancedErrorAnalysis {
        execution_id: execution_id.to_string(),
        total_errors: records.len() as u32,
        root_cause: records
            .iter()
            .find(|r| r.root_cause_id == r.id || r.parent_error_id.is_none())
            .map(|r| r.error.clone())
            .or_else(|| records.first().map(|r| r.error.clone())),
        first_error_timestamp: first,
        last_error_timestamp: last,
        error_types,
        recurring,
    })
}

/// Recovery proposal for a specific error id, or `None` when the error is
/// unknown or unrecoverable.
pub async fn get_recovery_proposal(
    ctx: &ApiContext,
    execution_id: &str,
    error_id: &str,
) -> ApiResult<Option<ErrorRecoveryProposal>> {
    let records = error_records(ctx, execution_id).await?;
    let Some(record) = records.iter().find(|r| r.id == error_id) else {
        return Ok(None);
    };
    if !record.is_recoverable && record.recovery_action.is_none() {
        return Ok(None);
    }
    Ok(Some(ErrorRecoveryProposal {
        error_id: record.id.clone(),
        recovery_action: record
            .recovery_action
            .as_ref()
            .map(action_name)
            .unwrap_or_else(|| "retry".to_string()),
        is_recoverable: record.is_recoverable,
        caused_by: record.caused_by.as_ref().map(|c| c.reason.clone()),
    }))
}

/// Errors similar to an error of this agent loop across all persisted
/// failed agent executions, clustered by normalized message.
pub async fn get_similar_errors(
    ctx: &ApiContext,
    execution_id: &str,
    error_id: &str,
) -> ApiResult<Vec<ExecutionErrorRecord>> {
    let records = error_records(ctx, execution_id).await?;
    let Some(target) = records.iter().find(|r| r.id == error_id) else {
        return Ok(Vec::new());
    };
    let target_normalized = normalize_message(&target.error);

    let mut similar = Vec::new();
    let persisted = ctx.storage.agent_execution.list(None).await?;
    for record in persisted {
        if record.id != execution_id && record.status == ExecutionStatus::Failed {
            if let Some(error) = &record.error {
                if normalize_message(error) == target_normalized {
                    similar.push(record_view(&ErrorRecord {
                        id: wf_common::generate_id(),
                        execution_id: record.id.to_string(),
                        error: error.clone(),
                        error_type: None,
                        timestamp: record.completed_at.unwrap_or(record.started_at),
                        node_id: None,
                        parent_error_id: None,
                        error_chain: Vec::new(),
                        root_cause_id: String::new(),
                        caused_by: None,
                        is_recoverable: false,
                        recovery_action: None,
                    }));
                }
            }
        }
    }
    similar.sort_by_key(|r| std::cmp::Reverse(r.timestamp));
    similar.truncate(20);
    Ok(similar)
}

async fn error_records(ctx: &ApiContext, execution_id: &str) -> ApiResult<Vec<ErrorRecord>> {
    if let Some(entity) = ctx.agent_loop(execution_id) {
        return Ok(entity.state.read().await.error_records().to_vec());
    }
    let record = ctx
        .storage
        .agent_execution
        .load(execution_id)
        .await?
        .ok_or_else(|| ApiError::execution_not_found(execution_id))?;
    let mut records = Vec::new();
    if let Some(error) = &record.error {
        records.push(ErrorRecord {
            id: wf_common::generate_id(),
            execution_id: execution_id.to_string(),
            error: error.clone(),
            error_type: None,
            timestamp: record.completed_at.unwrap_or(record.started_at),
            node_id: None,
            parent_error_id: None,
            error_chain: Vec::new(),
            root_cause_id: String::new(),
            caused_by: None,
            is_recoverable: false,
            recovery_action: None,
        });
    }
    Ok(records)
}

fn record_view(record: &ErrorRecord) -> ExecutionErrorRecord {
    ExecutionErrorRecord {
        id: record.id.clone(),
        execution_id: record.execution_id.clone(),
        error: record.error.clone(),
        error_type: record.error_type.as_ref().map(|t| format!("{t:?}")),
        timestamp: record.timestamp,
        node_id: record.node_id.clone(),
        parent_error_id: record.parent_error_id.clone(),
        error_chain: record.error_chain.clone(),
        root_cause_id: record.root_cause_id.clone(),
        caused_by: record.caused_by.as_ref().map(|c| c.reason.clone()),
        is_recoverable: record.is_recoverable,
        recovery_action: record.recovery_action.as_ref().map(action_name),
    }
}

fn action_name(action: &RecoveryAction) -> String {
    match action {
        RecoveryAction::Retry => "retry".to_string(),
        RecoveryAction::Fallback => "fallback".to_string(),
        RecoveryAction::ManualIntervention => "manual_intervention".to_string(),
        RecoveryAction::Abort => "abort".to_string(),
    }
}

/// Normalize an error message for similarity clustering.
fn normalize_message(message: &str) -> String {
    let trimmed = message.trim().to_lowercase();
    let trimmed = trimmed
        .strip_suffix(")")
        .map(|s| s.rsplit_once("(").map(|(head, _)| head).unwrap_or(s))
        .unwrap_or(&trimmed);
    trimmed.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_agent::entity::AgentLoopEntity;
    use wf_common::error_chain::ErrorRecord;
    use wf_resource::registrar::Registries;
    use wf_resource::starter::BundleRegistry;
    use wf_storage::context::StorageContext;
    use wf_types::errors::{ErrorCause, ErrorType};
    use wf_types::Id;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(Registries::new()),
            Arc::new(BundleRegistry::new()),
        ))
    }

    fn make_record(execution_id: &str, message: &str) -> ErrorRecord {
        ErrorRecord {
            id: wf_common::generate_id(),
            execution_id: execution_id.to_string(),
            error: message.to_string(),
            error_type: Some(ErrorType::ToolError),
            timestamp: wf_common::now(),
            node_id: None,
            parent_error_id: None,
            error_chain: Vec::new(),
            root_cause_id: String::new(),
            caused_by: Some(ErrorCause {
                reason: "root".to_string(),
                handling_attempt: None,
            }),
            is_recoverable: true,
            recovery_action: Some(RecoveryAction::Retry),
        }
    }

    #[tokio::test]
    async fn records_chain_root_cause_and_stats() {
        let ctx = make_ctx();
        let entity = Arc::new(AgentLoopEntity::new(Id::from("exec-e".to_string())));
        let record = make_record("exec-e", "tool boom");
        entity.state.write().await.record_error(record.clone());
        ctx.agent_loops.register(entity);

        let records = get_execution_error_records(&ctx, "exec-e").await.unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].error, "tool boom");

        let chain = get_error_chain(&ctx, "exec-e", None).await.unwrap();
        assert_eq!(chain.len(), 1);

        let root = analyze_root_cause(&ctx, "exec-e").await.unwrap();
        assert_eq!(root.error, "tool boom");
        assert_eq!(root.chain_length, 1);
        assert_eq!(root.suggested_action.as_deref(), Some("retry"));

        let stats = get_error_statistics(&ctx, "exec-e").await.unwrap();
        assert_eq!(stats.total, 1);
        assert_eq!(stats.recoverable, 1);
        assert!(stats.by_type.contains_key("ToolError"));

        let advanced = get_advanced_error_analysis(&ctx, "exec-e").await.unwrap();
        assert_eq!(advanced.total_errors, 1);
        assert_eq!(advanced.root_cause.as_deref(), Some("tool boom"));

        let proposal = get_recovery_proposal(&ctx, "exec-e", &record.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(proposal.recovery_action, "retry");
    }

    #[tokio::test]
    async fn degrades_to_persisted_error_text() {
        let ctx = make_ctx();
        let record = wf_types::AgentExecution {
            id: Id::from("exec-p".to_string()),
            definition_id: Id::from("agent-p".to_string()),
            status: ExecutionStatus::Failed,
            current_iteration: 1,
            tool_call_count: 0,
            iteration_history: None,
            started_at: wf_common::now(),
            completed_at: Some(wf_common::now()),
            error: Some("fatal timeout".to_string()),
            context: None,
        };
        ctx.storage.agent_execution.save(&record).await.unwrap();

        let records = get_execution_error_records(&ctx, "exec-p").await.unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].error, "fatal timeout");

        let stats = get_error_statistics(&ctx, "exec-p").await.unwrap();
        assert_eq!(stats.total, 1);
    }

    #[tokio::test]
    async fn unknown_execution_is_not_found() {
        let ctx = make_ctx();
        let err = get_execution_error_records(&ctx, "missing")
            .await
            .unwrap_err();
        assert!(matches!(err, ApiError::ExecutionNotFound { .. }));
    }
}
