use std::sync::Arc;

use tracing::{info, warn};

use crate::error::RuntimeResult;
use crate::recovery::scanner::RecoveryScanner;
use crate::recovery::{RecoveryExecutor, RecoveryItem, RecoveryResult};

/// Orchestrates startup recovery: scans incomplete executions and drives each
/// one through the injected [`RecoveryExecutor`].
///
/// Without an executor the orchestrator reports executions as *skipped*
/// (warn + `recovered: false`) instead of pretending they were recovered —
/// the pre-wiring behavior logged `info!("Recovered execution")` for rows it
/// never touched.
pub struct RecoveryOrchestrator {
    scanner: RecoveryScanner,
    executor: Option<Arc<dyn RecoveryExecutor>>,
}

impl RecoveryOrchestrator {
    pub fn new(scanner: RecoveryScanner) -> Self {
        Self {
            scanner,
            executor: None,
        }
    }

    pub fn with_recovery_executor(mut self, executor: Arc<dyn RecoveryExecutor>) -> Self {
        self.executor = Some(executor);
        self
    }

    pub async fn recover_all(&self, ctx: &wf_api::ApiContext) -> RuntimeResult<RecoveryResult> {
        let incomplete = self.scanner.scan_incomplete().await?;
        let mut result = RecoveryResult::default();

        if incomplete.is_empty() {
            info!("No incomplete executions found; nothing to recover");
            return Ok(result);
        }

        let Some(executor) = &self.executor else {
            warn!(
                count = incomplete.len(),
                "Recovery executor not wired; executions are reported, not recovered"
            );
            for execution in &incomplete {
                result.skipped.push(skip_item(execution));
            }
            return Ok(result);
        };

        for execution in &incomplete {
            match executor.recover_execution(ctx, execution).await {
                Ok(item) if item.recovered => {
                    info!(
                        execution_id = %item.execution_id,
                        status = %item.status,
                        "Recovered execution"
                    );
                    result.recovered.push(item);
                }
                Ok(item) => {
                    warn!(
                        execution_id = %item.execution_id,
                        note = %item.note.as_deref().unwrap_or("unknown"),
                        "Execution left un-recovered"
                    );
                    result.skipped.push(item);
                }
                Err(e) => {
                    warn!("Failed to recover execution {}: {}", execution.id, e);
                    result
                        .failed
                        .push((execution.id.to_string(), e.to_string()));
                }
            }
        }

        Ok(result)
    }
}

