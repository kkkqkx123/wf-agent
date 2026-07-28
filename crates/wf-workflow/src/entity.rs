use std::sync::Arc;

use dashmap::DashMap;
use serde_json::Value;
use wf_execution_shared::interruption::InterruptionState;
use wf_execution_shared::types::execution_entity::{ExecutionStatus, IExecutionEntity};
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
        }
    }

    pub fn with_parent_execution_id(mut self, parent_id: Id) -> Self {
        self.parent_execution_id = Some(parent_id);
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
        self.child_execution_ids.write().await.retain(|id| id != child_id);
    }
}

#[async_trait::async_trait]
impl IExecutionEntity for WorkflowExecutionEntity {
    fn id(&self) -> &Id {
        &self.id
    }

    fn status(&self) -> ExecutionStatus {
        futures::executor::block_on(async {
            self.state.read().await.status()
        })
    }

    fn is_running(&self) -> bool {
        futures::executor::block_on(async {
            self.state.read().await.is_running()
        })
    }

    fn is_paused(&self) -> bool {
        futures::executor::block_on(async {
            self.state.read().await.is_paused()
        })
    }

    fn is_completed(&self) -> bool {
        futures::executor::block_on(async {
            self.state.read().await.is_completed()
        })
    }

    fn is_failed(&self) -> bool {
        futures::executor::block_on(async {
            self.state.read().await.is_failed()
        })
    }

    fn is_cancelled(&self) -> bool {
        futures::executor::block_on(async {
            self.state.read().await.is_cancelled()
        })
    }

    async fn pause(&self) -> Result<(), wf_execution_shared::error::ExecutionSharedError> {
        self.interruption.pause()?;
        self.state.write().await.pause();
        Ok(())
    }

    async fn resume(&self) -> Result<(), wf_execution_shared::error::ExecutionSharedError> {
        self.state.write().await.resume();
        Ok(())
    }

    async fn stop(&self) -> Result<(), wf_execution_shared::error::ExecutionSharedError> {
        self.interruption.stop()?;
        self.cancellation.cancel();
        self.state.write().await.cancel();
        Ok(())
    }

    async fn abort(&self) {
        self.cancellation.cancel();
    }

    fn get_abort_signal(&self) -> tokio_util::sync::CancellationToken {
        self.cancellation.clone()
    }

    fn get_hierarchy_depth(&self) -> u32 {
        0
    }

    fn get_root_execution_id(&self) -> Option<Id> {
        Some(self.id.clone())
    }
}
