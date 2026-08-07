//! Error analysis over live entity error chains and persisted execution
//! records. Lives on top of `wf-common::error_chain::ErrorRecord` (the shape
//! the workflow/agent engines record on failure) and the `FailurePolicyManager`
//! recovery semantics exposed through `RecoveryAction`.

use std::collections::{BTreeMap, HashMap};
use std::pin::Pin;

use serde::Serialize;

use futures::Stream;
use wf_common::error_chain::ErrorRecord;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_types::enums::{ErrorSeverity, ErrorTrend};
use wf_types::errors::RecoveryAction;
use wf_types::events::EventType;
use wf_types::ExecutionStatus;

use crate::agent::agent_error_analysis::ExecutionErrorRecord;
use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};
use crate::infra::util::round2;

/// Error hotspot: a node where errors concentrate (TS `WorkflowErrorHotspot`).
#[derive(Debug, Clone, Serialize)]
pub struct WorkflowErrorHotspot {
    pub node_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_name: Option<String>,
    pub error_count: u64,
    pub error_types: Vec<String>,
    pub severity: ErrorSeverity,
}

/// A problematic node of a workflow execution (TS `ProblematicNode`).
#[derive(Debug, Clone, Serialize)]
pub struct ProblematicNode {
    pub node_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_name: Option<String>,
    pub error_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_type: Option<String>,
}

/// Advanced workflow error analysis with frequency, hotspots and trends (TS
/// `AdvancedWorkflowErrorAnalysis`).
#[derive(Debug, Clone, Serialize)]
pub struct AdvancedWorkflowErrorAnalysis {
    pub execution_id: String,
    pub total_errors: u32,
    pub error_frequency: BTreeMap<String, u64>,
    pub error_hotspots: Vec<WorkflowErrorHotspot>,
    /// `none` | `steady` | `accelerating` | `decelerating`.
    pub temporal_pattern: String,
    pub most_problematic_nodes: Vec<ProblematicNode>,
    /// `increasing` | `decreasing` | `stable`.
    pub error_trend: ErrorTrend,
}

