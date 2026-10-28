use std::sync::Arc;

use async_trait::async_trait;
use wf_core::hierarchy::manager::ExecutionHierarchyManager;
use crate::types::execution_entity::ExecutionStatus;
use wf_types::Id;

/// Where a runtime append input is buffered by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AgentAppendMode {
    Followup,
    Steering,
}

/// Minimal control surface over a running agent loop, defined in the shared
/// execution layer so workflow (and other engines) can address nested loops
/// without depending on `wf-agent`.
#[async_trait]
pub trait AgentLoopControl: Send + Sync {
    fn agent_loop_id(&self) -> Id;
    fn status(&self) -> ExecutionStatus;
    fn is_terminal(&self) -> bool;
    fn parent_execution_id(&self) -> Option<Id>;
    fn hierarchy_manager(&self) -> Option<Arc<ExecutionHierarchyManager>>;
    async fn append_runtime_input(
        &self,
        text: String,
        mode: Option<AgentAppendMode>,
    ) -> AgentAppendMode;
    async fn set_append_mode(&self, mode: AgentAppendMode);
    async fn append_mode(&self) -> AgentAppendMode;
    async fn convert_pending_to_steering(&self);
    async fn pending_append_counts(&self) -> (usize, usize);
    async fn register_child(&self, child_id: Id);

    fn as_any(&self) -> &dyn std::any::Any;
}

/// Registry handle used to register and look up agent loops by id. Workflow
/// nodes receive this handle (instead of the concrete `wf-agent` registry)
/// so the shared layer stays free of a reverse dependency.
#[async_trait]
pub trait AgentLoopRegistryHandle: Send + Sync {
    fn register_control(&self, control: Arc<dyn AgentLoopControl>) -> Result<(), String>;
    fn unregister(&self, id: &Id) -> bool;
    fn get(&self, id: &Id) -> Option<Arc<dyn AgentLoopControl>>;
    fn has(&self, id: &Id) -> bool;
    fn all_ids(&self) -> Vec<Id>;

    fn as_any(&self) -> &dyn std::any::Any;
}
