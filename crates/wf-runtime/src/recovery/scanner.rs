use wf_storage::adapter::concrete::MemoryWorkflowExecutionStorage;
use wf_storage::domain::store::QueryFilter;
use wf_types::WorkflowExecution;

use crate::error::{RuntimeError, RuntimeResult};

pub struct RecoveryScanner {
    execution_storage: MemoryWorkflowExecutionStorage,
}

impl RecoveryScanner {
    pub fn new(execution_storage: MemoryWorkflowExecutionStorage) -> Self {
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
