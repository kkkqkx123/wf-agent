//! Core error statistics, chain and clustering queries over workflow
//! execution error records.

use std::collections::{BTreeMap, HashMap};
use std::pin::Pin;

use futures::Stream;
use wf_common::error_chain::ErrorRecord;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_types::enums::ErrorTrend;
use wf_types::ExecutionStatus;

use crate::agent::agent_error_analysis::ExecutionErrorRecord;
use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

use super::advanced::{
    analyze_error_trend, analyze_temporal_pattern, severity_of, severity_rank,
};
use super::recovery::{estimate_likelihood, estimate_recovery_time, recovery_steps, suggest_action};
use super::records::{node_name, workflow_error_records};
use super::views::{
    AdvancedWorkflowErrorAnalysis, ErrorRecommendation, ProblematicNode, RecoveryProposal,
    SimilarErrorGroup, WorkflowErrorHotspot, WorkflowErrorStats, WorkflowNodeRef,
};
use super::{MAX_RECOVERY_RECOMMENDATIONS, MAX_SIMILAR_EXECUTIONS_PER_GROUP, MAX_SIMILAR_GROUPS};
use crate::infra::util::round2;

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
    let mut out: Vec<ErrorRecommendation> = records
        .iter()
        .filter_map(|record| {
            let action = match &record.recovery_action {
                Some(action) => crate::analysis::error_common::action_name(action),
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
        .collect();
    out.truncate(MAX_RECOVERY_RECOMMENDATIONS);
    Ok(out)
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
        .map(|r| crate::analysis::error_common::normalize_message(&r.error))
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
            let normalized = crate::analysis::error_common::normalize_message(&message);
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
    let capped_limit = if limit == 0 {
        20
    } else {
        limit.min(MAX_SIMILAR_GROUPS)
    };
    groups.truncate(capped_limit);
    for group in &mut groups {
        group.executions.truncate(MAX_SIMILAR_EXECUTIONS_PER_GROUP);
        group.nodes.truncate(MAX_SIMILAR_EXECUTIONS_PER_GROUP);
    }
    Ok(groups)
}

/// Error chain of a workflow execution from the root cause up to and
/// including the given error id (or the last error when omitted).
pub async fn get_error_chain(
    ctx: &ApiContext,
    execution_id: &str,
    from_error_id: Option<&str>,
) -> ApiResult<Vec<ExecutionErrorRecord>> {
    let records = workflow_error_records(ctx, execution_id).await?;
    if records.is_empty() {
        return Ok(Vec::new());
    }
    let Some(target) = from_error_id
        .and_then(|id| records.iter().find(|r| r.id == id))
        .or_else(|| records.last())
    else {
        return Ok(Vec::new());
    };
    let chain_ids: Vec<&str> = target.error_chain.iter().map(String::as_str).collect();
    let mut chain: Vec<ExecutionErrorRecord> = records
        .iter()
        .filter(|r| chain_ids.contains(&r.id.as_str()))
        .map(crate::analysis::error_common::record_view)
        .collect();
    chain.sort_by_key(|r| r.timestamp);
    Ok(chain)
}

/// Advanced error analysis of a workflow execution: frequency by type,
/// node hotspots, temporal pattern and trend.
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
            truncated: false,
        });
    }

    let mut error_frequency: BTreeMap<String, u64> = BTreeMap::new();
    let mut node_problems: BTreeMap<
        String,
        (u64, Vec<String>, Vec<String>, wf_types::enums::ErrorSeverity),
    > = BTreeMap::new();
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

    let truncated = hotspots.len() > 10;
    Ok(AdvancedWorkflowErrorAnalysis {
        execution_id: execution_id.to_string(),
        total_errors: records.len() as u32,
        error_frequency,
        error_hotspots: hotspots.into_iter().take(10).collect(),
        temporal_pattern,
        most_problematic_nodes,
        error_trend,
        truncated,
    })
}

/// Recovery proposal for a specific error of a workflow execution.
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
        Some(action) => crate::analysis::error_common::action_name(action),
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
/// starting from the root cause.
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
        root_first.push(crate::analysis::error_common::record_view(root));
        for record in &sorted {
            if record.id != root.id && record.error_chain.contains(&root.id) {
                root_first.push(crate::analysis::error_common::record_view(record));
            }
        }
    }
    for record in &sorted {
        if !root_first.iter().any(|r| r.id == record.id) {
            root_first.push(crate::analysis::error_common::record_view(record));
        }
    }
    Ok(Box::pin(futures::stream::iter(root_first)))
}
