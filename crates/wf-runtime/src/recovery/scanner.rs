use wf_storage::adapter::adapter_impls::WorkflowExecutionStorage;
use wf_storage::backend::StorageBackend;
use wf_storage::domain::store::QueryFilter;
use wf_types::WorkflowExecution;

use crate::error::{RuntimeError, RuntimeResult};

pub struct RecoveryScanner {
    execution_storage: WorkflowExecutionStorage<StorageBackend>,
}

impl RecoveryScanner {
    pub fn new(execution_storage: WorkflowExecutionStorage<StorageBackend>) -> Self {
        Self { execution_storage }
    }

    pub async fn scan_incomplete(&self) -> RuntimeResult<Vec<WorkflowExecution>> {
        let mut results = Vec::new();

        for status in &["running", "paused", "created"] {
            let filter = QueryFilter::new().with_field("status", status);
            let mut executions = self
                .execution_storage
                .entity_store()
                .list(Some(&filter))
                .await
                .map_err(RuntimeError::Storage)?;
            results.append(&mut executions);
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_storage::adapter::base::BaseStorageAdapter;
    use wf_storage::context::StorageContext;
    use wf_types::ExecutionStatus;

    fn make_execution(id: &str, status: ExecutionStatus) -> WorkflowExecution {
        WorkflowExecution {
            id: id.into(),
            workflow_id: "wf-1".into(),
            workflow_version: None,
            status,
            current_node_id: None,
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
    async fn scan_incomplete_returns_running_paused_created_only() {
        let ctx = StorageContext::new_memory();
        let storage = ctx.workflow_execution.clone();
        storage
            .save(&make_execution("run-1", ExecutionStatus::Running))
            .await
            .unwrap();
        storage
            .save(&make_execution("paused-1", ExecutionStatus::Paused))
            .await
            .unwrap();
        storage
            .save(&make_execution("created-1", ExecutionStatus::Created))
            .await
            .unwrap();
        storage
            .save(&make_execution("done-1", ExecutionStatus::Completed))
            .await
            .unwrap();
        storage
            .save(&make_execution("failed-1", ExecutionStatus::Failed))
            .await
            .unwrap();
        storage
            .save(&make_execution("cancelled-1", ExecutionStatus::Cancelled))
            .await
            .unwrap();

        let scanner = RecoveryScanner::new(storage);
        let found = scanner.scan_incomplete().await.unwrap();
        let mut ids: Vec<&str> = found.iter().map(|e| e.id.as_str()).collect();
        ids.sort_unstable();
        assert_eq!(ids, vec!["created-1", "paused-1", "run-1"]);
    }

    #[tokio::test]
    async fn scan_incomplete_empty_when_nothing_incomplete() {
        let ctx = StorageContext::new_memory();
        let storage = ctx.workflow_execution.clone();
        storage
            .save(&make_execution("done-1", ExecutionStatus::Completed))
            .await
            .unwrap();

        let scanner = RecoveryScanner::new(storage);
        let found = scanner.scan_incomplete().await.unwrap();
        assert!(found.is_empty());
    }
}
