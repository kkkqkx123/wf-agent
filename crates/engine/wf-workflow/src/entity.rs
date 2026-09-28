use std::sync::Arc;

use dashmap::DashMap;
use serde_json::Value;
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
        self.state.write().await.pause()?;
        self.interruption.pause()?;
        Ok(())
    }

    async fn resume(&self) -> Result<(), wf_execution_shared::error::ExecutionSharedError> {
        self.state.write().await.resume()?;
        self.interruption.resume()?;
        Ok(())
    }

    async fn stop(&self) -> Result<(), wf_execution_shared::error::ExecutionSharedError> {
        if self.state.read().await.status().is_terminal() {
            return Ok(());
        }
        self.state.write().await.cancel()?;
        self.interruption.stop()?;
        self.cancellation.cancel();
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
        if let Some(root) = self.ancestors.first() {
            return Some(root.clone());
        }
        if let Some(parent) = &self.parent_execution_id {
            return Some(parent.clone());
        }
        Some(self.id.clone())
    }

    fn get_ancestors(&self) -> Vec<Id> {
        self.ancestors.clone()
    }
}

impl wf_execution_shared::execution_loop::HasInterruption for WorkflowExecutionEntity {
    fn interruption(&self) -> &InterruptionState {
        &self.interruption
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn new_entity_starts_in_created_state() {
        let entity = WorkflowExecutionEntity::new("exec-1".to_string(), "wf-1".to_string());
        assert_eq!(entity.id(), "exec-1");
        assert_eq!(entity.workflow_id(), "wf-1");
        assert_eq!(entity.status(), ExecutionStatus::Created);
        assert!(!entity.is_running());
        assert!(entity.output().await.is_none());
    }

    #[tokio::test]
    async fn variables_and_node_results_round_trip() {
        let entity = WorkflowExecutionEntity::new("exec-1".to_string(), "wf-1".to_string());
        assert!(entity.get_variable("missing").is_none());
        entity.set_variable("count", serde_json::json!(3));
        assert_eq!(entity.get_variable("count"), Some(serde_json::json!(3)));

        assert!(entity.get_node_result("n1").is_none());
        entity.set_node_result("n1", serde_json::json!({"ok": true}));
        assert_eq!(
            entity.get_node_result("n1"),
            Some(serde_json::json!({"ok": true}))
        );
    }

    #[tokio::test]
    async fn output_can_be_set_once_and_read_back() {
        let entity = WorkflowExecutionEntity::new("exec-1".to_string(), "wf-1".to_string());
        entity.set_output(serde_json::json!({"final": 42})).await;
        assert_eq!(
            entity.output().await,
            Some(serde_json::json!({"final": 42}))
        );
    }

    #[tokio::test]
    async fn pause_resume_flow_through_state_and_interruption() {
        let entity = WorkflowExecutionEntity::new("exec-1".to_string(), "wf-1".to_string());
        entity.state.write().await.start().expect("start");

        entity.pause().await.expect("pause");
        assert_eq!(entity.status(), ExecutionStatus::Paused);
        assert!(entity.is_paused());
        assert!(!entity.is_running());

        entity.resume().await.expect("resume");
        assert_eq!(entity.status(), ExecutionStatus::Running);
        assert!(entity.is_running());
    }

    #[tokio::test]
    async fn stop_is_idempotent_and_cancels_the_token() {
        let entity = WorkflowExecutionEntity::new("exec-1".to_string(), "wf-1".to_string());
        entity.state.write().await.start().expect("start");

        let signal = entity.get_abort_signal();
        assert!(!signal.is_cancelled());
        entity.stop().await.expect("stop");
        assert!(signal.is_cancelled());
        assert!(entity.is_cancelled());

        // A second stop on the already-terminal state must be a no-op Ok.
        entity.stop().await.expect("second stop");
    }

    #[tokio::test]
    async fn abort_cancels_without_touching_the_state() {
        let entity = WorkflowExecutionEntity::new("exec-1".to_string(), "wf-1".to_string());
        entity.state.write().await.start().expect("start");
        entity.abort().await;
        assert!(entity.get_abort_signal().is_cancelled());
        // abort() only cancels the token; the state machine stays Running.
        assert!(entity.is_running());
    }

    #[test]
    fn root_execution_id_falls_back_through_the_hierarchy() {
        // Root: ancestors empty, no parent -> self.
        let root = WorkflowExecutionEntity::new("root".to_string(), "wf-1".to_string());
        assert_eq!(root.get_root_execution_id(), Some("root".to_string()));
        assert_eq!(root.get_ancestors(), Vec::<Id>::new());
        assert_eq!(root.get_hierarchy_depth(), 0);

        // Child with only a parent -> parent is the root.
        let child = WorkflowExecutionEntity::new("child".to_string(), "wf-1".to_string())
            .with_parent_execution_id("root".to_string());
        assert_eq!(child.get_root_execution_id(), Some("root".to_string()));

        // Grandchild with a full ancestor chain -> oldest ancestor is the root.
        let grandchild = WorkflowExecutionEntity::new("gc".to_string(), "wf-1".to_string())
            .with_parent_execution_id("child".to_string())
            .with_ancestors(vec!["root".to_string(), "child".to_string()])
            .with_hierarchy_depth(2);
        assert_eq!(grandchild.get_root_execution_id(), Some("root".to_string()));
        assert_eq!(
            grandchild.get_ancestors(),
            vec!["root".to_string(), "child".to_string()]
        );
        assert_eq!(grandchild.get_hierarchy_depth(), 2);
    }

    #[tokio::test]
    async fn child_registration_can_be_reversed() {
        let entity = WorkflowExecutionEntity::new("exec-1".to_string(), "wf-1".to_string());
        entity.register_child("child-1".to_string()).await;
        entity.register_child("child-2".to_string()).await;
        {
            let ids = entity.child_execution_ids.read().await;
            assert_eq!(
                ids.as_slice(),
                &["child-1".to_string(), "child-2".to_string()]
            );
        }
        entity.unregister_child(&"child-1".to_string()).await;
        let ids = entity.child_execution_ids.read().await;
        assert_eq!(ids.as_slice(), &["child-2".to_string()]);
    }

    #[tokio::test]
    async fn failed_status_reports_through_the_entity_trait() {
        let entity = WorkflowExecutionEntity::new("exec-1".to_string(), "wf-1".to_string());
        entity.state.write().await.start().expect("start");
        entity
            .state
            .write()
            .await
            .fail("boom".to_string())
            .expect("fail");
        assert!(entity.is_failed());
        assert!(!entity.is_completed());
        assert!(!entity.is_cancelled());
    }
}
