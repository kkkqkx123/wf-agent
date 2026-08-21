use std::sync::Arc;

use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::adapter::execution::WorkflowExecutionStorageAdapter;
use wf_storage::adapter::AgentExecutionStorageAdapter;
use wf_storage::adapter::{AgentExecutionStorage, WorkflowExecutionStorage};
use wf_storage::backend::StorageBackend;
use wf_types::{AgentExecution, ExecutionStatus, WorkflowExecution};

/// Unified write point for execution records.
///
/// The execution engines (`wf-workflow` / `wf-agent`) persist their
/// `WorkflowExecution` / `AgentExecution` records through this manager, so the
/// storage adapters remain the single source of truth and the API layer stays
/// read-only with respect to the execution stores.
///
/// Both adapter slots are optional: an unwired slot turns persistence for that
/// side into a no-op (the execution is still driven fully in memory, matching
/// the pre-persistence behavior). Persistence failures never fail the execution;
/// they degrade to a warning log.
#[derive(Clone)]
pub struct ExecutionStateManager {
    workflow_store: Option<Arc<WorkflowExecutionStorage<StorageBackend>>>,
    agent_store: Option<Arc<AgentExecutionStorage<StorageBackend>>>,
}

impl Default for ExecutionStateManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutionStateManager {
    pub fn new() -> Self {
        Self {
            workflow_store: None,
            agent_store: None,
        }
    }

    /// Wire the workflow execution store; without it workflow executions are
    /// not persisted.
    pub fn with_workflow_store(
        mut self,
        store: Arc<WorkflowExecutionStorage<StorageBackend>>,
    ) -> Self {
        self.workflow_store = Some(store);
        self
    }

    /// Wire the agent execution store; without it agent executions are not
    /// persisted.
    pub fn with_agent_store(mut self, store: Arc<AgentExecutionStorage<StorageBackend>>) -> Self {
        self.agent_store = Some(store);
        self
    }

    /// Persist (upsert) a workflow execution record.
    pub async fn persist_workflow(&self, record: &WorkflowExecution) {
        let Some(store) = &self.workflow_store else {
            return;
        };
        if let Err(err) = store.save(record).await {
            tracing::warn!(
                target: "wf_execution_shared",
                execution_id = %record.id,
                error = %err,
                "failed to persist workflow execution record"
            );
        }
    }

    /// Update only the status of a persisted workflow execution. No-op when no
    /// record exists yet (e.g. the execution was aborted before its start
    /// record was written).
    pub async fn update_workflow_status(&self, execution_id: &str, status: &ExecutionStatus) {
        let Some(store) = &self.workflow_store else {
            return;
        };
        if let Err(err) = store.update_status(execution_id, status).await {
            tracing::warn!(
                target: "wf_execution_shared",
                execution_id,
                error = %err,
                "failed to update workflow execution status"
            );
        }
    }

    /// Persist (upsert) an agent execution record.
    pub async fn persist_agent(&self, record: &AgentExecution) {
        let Some(store) = &self.agent_store else {
            return;
        };
        if let Err(err) = store.save(record).await {
            tracing::warn!(
                target: "wf_execution_shared",
                agent_loop_id = %record.id,
                error = %err,
                "failed to persist agent execution record"
            );
        }
    }

    /// Update only the status of a persisted agent execution. No-op when no
    /// record exists yet.
    pub async fn update_agent_status(&self, agent_loop_id: &str, status: &ExecutionStatus) {
        let Some(store) = &self.agent_store else {
            return;
        };
        if let Err(err) = store.update_status(agent_loop_id, status).await {
            tracing::warn!(
                target: "wf_execution_shared",
                agent_loop_id,
                error = %err,
                "failed to update agent execution status"
            );
        }
    }
}
