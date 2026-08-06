use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use serde::Serialize;

use wf_common::error_chain::ErrorRecord;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_types::errors::RecoveryAction;
use wf_types::ExecutionStatus;

use crate::context::ApiContext;
use crate::error::{ApiError, ApiResult};

/// Aggregate error statistics of one workflow execution (TS
/// `WorkflowErrorAnalysisAPI` counterpart).
#[derive(Debug, Clone, Default, Serialize)]
pub struct WorkflowErrorStats {
    pub execution_id: String,
    pub total: u32,
    pub by_type: BTreeMap<String, u64>,
    pub by_node: BTreeMap<String, u64>,
    pub by_severity: BTreeMap<String, u64>,
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

/// Error analysis over live entity error chains and persisted execution
/// records. Lives on top of `wf-common::error_chain::ErrorRecord` (the shape
/// the workflow/agent engines record on failure) and the `FailurePolicyManager`
/// recovery semantics exposed through `RecoveryAction`.
pub struct ErrorAnalysisApi {
    ctx: Arc<ApiContext>,
}

impl ErrorAnalysisApi {
    pub fn new(ctx: Arc<ApiContext>) -> Self {
        Self { ctx }
    }

    /// Error statistics of a workflow execution.
    pub async fn workflow_error_stats(&self, execution_id: &str) -> ApiResult<WorkflowErrorStats> {
        let records = self.workflow_error_records(execution_id).await?;
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
        &self,
        execution_id: &str,
    ) -> ApiResult<Vec<ErrorRecommendation>> {
        let records = self.workflow_error_records(execution_id).await?;
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
        &self,
        execution_id: &str,
        limit: usize,
    ) -> ApiResult<Vec<SimilarErrorGroup>> {
        let records = self.workflow_error_records(execution_id).await?;
        if records.is_empty() {
            return Ok(Vec::new());
        }
        let query_messages: Vec<String> = records
            .iter()
            .map(|r| normalize_message(&r.error))
            .collect();

        let mut clusters: HashMap<String, SimilarErrorGroup> = HashMap::new();
        let mut others = self.ctx.storage.workflow_execution.list(None).await?;
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

    async fn workflow_error_records(&self, execution_id: &str) -> ApiResult<Vec<ErrorRecord>> {
        if let Some(entity) = self.ctx.workflow_execution(execution_id) {
            return Ok(entity.state.read().await.error_records().to_vec());
        }
        let record = self
            .ctx
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
fn severity_of(record: &ErrorRecord) -> String {
    match &record.recovery_action {
        Some(RecoveryAction::Retry) | Some(RecoveryAction::Fallback) => "warning".to_string(),
        Some(RecoveryAction::ManualIntervention) | Some(RecoveryAction::Abort) => {
            "critical".to_string()
        }
        None => {
            if record.is_recoverable {
                "warning".to_string()
            } else {
                "critical".to_string()
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

#[cfg(test)]
mod tests {
    use super::*;
    use wf_common::error_chain::ErrorRecord;
    use wf_resource::registrar::Registries;
    use wf_resource::starter::BundleRegistry;
    use wf_storage::context::StorageContext;
    use wf_types::errors::{ErrorCause, ErrorType};

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

        let api = ErrorAnalysisApi::new(ctx);
        let stats = api.workflow_error_stats("exec-e").await.unwrap();
        assert_eq!(stats.total, 2);
        assert_eq!(stats.by_node.get("n1"), Some(&1));
        assert_eq!(stats.recoverable, 2);
        assert_eq!(stats.by_severity.get("warning"), Some(&2));

        let recommendations = api.recovery_recommendations("exec-e").await.unwrap();
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

        let api = ErrorAnalysisApi::new(ctx);
        let stats = api.workflow_error_stats("exec-p2").await.unwrap();
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

        let api = ErrorAnalysisApi::new(ctx);
        let groups = api.similar_errors("exec-target", 10).await.unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].count, 2);
    }

    #[tokio::test]
    async fn unknown_execution_is_not_found() {
        let ctx = make_ctx();
        let api = ErrorAnalysisApi::new(ctx);
        let err = api.workflow_error_stats("missing").await.unwrap_err();
        assert!(matches!(err, ApiError::ExecutionNotFound { .. }));
    }
}