/// Reference to a workflow node affected by an error.
#[derive(Debug, Clone, Serialize)]
pub struct WorkflowNodeRef {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Workflow recovery proposal for a specific error (TS `WorkflowRecoveryProposal`).
#[derive(Debug, Clone, Serialize)]
pub struct RecoveryProposal {
    pub error_id: String,
    /// `retry` | `fallback` | `skip` | `abort`.
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub affected_node: Option<WorkflowNodeRef>,
    pub reason: String,
    /// Estimated success likelihood (0.0 - 1.0).
    pub likelihood: f64,
    pub steps: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_time_to_recover: Option<i64>,
}

/// Handle returned by [`subscribe_to_errors`]; dropping it
/// stops the subscription.
pub struct ErrorSubscription {
    handle: tokio::task::AbortHandle,
}

impl Drop for ErrorSubscription {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

/// Aggregate error statistics of one workflow execution (TS
/// `WorkflowErrorAnalysisAPI` counterpart).
#[derive(Debug, Clone, Default, Serialize)]
pub struct WorkflowErrorStats {
    pub execution_id: String,
    pub total: u32,
    pub by_type: BTreeMap<String, u64>,
    pub by_node: BTreeMap<String, u64>,
    pub by_severity: BTreeMap<ErrorSeverity, u64>,
    pub recoverable: u64,
    pub root_cause: Option<String>,
}

/// A concrete recovery recommendation derived from an error record.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorRecommendation {
    pub execution_id: String,
    pub error: String,
    pub node_id: Option<String>,
    pub recovery_action: String,
    pub timestamp: i64,
}

/// Errors clustered by normalized message across executions.
#[derive(Debug, Clone, Serialize)]
pub struct SimilarErrorGroup {
    pub message: String,
    pub count: u64,
    pub executions: Vec<String>,
    pub nodes: Vec<String>,
}

/// Error statistics of a workflow execution.
pub async fn workflow_error_stats(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<WorkflowErrorStats> {
    let records = workflow_error_records(ctx, execution_id).await?;
    let mut stats = WorkflowErrorStats {
        execution_id: execution_id.to_string(),
        ..WorkflowErrorStats::default()
    };
    stats.total = records.len() as u32;
    for record in &records {
        let type_name = record
            .error_type
            .as_ref()
            .map(|t| format!("{t:?}"))
            .unwrap_or_else(|| "Unknown".to_string());
        *stats.by_type.entry(type_name).or_insert(0) += 1;

        let node = record
            .node_id
            .clone()
            .unwrap_or_else(|| "<unknown>".to_string());
        *stats.by_node.entry(node).or_insert(0) += 1;

        let severity = severity_of(record);
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

/// Recovery recommendations of a workflow execution (one per error
/// record carrying a recovery action).
pub async fn recovery_recommendations(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<ErrorRecommendation>> {
    let records = workflow_error_records(ctx, execution_id).await?;
    Ok(records
        .iter()
        .filter_map(|record| {
            let action = match &record.recovery_action {
                Some(action) => action_name(action),
                None if record.is_recoverable => "retry".to_string(),
                None => return None,
            };
            Some(ErrorRecommendation {
                execution_id: record.execution_id.clone(),
                error: record.error.clone(),
                node_id: record.node_id.clone(),
                recovery_action: action,
                timestamp: record.timestamp,
            })
        })
        .collect())
}

/// Errors similar to this execution's errors across all persisted
/// workflow executions, clustered by normalized error message.
pub async fn similar_errors(
    ctx: &ApiContext,
    execution_id: &str,
    limit: usize,
) -> ApiResult<Vec<SimilarErrorGroup>> {
    let records = workflow_error_records(ctx, execution_id).await?;
    if records.is_empty() {
        return Ok(Vec::new());
    }
    let query_messages: Vec<String> = records
        .iter()
        .map(|r| normalize_message(&r.error))
        .collect();

    let mut clusters: HashMap<String, SimilarErrorGroup> = HashMap::new();
    let mut others = ctx.storage.workflow_execution.list(None).await?;
    others.retain(|e| e.id != execution_id && e.status == ExecutionStatus::Failed);
    for execution in &others {
        let messages = execution
            .errors
            .clone()
            .unwrap_or_default()
            .into_iter()
            .chain(execution.error.clone());
        for message in messages {
            let normalized = normalize_message(&message);
            if query_messages.contains(&normalized) {
                let group =
                    clusters
                        .entry(normalized.clone())
                        .or_insert_with(|| SimilarErrorGroup {
                            message: normalized.clone(),
                            count: 0,
                            executions: Vec::new(),
                            nodes: Vec::new(),
                        });
                group.count += 1;
                if !group.executions.contains(&execution.id) {
                    group.executions.push(execution.id.clone());
                }
            }
        }
    }
    let mut groups: Vec<SimilarErrorGroup> = clusters.into_values().collect();
    groups.sort_by_key(|group| std::cmp::Reverse(group.count));
    groups.truncate(if limit == 0 { 20 } else { limit });
    Ok(groups)
}

async fn workflow_error_records(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<ErrorRecord>> {
    if let Some(entity) = ctx.workflow_execution(execution_id) {
        return Ok(entity.state.read().await.error_records().to_vec());
    }
    let record = ctx
        .storage
        .workflow_execution
        .load(execution_id)
        .await?
        .ok_or_else(|| ApiError::execution_not_found(execution_id))?;
    // Persisted boundary keeps only plain error strings; build minimal
    // records so the analysis stays available after the entity is gone.
    let mut records = Vec::new();
    if let Some(error) = &record.error {
        records.push(minimal_record(execution_id, error, None));
    }
    if let Some(errors) = &record.errors {
        for error in errors {
            records.push(minimal_record(execution_id, error, None));
        }
    }
    Ok(records)
}

/// Error chain of a workflow execution from the root cause up to and
/// including the given error id (or the last error when omitted) (TS
/// `getErrorChain`).
pub async fn get_error_chain(
    ctx: &ApiContext,
    execution_id: &str,
    from_error_id: Option<&str>,
) -> ApiResult<Vec<ExecutionErrorRecord>> {
    let records = workflow_error_records(ctx, execution_id).await?;
    if records.is_empty() {
        return Ok(Vec::new());
    }
    let target = from_error_id
        .and_then(|id| records.iter().find(|r| r.id == id))
        .or_else(|| records.last())
        .ok_or_else(|| ApiError::execution_not_found(execution_id))?;
    let chain_ids: Vec<&str> = target.error_chain.iter().map(String::as_str).collect();
    let mut chain: Vec<ExecutionErrorRecord> = records
        .iter()
        .filter(|r| chain_ids.contains(&r.id.as_str()))
        .map(workflow_record_view)
        .collect();
    chain.sort_by_key(|r| r.timestamp);
    Ok(chain)
}

/// Advanced error analysis of a workflow execution: frequency by type,
/// node hotspots, temporal pattern and trend (TS `getAdvancedErrorAnalysis`).
pub async fn get_advanced_error_analysis(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<AdvancedWorkflowErrorAnalysis> {
    let records = workflow_error_records(ctx, execution_id).await?;
    if records.is_empty() {
        return Ok(AdvancedWorkflowErrorAnalysis {
            execution_id: execution_id.to_string(),
            total_errors: 0,
            error_frequency: BTreeMap::new(),
            error_hotspots: Vec::new(),
            temporal_pattern: "none".to_string(),
            most_problematic_nodes: Vec::new(),
            error_trend: ErrorTrend::Stable,
        });
    }

    let mut error_frequency: BTreeMap<String, u64> = BTreeMap::new();
    let mut node_problems: BTreeMap<String, (u64, Vec<String>, Vec<String>, ErrorSeverity)> =
        BTreeMap::new();
    let mut sorted = records.clone();
    sorted.sort_by_key(|r| r.timestamp);

    for record in &sorted {
        let type_name = record
            .error_type
            .as_ref()
            .map(|t| format!("{t:?}"))
            .unwrap_or_else(|| "Unknown".to_string());
        *error_frequency.entry(type_name.clone()).or_insert(0) += 1;

        if let Some(node_id) = &record.node_id {
            let entry = node_problems
                .entry(node_id.clone())
                .or_insert_with(|| (0, Vec::new(), Vec::new(), severity_of(record)));
            entry.0 += 1;
            if !entry.1.contains(&type_name) {
                entry.1.push(type_name);
            }
            let node_name = node_name(ctx, execution_id, node_id).await;
            if let Some(name) = node_name {
                if !entry.2.contains(&name) {
                    entry.2.push(name);
                }
            }
            if severity_rank(severity_of(record)) > severity_rank(entry.3) {
                entry.3 = severity_of(record);
            }
        }
    }

    let mut hotspots: Vec<WorkflowErrorHotspot> = node_problems
        .into_iter()
        .map(
            |(node_id, (count, types, names, severity))| WorkflowErrorHotspot {
                node_id,
                node_name: names.first().cloned(),
                error_count: count,
                error_types: types,
                severity,
            },
        )
        .collect();
    hotspots.sort_by_key(|h| std::cmp::Reverse(h.error_count));

    let most_problematic_nodes: Vec<ProblematicNode> = hotspots
        .iter()
        .take(5)
        .map(|h| ProblematicNode {
            node_id: h.node_id.clone(),
            node_name: h.node_name.clone(),
            error_count: h.error_count,
            node_type: None,
        })
        .collect();

    let temporal_pattern = analyze_temporal_pattern(&sorted);
    let error_trend = analyze_error_trend(&sorted);

    Ok(AdvancedWorkflowErrorAnalysis {
        execution_id: execution_id.to_string(),
        total_errors: records.len() as u32,
        error_frequency,
        error_hotspots: hotspots.into_iter().take(10).collect(),
        temporal_pattern,
        most_problematic_nodes,
        error_trend,
    })
}

/// Recovery proposal for a specific error of a workflow execution (TS
/// `getRecoveryProposal`).
pub async fn get_recovery_proposal(
    ctx: &ApiContext,
    execution_id: &str,
    error_id: &str,
) -> ApiResult<Option<RecoveryProposal>> {
    let records = workflow_error_records(ctx, execution_id).await?;
    let Some(record) = records.iter().find(|r| r.id == error_id) else {
        return Ok(None);
    };
    let action = match &record.recovery_action {
        Some(action) => action_name(action),
        None => suggest_action(record),
    };
    let likelihood = estimate_likelihood(record, &action);
    let steps = recovery_steps(&action);
    let affected_node = match record.node_id.as_ref() {
        Some(node_id) => Some(WorkflowNodeRef {
            id: node_id.clone(),
            name: node_name(ctx, execution_id, node_id).await,
        }),
        None => None,
    };
    let reason = record
        .caused_by
        .as_ref()
        .map(|c| c.reason.clone())
        .unwrap_or_else(|| format!("error '{}' triggers {action}", record.error));

    Ok(Some(RecoveryProposal {
        error_id: record.id.clone(),
        action: action.clone(),
        affected_node,
        reason,
        likelihood: round2(likelihood),
        steps,
        estimated_time_to_recover: estimate_recovery_time(&action),
    }))
}

/// Stream the error chain of a workflow execution one record at a time,
/// starting from the root cause (TS `streamErrorChain`).
pub async fn stream_error_chain(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Pin<Box<dyn Stream<Item = ExecutionErrorRecord> + Send>>> {
    let records = workflow_error_records(ctx, execution_id).await?;
    let mut sorted: Vec<ErrorRecord> = records;
    sorted.sort_by_key(|r| r.timestamp);
    let mut root_first: Vec<ExecutionErrorRecord> = Vec::with_capacity(sorted.len());
    if let Some(root) = sorted
        .iter()
        .find(|r| r.parent_error_id.is_none() || r.root_cause_id == r.id)
    {
        root_first.push(workflow_record_view(root));
        for record in &sorted {
            if record.id != root.id && record.error_chain.contains(&root.id) {
                root_first.push(workflow_record_view(record));
            }
        }
    }
    for record in &sorted {
        if !root_first.iter().any(|r| r.id == record.id) {
            root_first.push(workflow_record_view(record));
        }
    }
    Ok(Box::pin(futures::stream::iter(root_first)))
}

/// Subscribe to error events of a workflow execution published on the
/// shared event bus (TS `subscribeToErrors`). The returned guard aborts
/// the subscription when dropped.
pub fn subscribe_to_errors<F>(
    ctx: &ApiContext,
    execution_id: &str,
    callback: F,
) -> ErrorSubscription
where
    F: Fn(ExecutionErrorRecord) + Send + Sync + 'static,
{
    let bus = ctx.event_bus.clone();
    let mut subscription = bus.subscribe();
    let filter = execution_id.to_string();
    let handle = tokio::spawn(async move {
        while let Ok(event) = subscription.recv().await {
            if event.execution_id.as_deref() != Some(filter.as_str()) {
                continue;
            }
            if event.r#type != EventType::Error {
                continue;
            }
            let message = event
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("message"))
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned)
                .or_else(|| {
                    event
                        .metadata
                        .as_ref()
                        .and_then(|metadata| metadata.get("error"))
                        .and_then(|value| value.as_str())
                        .map(ToOwned::to_owned)
                })
                .unwrap_or_else(|| "workflow execution error".to_string());
            let node_id = event
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("node_id"))
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned);
            let error_type = event
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("error_type"))
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned);
            callback(ExecutionErrorRecord {
                id: event.id.to_string(),
                execution_id: filter.clone(),
                error: message,
                error_type,
                timestamp: event.timestamp,
                node_id,
                parent_error_id: None,
                error_chain: Vec::new(),
                root_cause_id: String::new(),
                caused_by: None,
                is_recoverable: false,
                recovery_action: None,
            });
        }
    });
    ErrorSubscription {
        handle: handle.abort_handle(),
    }
}

async fn node_name(ctx: &ApiContext, execution_id: &str, node_id: &str) -> Option<String> {
    if let Ok(Some(record)) = ctx.storage.workflow_execution.load(execution_id).await {
        if let Some(graph) = &record.graph {
            return graph
                .nodes
                .iter()
                .find(|n| n.id == node_id)
                .and_then(|n| n.name.clone());
        }
    }
    None
}

/// Reuse the engine's structured error chains if a live entity is around,
/// otherwise degrade to the persisted record's plain error text.
fn minimal_record(execution_id: &str, error: &str, node_id: Option<String>) -> ErrorRecord {
    ErrorRecord {
        id: wf_common::generate_id(),
        execution_id: execution_id.to_string(),
        error: error.to_string(),
        error_type: None,
        timestamp: wf_common::now(),
        node_id,
        parent_error_id: None,
        error_chain: Vec::new(),
        root_cause_id: String::new(),
        caused_by: None,
        is_recoverable: false,
        recovery_action: None,
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

/// Coarse severity bucket used by the stats view.
fn severity_of(record: &ErrorRecord) -> ErrorSeverity {
    match &record.recovery_action {
        Some(RecoveryAction::Retry) | Some(RecoveryAction::Fallback) => ErrorSeverity::Warning,
        Some(RecoveryAction::ManualIntervention) | Some(RecoveryAction::Abort) => {
            ErrorSeverity::Critical
        }
        None => {
            if record.is_recoverable {
                ErrorSeverity::Warning
            } else {
                ErrorSeverity::Critical
            }
        }
    }
}

/// Normalize an error message for similarity clustering: lowercase, collapse
/// whitespace and strip common retry/attempt suffixes.
fn normalize_message(message: &str) -> String {
    let trimmed = message.trim().to_lowercase();
    let trimmed = trimmed
        .strip_suffix(")")
        .map(|s| s.rsplit_once("(").map(|(head, _)| head).unwrap_or(s))
        .unwrap_or(&trimmed);
    trimmed.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Map a workflow `ErrorRecord` onto the shared serializable view.
fn workflow_record_view(record: &ErrorRecord) -> ExecutionErrorRecord {
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

/// Heuristic recovery action for a record without an explicit one.
fn suggest_action(record: &ErrorRecord) -> String {
    if let Some(action) = &record.recovery_action {
        return action_name(action);
    }
    match record.error_type {
        Some(wf_types::ErrorType::ToolError) => {
            if record.is_recoverable {
                "retry"
            } else {
                "fallback"
            }
        }
        Some(wf_types::ErrorType::Timeout) => "retry",
        Some(wf_types::ErrorType::Validation) => {
            if record.is_recoverable {
                "skip"
            } else {
                "fallback"
            }
        }
        _ => {
            if record.is_recoverable {
                "retry"
            } else {
                "abort"
            }
        }
    }
    .to_string()
}

/// Estimated recovery likelihood in percent for an error + action pair.
fn estimate_likelihood(record: &ErrorRecord, action: &str) -> f64 {
    let mut likelihood: f64 = 50.0;
    if record.is_recoverable {
        likelihood += 30.0;
    }
    if severity_of(record) == ErrorSeverity::Warning {
        likelihood += 20.0;
    }
    match action {
        "retry" => likelihood += 15.0,
        "fallback" => likelihood += 10.0,
        "skip" => likelihood += 20.0,
        _ => {}
    }
    likelihood.clamp(0.0, 100.0)
}

/// Human-readable recovery steps for an action.
fn recovery_steps(action: &str) -> Vec<String> {
    match action {
        "retry" => vec![
            "Wait a moment".to_string(),
            "Retry the failed operation".to_string(),
            "If still failing, escalate to manual intervention".to_string(),
        ],
        "fallback" => vec![
            "Check if a fallback implementation is available".to_string(),
            "Switch to the fallback implementation".to_string(),
            "Continue execution with the fallback".to_string(),
        ],
        "skip" => vec![
            "Mark the operation as skipped".to_string(),
            "Continue with the next operation".to_string(),
        ],
        _ => vec![
            "Log detailed error information".to_string(),
            "Clean up active resources".to_string(),
            "Gracefully stop the workflow execution".to_string(),
        ],
    }
}

/// Rough estimated time to recover in milliseconds per action.
fn estimate_recovery_time(action: &str) -> Option<i64> {
    match action {
        "retry" => Some(1100),
        "fallback" => Some(600),
        "skip" => Some(200),
        _ => Some(0),
    }
}

/// Temporal pattern of error occurrence (TS `WorkflowTemporalPattern`).
fn analyze_temporal_pattern(sorted: &[ErrorRecord]) -> String {
    if sorted.len() < 3 {
        return "none".to_string();
    }
    let intervals: Vec<i64> = sorted
        .windows(2)
        .map(|pair| pair[1].timestamp - pair[0].timestamp)
        .collect();
    if intervals.len() < 2 {
        return "none".to_string();
    }
    let recent = &intervals[intervals.len().saturating_sub(3)..];
    let early = &intervals[..intervals.len().min(3)];
    let recent_avg = recent.iter().sum::<i64>() as f64 / recent.len().max(1) as f64;
    let early_avg = early.iter().sum::<i64>() as f64 / early.len().max(1) as f64;
    if recent_avg > 0.0 && early_avg > 0.0 {
        if recent_avg < early_avg * 0.7 {
            return "accelerating".to_string();
        }
        if recent_avg > early_avg * 1.3 {
            return "decelerating".to_string();
        }
    }
    "steady".to_string()
}

/// Error trend direction by comparing the first and second half (TS
/// `WorkflowErrorTrend`).
fn analyze_error_trend(sorted: &[ErrorRecord]) -> ErrorTrend {
    if sorted.len() < 2 {
        return ErrorTrend::Stable;
    }
    let mid = sorted.len() / 2;
    let first = sorted.len().saturating_sub(mid);
    let second = mid;
    let ratio = if first > 0 {
        second as f64 / first as f64
    } else {
        1.0
    };
    if ratio > 1.3 {
        ErrorTrend::Increasing
    } else if ratio < 0.7 {
        ErrorTrend::Decreasing
    } else {
        ErrorTrend::Stable
    }
}

/// Rank order of a severity label (lower = less severe).
fn severity_rank(severity: ErrorSeverity) -> u8 {
    match severity {
        ErrorSeverity::Warning => 0,
        ErrorSeverity::Critical => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_common::error_chain::ErrorRecord;
    use wf_resource::registrar::Registries;
    use wf_resource::starter::BundleRegistry;
    use wf_storage::context::StorageContext;
    use wf_types::errors::{ErrorCause, ErrorType};
    use wf_types::events::BaseEvent;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(Registries::new()),
            Arc::new(BundleRegistry::new()),
        ))
    }

    fn make_record(execution_id: &str, node_id: &str, message: &str) -> ErrorRecord {
        ErrorRecord {
            id: wf_common::generate_id(),
            execution_id: execution_id.to_string(),
            error: message.to_string(),
            error_type: Some(ErrorType::ToolError),
            timestamp: wf_common::now(),
            node_id: Some(node_id.to_string()),
            parent_error_id: None,
            error_chain: vec![],
            root_cause_id: "".into(),
            caused_by: Some(ErrorCause {
                reason: message.to_string(),
                handling_attempt: None,
            }),
            is_recoverable: true,
            recovery_action: Some(RecoveryAction::Retry),
        }
    }

    #[tokio::test]
    async fn stats_from_live_entity_error_records() {
        use wf_core::registry::MutableRegistry;
        use wf_workflow::entity::WorkflowExecutionEntity;

        let ctx = make_ctx();
        let entity = Arc::new(WorkflowExecutionEntity::new(
            wf_types::Id::from("exec-e".to_string()),
            wf_types::Id::from("wf-e".to_string()),
        ));
        entity
            .state
            .write()
            .await
            .add_error_record(make_record("exec-e", "n1", "tool boom"));
        entity
            .state
            .write()
            .await
            .add_error_record(make_record("exec-e", "n2", "llm boom"));
        ctx.workflow_executions
            .register("exec-e".to_string(), entity.clone())
            .expect("register");

        let stats = workflow_error_stats(&ctx, "exec-e").await.unwrap();
        assert_eq!(stats.total, 2);
        assert_eq!(stats.by_node.get("n1"), Some(&1));
        assert_eq!(stats.recoverable, 2);
        assert_eq!(stats.by_severity.get(&ErrorSeverity::Warning), Some(&2));

        let recommendations = recovery_recommendations(&ctx, "exec-e").await.unwrap();
        assert_eq!(recommendations.len(), 2);
        assert!(recommendations.iter().all(|r| r.recovery_action == "retry"));
    }

    #[tokio::test]
    async fn degrades_to_persisted_plain_errors() {
        let ctx = make_ctx();
        let record = wf_types::WorkflowExecution {
            id: "exec-p2".into(),
            workflow_id: "wf-p2".into(),
            workflow_version: None,
            status: ExecutionStatus::Failed,
            current_node_id: None,
            graph: None,
            variables: None,
            input: None,
            output: None,
            node_results: None,
            errors: Some(vec!["boom one".to_string(), "boom two".to_string()]),
            error: Some("fatal".to_string()),
            started_at: wf_common::now(),
            completed_at: Some(wf_common::now()),
            execution_type: None,
            fork_join_context: None,
            hierarchy: None,
        };
        ctx.storage.workflow_execution.save(&record).await.unwrap();

        let stats = workflow_error_stats(&ctx, "exec-p2").await.unwrap();
        assert_eq!(stats.total, 3);
    }

    #[tokio::test]
    async fn similar_errors_clusters_by_message() {
        let ctx = make_ctx();
        let failed = wf_types::WorkflowExecution {
            id: "exec-a".into(),
            workflow_id: "wf-a".into(),
            workflow_version: None,
            status: ExecutionStatus::Failed,
            current_node_id: None,
            graph: None,
            variables: None,
            input: None,
            output: None,
            node_results: None,
            errors: Some(vec!["tool timeout".to_string()]),
            error: None,
            started_at: wf_common::now(),
            completed_at: Some(wf_common::now()),
            execution_type: None,
            fork_join_context: None,
            hierarchy: None,
        };
        let failed2 = wf_types::WorkflowExecution {
            id: "exec-b".into(),
            workflow_id: "wf-b".into(),
            workflow_version: None,
            status: ExecutionStatus::Failed,
            current_node_id: None,
            graph: None,
            variables: None,
            input: None,
            output: None,
            node_results: None,
            errors: Some(vec!["tool timeout".to_string()]),
            error: None,
            started_at: wf_common::now(),
            completed_at: Some(wf_common::now()),
            execution_type: None,
            fork_join_context: None,
            hierarchy: None,
        };
        ctx.storage.workflow_execution.save(&failed).await.unwrap();
        ctx.storage.workflow_execution.save(&failed2).await.unwrap();

        // Live entity with the same normalized message.
        use wf_core::registry::MutableRegistry;
        use wf_workflow::entity::WorkflowExecutionEntity;
        let entity = Arc::new(WorkflowExecutionEntity::new(
            wf_types::Id::from("exec-target".to_string()),
            wf_types::Id::from("wf-t".to_string()),
        ));
        entity.state.write().await.add_error_record(make_record(
            "exec-target",
            "n1",
            "tool timeout",
        ));
        ctx.workflow_executions
            .register("exec-target".to_string(), entity.clone())
            .expect("register");

        let groups = similar_errors(&ctx, "exec-target", 10).await.unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].count, 2);
    }

    #[tokio::test]
    async fn unknown_execution_is_not_found() {
        let ctx = make_ctx();
        let err = workflow_error_stats(&ctx, "missing").await.unwrap_err();
        assert!(matches!(err, ApiError::ExecutionNotFound { .. }));
    }

    /// Build a chained pair of error records (root + dependent).
    fn chained_records(execution_id: &str, node_id: &str) -> (ErrorRecord, ErrorRecord) {
        let root = make_record(execution_id, node_id, "root failure");
        let dependent = ErrorRecord {
            id: format!("{}-dependent", root.id),
            execution_id: execution_id.to_string(),
            error: "cascading failure".to_string(),
            error_type: Some(wf_types::ErrorType::Internal),
            timestamp: wf_common::now() + 10,
            node_id: Some(node_id.to_string()),
            parent_error_id: Some(root.id.clone()),
            error_chain: vec![root.id.clone(), format!("{}-dependent", root.id)],
            root_cause_id: root.id.clone(),
            caused_by: None,
            is_recoverable: false,
            recovery_action: Some(RecoveryAction::Abort),
        };
        (root, dependent)
    }

    #[tokio::test]
    async fn error_chain_advanced_analysis_and_recovery_proposal() {
        use wf_core::registry::MutableRegistry;
        use wf_workflow::entity::WorkflowExecutionEntity;

        let ctx = make_ctx();
        let entity = Arc::new(WorkflowExecutionEntity::new(
            wf_types::Id::from("exec-chain".to_string()),
            wf_types::Id::from("wf-chain".to_string()),
        ));
        let (root, dependent) = chained_records("exec-chain", "n1");
        let dependent_id = dependent.id.clone();
        {
            let mut state = entity.state.write().await;
            state.add_error_record(root);
            state.add_error_record(dependent);
        }
        ctx.workflow_executions
            .register("exec-chain".to_string(), entity.clone())
            .expect("register");

        let chain = get_error_chain(&ctx, "exec-chain", None).await.unwrap();
        assert_eq!(chain.len(), 2);

        let from_dependent = get_error_chain(&ctx, "exec-chain", Some(&dependent_id))
            .await
            .unwrap();
        assert!(!from_dependent.is_empty());
        assert_eq!(from_dependent[0].error, "root failure");

        let advanced = get_advanced_error_analysis(&ctx, "exec-chain")
            .await
            .unwrap();
        assert_eq!(advanced.total_errors, 2);
        assert!(advanced.error_frequency.contains_key("ToolError"));
        assert_eq!(advanced.error_hotspots.len(), 1);
        assert_eq!(advanced.error_hotspots[0].node_id, "n1");
        assert_eq!(advanced.most_problematic_nodes.len(), 1);

        let proposal = get_recovery_proposal(&ctx, "exec-chain", &dependent_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(proposal.action, "abort");
        assert_eq!(proposal.affected_node.as_ref().unwrap().id, "n1");
        assert!(!proposal.steps.is_empty());
        assert!(proposal.likelihood >= 0.0);

        let stream = stream_error_chain(&ctx, "exec-chain").await.unwrap();
        let collected: Vec<_> = futures::StreamExt::collect::<Vec<_>>(stream).await;
        assert_eq!(collected.len(), 2);
        assert_eq!(collected[0].error, "root failure", "root first");
    }

    #[tokio::test]
    async fn subscribe_to_errors_forwards_bus_events() {
        let ctx = make_ctx();
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let _guard = subscribe_to_errors(&ctx, "exec-sub", move |record| {
            let _ = tx.try_send(record);
        });

        // The subscription is spawned on the current runtime; publish after
        // a short yield so the subscriber has registered.
        tokio::task::yield_now().await;
        ctx.event_bus
            .publish(BaseEvent {
                id: wf_common::generate_id(),
                r#type: EventType::Error,
                timestamp: wf_common::now(),
                workflow_id: Some("wf-sub".into()),
                execution_id: Some("exec-sub".into()),
                agent_loop_id: None,
                metadata: Some(
                    [("message".to_string(), serde_json::json!("boom"))]
                        .into_iter()
                        .collect(),
                ),
            })
            .unwrap();
        // Publish an unrelated event that must be ignored.
        ctx.event_bus
            .publish(BaseEvent {
                id: wf_common::generate_id(),
                r#type: EventType::Error,
                timestamp: wf_common::now(),
                workflow_id: None,
                execution_id: Some("other-exec".into()),
                agent_loop_id: None,
                metadata: None,
            })
            .unwrap();

        let record = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(record.execution_id, "exec-sub");
        assert_eq!(record.error, "boom");
    }
}