fn skip_item(execution: &wf_types::WorkflowExecution) -> RecoveryItem {
    RecoveryItem {
        execution_id: execution.id.clone(),
        status: format!("{:?}", execution.status),
        current_node_id: execution.current_node_id.clone(),
        recovered: false,
        note: Some("recovery executor not wired".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_storage::adapter::base::BaseStorageAdapter;
    use wf_storage::context::StorageContext;
    use wf_types::ExecutionStatus;

    fn make_execution(id: &str, status: ExecutionStatus) -> wf_types::WorkflowExecution {
        wf_types::WorkflowExecution {
            id: id.into(),
            workflow_id: "wf-1".into(),
            workflow_version: None,
            status,
            current_node_id: Some("v1".into()),
            graph: None,
            variables: None,
            input: None,
            output: None,
            node_results: None,
            errors: None,
            started_at: 0,
            completed_at: None,
            error: None,
            execution_type: None,
            fork_join_context: None,
            hierarchy: None,
        }
    }

    #[tokio::test]
    async fn recover_all_without_executor_marks_everything_skipped() {
        let ctx = StorageContext::new_memory();
        ctx.workflow_execution
            .save(&make_execution("run-1", ExecutionStatus::Running))
            .await
            .unwrap();
        ctx.workflow_execution
            .save(&make_execution("paused-1", ExecutionStatus::Paused))
            .await
            .unwrap();

        let orchestrator = RecoveryOrchestrator::new(RecoveryScanner::new(ctx.workflow_execution));
        let api_ctx = wf_api::ApiContext::new(
            wf_storage::context::StorageContext::new_memory(),
            std::sync::Arc::new(wf_resource::registry::ResourceRegistries::new()),
            std::sync::Arc::new(wf_resource::resource_plugin::ResourcePluginRegistry::new()),
        );
        let result = orchestrator.recover_all(&api_ctx).await.unwrap();

        assert!(result.recovered.is_empty());
        assert!(result.failed.is_empty());
        assert_eq!(result.skipped.len(), 2);
        assert!(
            result.skipped.iter().all(|i| !i.recovered),
            "every skipped item must be reported as not recovered"
        );
        assert!(result
            .skipped
            .iter()
            .all(|i| i.note.as_deref() == Some("recovery executor not wired")));
    }

    #[tokio::test]
    async fn recover_all_with_executor_reports_recovered_and_failed() {
        let ctx = StorageContext::new_memory();
        ctx.workflow_execution
            .save(&make_execution("good-1", ExecutionStatus::Running))
            .await
            .unwrap();
        ctx.workflow_execution
            .save(&make_execution("bad-1", ExecutionStatus::Running))
            .await
            .unwrap();

        struct MockExecutor;
        #[async_trait::async_trait]
        impl RecoveryExecutor for MockExecutor {
            async fn recover_execution(
                &self,
                _ctx: &wf_api::ApiContext,
                execution: &wf_types::WorkflowExecution,
            ) -> RuntimeResult<RecoveryItem> {
                if execution.id.as_str() == "bad-1" {
                    return Err(crate::error::RuntimeError::Config("boom".to_string()));
                }
                Ok(RecoveryItem {
                    execution_id: execution.id.clone(),
                    status: "Completed".into(),
                    current_node_id: None,
                    recovered: true,
                    note: None,
                })
            }
        }

        let orchestrator = RecoveryOrchestrator::new(RecoveryScanner::new(ctx.workflow_execution))
            .with_recovery_executor(std::sync::Arc::new(MockExecutor));
        let api_ctx = wf_api::ApiContext::new(
            wf_storage::context::StorageContext::new_memory(),
            std::sync::Arc::new(wf_resource::registry::ResourceRegistries::new()),
            std::sync::Arc::new(wf_resource::resource_plugin::ResourcePluginRegistry::new()),
        );
        let result = orchestrator.recover_all(&api_ctx).await.unwrap();

        assert_eq!(result.recovered.len(), 1);
        assert_eq!(result.recovered[0].execution_id, "good-1");
        assert!(result.recovered[0].recovered);
        assert_eq!(result.failed.len(), 1);
        assert_eq!(result.failed[0].0, "bad-1");
        assert!(result.skipped.is_empty());
    }

    #[tokio::test]
    async fn recover_all_with_executor_marks_unrecoverable_as_skipped() {
        let ctx = StorageContext::new_memory();
        ctx.workflow_execution
            .save(&make_execution("skip-1", ExecutionStatus::Running))
            .await
            .unwrap();

        struct SkipExecutor;
        #[async_trait::async_trait]
        impl RecoveryExecutor for SkipExecutor {
            async fn recover_execution(
                &self,
                _ctx: &wf_api::ApiContext,
                execution: &wf_types::WorkflowExecution,
            ) -> RuntimeResult<RecoveryItem> {
                Ok(RecoveryItem {
                    execution_id: execution.id.clone(),
                    status: format!("{:?}", execution.status),
                    current_node_id: execution.current_node_id.clone(),
                    recovered: false,
                    note: Some("no checkpoint available".to_string()),
                })
            }
        }

        let orchestrator = RecoveryOrchestrator::new(RecoveryScanner::new(ctx.workflow_execution))
            .with_recovery_executor(std::sync::Arc::new(SkipExecutor));
        let api_ctx = wf_api::ApiContext::new(
            wf_storage::context::StorageContext::new_memory(),
            std::sync::Arc::new(wf_resource::registry::ResourceRegistries::new()),
            std::sync::Arc::new(wf_resource::resource_plugin::ResourcePluginRegistry::new()),
        );
        let result = orchestrator.recover_all(&api_ctx).await.unwrap();

        assert!(result.recovered.is_empty());
        assert!(result.failed.is_empty());
        assert_eq!(result.skipped.len(), 1);
        assert_eq!(result.skipped[0].execution_id, "skip-1");
        assert_eq!(
            result.skipped[0].note.as_deref(),
            Some("no checkpoint available")
        );
    }

    #[tokio::test]
    async fn recover_all_with_nothing_incomplete_is_empty() {
        let ctx = StorageContext::new_memory();
        ctx.workflow_execution
            .save(&make_execution("done-1", ExecutionStatus::Completed))
            .await
            .unwrap();

        let orchestrator = RecoveryOrchestrator::new(RecoveryScanner::new(ctx.workflow_execution));
        let api_ctx = wf_api::ApiContext::new(
            wf_storage::context::StorageContext::new_memory(),
            std::sync::Arc::new(wf_resource::registry::ResourceRegistries::new()),
            std::sync::Arc::new(wf_resource::resource_plugin::ResourcePluginRegistry::new()),
        );
        let result = orchestrator.recover_all(&api_ctx).await.unwrap();
        assert!(result.is_empty());
    }
}
