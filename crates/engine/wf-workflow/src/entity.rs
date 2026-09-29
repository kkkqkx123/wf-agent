use std::sync::Arc;

use dashmap::DashMap;
use serde_json::Value;
use wf_core::hierarchy::manager::ExecutionHierarchyManager;
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
    hierarchy: Arc<ExecutionHierarchyManager>,
    execution_type: Option<wf_types::workflow_execution::WorkflowExecutionType>,
    /// Final result of the execution, written on completion (both sync and
    /// spawned paths). `None` until the execution settles.
    output: Arc<tokio::sync::RwLock<Option<Value>>>,
}

impl WorkflowExecutionEntity {
    pub fn new(id: Id, workflow_id: Id) -> Self {
        let hierarchy = Arc::new(ExecutionHierarchyManager::new(
            id.clone(),
            wf_types::execution::ExecutionType::Workflow,
        ));
        Self {
            id,
            workflow_id,
            state: Arc::new(tokio::sync::RwLock::new(WorkflowExecutionState::new())),
            interruption: InterruptionState::new(),
            cancellation: tokio_util::sync::CancellationToken::new(),
            variables: Arc::new(DashMap::new()),
            node_results: Arc::new(DashMap::new()),
            current_node_id: Arc::new(tokio::sync::RwLock::new(None)),
            hierarchy,
            execution_type: None,
            output: Arc::new(tokio::sync::RwLock::new(None)),
        }
    }

    pub fn hierarchy_manager(&self) -> Arc<ExecutionHierarchyManager> {
        self.hierarchy.clone()
    }

    pub fn with_hierarchy_manager(mut self, manager: Arc<ExecutionHierarchyManager>) -> Self {
        self.hierarchy = manager;
        self
    }

    pub fn with_execution_type(
        mut self,
        execution_type: wf_types::workflow_execution::WorkflowExecutionType,
    ) -> Self {
        self.execution_type = Some(execution_type);
        self
    }

    pub fn execution_type(&self) -> Option<wf_types::workflow_execution::WorkflowExecutionType> {
        self.execution_type.clone()
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

    pub fn child_ids(&self) -> Vec<Id> {
        let mut ids: Vec<Id> = self
            .hierarchy
            .children()
            .into_iter()
            .map(|c| c.child_id)
            .collect();
        ids.sort();
        ids
    }

    pub fn parent_execution_id(&self) -> Option<Id> {
        self.hierarchy.parent_id()
    }

    pub fn ancestors(&self) -> Vec<Id> {
        self.hierarchy.ancestors()
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
        let already = self
            .hierarchy
            .children()
            .iter()
            .any(|c| c.child_id == child_id);
        if already {
            return;
        }
        self.hierarchy
            .register_child_ref(wf_types::execution::ChildExecutionReference {
                child_type: wf_types::execution::ExecutionType::Workflow,
                child_id,
                created_at: wf_common::now(),
                fork_path: None,
            });
    }

    pub async fn register_child_ref(
        &self,
        child_ref: wf_types::execution::ChildExecutionReference,
    ) {
        self.hierarchy.register_child_ref(child_ref);
    }

    pub async fn unregister_child(&self, child_id: &Id) {
        for child_type in [
            wf_types::execution::ExecutionType::Workflow,
            wf_types::execution::ExecutionType::AgentLoop,
        ] {
            if self.hierarchy.remove_child(child_id, &child_type) {
                break;
            }
        }
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
        self.hierarchy.depth()
    }

    fn get_root_execution_id(&self) -> Option<Id> {
        Some(self.hierarchy.root_execution_id())
    }

    fn get_ancestors(&self) -> Vec<Id> {
        self.hierarchy.ancestors()
    }

    fn hierarchy_manager(
        &self,
    ) -> Option<Arc<wf_core::hierarchy::manager::ExecutionHierarchyManager>> {
        Some(self.hierarchy.clone())
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
        use std::sync::Arc;
        use wf_core::hierarchy::manager::ExecutionHierarchyManager;
        use wf_types::execution::ExecutionHierarchy;
        // Root: no parent -> self.
        let root = WorkflowExecutionEntity::new("root".to_string(), "wf-1".to_string());
        assert_eq!(root.get_root_execution_id(), Some("root".to_string()));
        assert_eq!(root.get_ancestors(), Vec::<Id>::new());
        assert_eq!(root.get_hierarchy_depth(), 0);

        // Child derived from the root manager -> parent is the root.
        let child_manager = Arc::new(ExecutionHierarchyManager::new(
            "root".to_string(),
            wf_types::execution::ExecutionType::Workflow,
        ))
        .derive_child(
            "child".to_string(),
            wf_types::execution::ExecutionType::Workflow,
            None,
        )
        .expect("derive");
        let child = WorkflowExecutionEntity::new("child".to_string(), "wf-1".to_string())
            .with_hierarchy_manager(child_manager);
        assert_eq!(child.get_root_execution_id(), Some("root".to_string()));

        // Grandchild restored from a snapshot hierarchy -> oldest ancestor
        // is the root, depth and chain preserved.
        let hierarchy = ExecutionHierarchy {
            workflow_id: "wf-1".to_string(),
            execution_id: "gc".to_string(),
            parent_execution_id: Some("child".to_string()),
            parent_execution_type: Some(wf_types::execution::ExecutionType::Workflow),
            depth: 2,
            root_execution_id: Some("root".to_string()),
            root_execution_type: Some(wf_types::execution::ExecutionType::Workflow),
            ancestors: Some(vec!["root".to_string(), "child".to_string()]),
            children: None,
        };
        let restored_manager = ExecutionHierarchyManager::restore(
            "gc".to_string(),
            wf_types::execution::ExecutionType::Workflow,
            &hierarchy,
            wf_types::execution::ExecutionType::Workflow,
        );
        let grandchild = WorkflowExecutionEntity::new("gc".to_string(), "wf-1".to_string())
            .with_hierarchy_manager(restored_manager);
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
        assert_eq!(
            entity.child_ids().as_slice(),
            &["child-1".to_string(), "child-2".to_string()]
        );
        entity.unregister_child(&"child-1".to_string()).await;
        assert_eq!(entity.child_ids().as_slice(), &["child-2".to_string()]);
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
