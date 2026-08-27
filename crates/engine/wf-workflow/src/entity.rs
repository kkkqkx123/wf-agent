use std::sync::Arc;

use dashmap::DashMap;
use serde_json::Value;
use wf_common::retry::RetryBudget;
use wf_core::interruption::{InterruptionSignal, InterruptionState};
use wf_execution_shared::types::execution_entity::{ExecutionEntity, ExecutionStatus};
use wf_types::Id;

use crate::state::WorkflowExecutionState;

pub struct WorkflowExecutionEntity {
    id: Id,
    workflow_id: Id,
    pub state: Arc<tokio::sync::RwLock<WorkflowExecutionState>>,
    interruption: InterruptionState,
    cancellation: tokio_util::sync::CancellationToken,
    variables: Arc<DashMap<String, Value>>,
    node_results: Arc<DashMap<String, Value>>,
    pub current_node_id: Arc<tokio::sync::RwLock<Option<String>>>,
    parent_execution_id: Option<Id>,
    child_execution_ids: Arc<tokio::sync::RwLock<Vec<Id>>>,
    /// Root-to-parent execution id chain (oldest first, excluding self).
    /// Resolved from the parent entity when the run is linked, so deep
    /// hierarchies keep full ancestry across checkpoint restore.
    ancestors: Vec<Id>,
    /// Nesting depth in the execution hierarchy (0 = root).
    hierarchy_depth: u32,
    /// Final result of the execution, written on completion (both sync and
    /// spawned paths). `None` until the execution settles.
    output: Arc<tokio::sync::RwLock<Option<Value>>>,
    /// Global retry budget shared across the execution (fork branches,
    /// node retries). `None` = no budget constraint.
    retry_budget: Option<Arc<RetryBudget>>,
}

impl WorkflowExecutionEntity {
    pub fn new(id: Id, workflow_id: Id) -> Self {
        Self {
            id,
            workflow_id,
            state: Arc::new(tokio::sync::RwLock::new(WorkflowExecutionState::new())),
            interruption: InterruptionState::new(),
            cancellation: tokio_util::sync::CancellationToken::new(),
            variables: Arc::new(DashMap::new()),
            node_results: Arc::new(DashMap::new()),
            current_node_id: Arc::new(tokio::sync::RwLock::new(None)),
            parent_execution_id: None,
            child_execution_ids: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            ancestors: Vec::new(),
            hierarchy_depth: 0,
            output: Arc::new(tokio::sync::RwLock::new(None)),
            retry_budget: None,
        }
    }

    pub fn with_parent_execution_id(mut self, parent_id: Id) -> Self {
        self.parent_execution_id = Some(parent_id);
        self
    }

    /// Record the full ancestor chain (oldest first, excluding self),
    /// resolved from the parent execution at build time.
    pub fn with_ancestors(mut self, ancestors: Vec<Id>) -> Self {
        self.ancestors = ancestors;
        self
    }

    /// Set the nesting depth in the execution hierarchy (0 = root).
    pub fn with_hierarchy_depth(mut self, depth: u32) -> Self {
        self.hierarchy_depth = depth;
        self
    }

    /// Set the global retry budget for this execution.
    pub fn with_retry_budget(mut self, budget: Arc<RetryBudget>) -> Self {
        self.retry_budget = Some(budget);
        self
    }

    /// Get the retry budget if configured.
    pub fn retry_budget(&self) -> Option<&Arc<RetryBudget>> {
        self.retry_budget.as_ref()
    }

    pub fn id(&self) -> &Id {
        &self.id
    }

    pub fn workflow_id(&self) -> &Id {
        &self.workflow_id
    }

    pub fn variables(&self) -> &Arc<DashMap<String, Value>> {
        &self.variables
    }

    pub fn interruption(&self) -> &InterruptionState {
        &self.interruption
    }

    pub fn node_results(&self) -> &Arc<DashMap<String, Value>> {
        &self.node_results
    }

    pub fn child_execution_ids(&self) -> &Arc<tokio::sync::RwLock<Vec<Id>>> {
        &self.child_execution_ids
    }

    pub fn parent_execution_id(&self) -> Option<&Id> {
        self.parent_execution_id.as_ref()
    }

    pub fn ancestors(&self) -> &[Id] {
        &self.ancestors
    }

    /// The final execution output; `None` until the execution settles.
    pub async fn output(&self) -> Option<Value> {
        self.output.read().await.clone()
    }

