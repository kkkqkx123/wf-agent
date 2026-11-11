use wf_execution_shared::types::execution_entity::ExecutionEntity;
use wf_tools::callback::ExecutionStatus;
use wf_tools::error::{ToolError, ToolResult};
use wf_types::Id;

use super::AgentLoopExecutor;

pub(super) fn status_string(
    status: &wf_execution_shared::types::execution_entity::ExecutionStatus,
) -> String {
    use wf_execution_shared::types::execution_entity::ExecutionStatus as S;
    match status {
        S::Created => "created".to_string(),
        S::Running => "running".to_string(),
        S::Paused => "paused".to_string(),
        S::Completed => "completed".to_string(),
        S::Failed => "failed".to_string(),
        S::Cancelled => "cancelled".to_string(),
        S::Stopped => "stopped".to_string(),
        S::Timeout => "timeout".to_string(),
    }
}

impl AgentLoopExecutor {
    pub(super) async fn control_query(
        &self,
        execution_id: &str,
        caller_root: &Id,
    ) -> ToolResult<ExecutionStatus> {
        let id = Id::from(execution_id.to_string());
        let entity = self
            .agent_registry
            .get(&id)
            .ok_or_else(|| ToolError::NotFound(format!("execution {} not found", execution_id)))?;
        wf_tools::callback::require_same_subtree(
            &entity.as_ref().hierarchy_manager(),
            caller_root,
        )?;
        let state = entity.state.read().await;
        let status = state.status();
        // Terminal executions expose their stored output. Reading is
        // repeatable: every query on a terminal execution returns the same
        // result until `cleanup_terminated` removes it. A failed run has no
        // stored output; the settled error string is handed over instead so
        // the failure reason reaches the polling caller.
        let result = if matches!(
            status,
            wf_execution_shared::types::execution_entity::ExecutionStatus::Completed
                | wf_execution_shared::types::execution_entity::ExecutionStatus::Failed
                | wf_execution_shared::types::execution_entity::ExecutionStatus::Cancelled
                | wf_execution_shared::types::execution_entity::ExecutionStatus::Stopped
        ) {
            self.agent_registry
                .result(&id)
                .map(|output| output.result)
                .or_else(|| state.error().map(|e| serde_json::json!({ "error": e })))
        } else {
            None
        };
        Ok(ExecutionStatus {
            execution_id: id,
            status: status_string(&status),
            progress: None,
            result,
        })
    }

    pub(super) async fn control_cancel(
        &self,
        execution_id: &str,
        caller_root: &Id,
    ) -> ToolResult<()> {
        let id = Id::from(execution_id.to_string());
        let entity = self
            .agent_registry
            .get(&id)
            .ok_or_else(|| ToolError::NotFound(format!("execution {} not found", execution_id)))?;
        wf_tools::callback::require_same_subtree(
            &entity.as_ref().hierarchy_manager(),
            caller_root,
        )?;
        entity
            .stop()
            .await
            .map_err(|e| ToolError::ExecutionError(e.to_string()))?;
        self.agent_registry.abort_task(&id);
        Ok(())
    }
}