    /// Record the final execution output (completion path).
    pub async fn set_output(&self, output: Value) {
        *self.output.write().await = Some(output);
    }

    pub fn get_variable(&self, name: &str) -> Option<Value> {
        self.variables.get(name).map(|v| v.clone())
    }

    pub fn set_variable(&self, name: impl Into<String>, value: Value) {
        self.variables.insert(name.into(), value);
    }

    pub fn get_node_result(&self, node_id: &str) -> Option<Value> {
        self.node_results.get(node_id).map(|v| v.clone())
    }

    pub fn set_node_result(&self, node_id: impl Into<String>, value: Value) {
        self.node_results.insert(node_id.into(), value);
    }

    pub async fn register_child(&self, child_id: Id) {
        self.child_execution_ids.write().await.push(child_id);
    }

    pub async fn unregister_child(&self, child_id: &Id) {
        self.child_execution_ids
            .write()
            .await
            .retain(|id| id != child_id);
    }

    /// Read the shared status from a synchronous context. Tries a non-blocking
    /// `try_read` first (works on any runtime); when the lock is contended it
    /// blocks on the tokio runtime (multi-thread only, where `block_in_place`
    /// is safe). When no suitable runtime context exists — `block_in_place`
    /// would panic on a current-thread runtime and blocking outside tokio
    /// would deadlock — it infers a coherent status from the sync-visible
    /// signals (cancellation / interruption) instead of fabricating a fresh
    /// state.
    fn sync_status(&self) -> ExecutionStatus {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            if handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread {
                return tokio::task::block_in_place(|| {
                    handle.block_on(async { self.state.read().await.status() })
                });
            }
        }
        if self.cancellation.is_cancelled() {
            return ExecutionStatus::Cancelled;
        }
        match self.interruption.check() {
            Some(InterruptionSignal::Stop) => return ExecutionStatus::Cancelled,
            Some(InterruptionSignal::Pause) => return ExecutionStatus::Paused,
            _ => {}
        }
        ExecutionStatus::Running
    }
}

#[async_trait::async_trait]
impl ExecutionEntity for WorkflowExecutionEntity {
    fn id(&self) -> &Id {
        &self.id
    }

    fn status(&self) -> ExecutionStatus {
        if let Ok(state) = self.state.try_read() {
            return state.status();
        }
        self.sync_status()
    }

    fn is_running(&self) -> bool {
        matches!(self.status(), ExecutionStatus::Running)
    }

    fn is_paused(&self) -> bool {
        matches!(self.status(), ExecutionStatus::Paused)
    }

    fn is_completed(&self) -> bool {
        matches!(self.status(), ExecutionStatus::Completed)
    }

    fn is_failed(&self) -> bool {
        matches!(self.status(), ExecutionStatus::Failed)
    }

    fn is_cancelled(&self) -> bool {
        matches!(
            self.status(),
            ExecutionStatus::Cancelled | ExecutionStatus::Stopped
        )
    }

    async fn pause(&self) -> Result<(), wf_execution_shared::error::ExecutionSharedError> {
        self.interruption.pause()?;
        self.state.write().await.pause()?;
        Ok(())
    }

    async fn resume(&self) -> Result<(), wf_execution_shared::error::ExecutionSharedError> {
        self.interruption.resume()?;
        self.state.write().await.resume()?;
        Ok(())
    }

    async fn stop(&self) -> Result<(), wf_execution_shared::error::ExecutionSharedError> {
        self.interruption.stop()?;
        self.cancellation.cancel();
        if self.state.read().await.status().is_terminal() {
            return Ok(());
        }
        self.state.write().await.cancel()?;
        Ok(())
    }

    async fn abort(&self) {
        self.cancellation.cancel();
    }

    fn get_abort_signal(&self) -> tokio_util::sync::CancellationToken {
        self.cancellation.clone()
    }

    fn get_hierarchy_depth(&self) -> u32 {
        self.hierarchy_depth
    }

    fn get_root_execution_id(&self) -> Option<Id> {
        Some(self.id.clone())
    }

    fn get_ancestors(&self) -> Vec<Id> {
        self.ancestors.clone()
    }
}

impl wf_core::execution_loop::HasInterruption for WorkflowExecutionEntity {
    fn interruption(&self) -> &InterruptionState {
        &self.interruption
    }
}
