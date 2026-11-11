use async_trait::async_trait;

use wf_tools::callback::{
    AgentLoopConfig, AgentLoopInput, AgentLoopOutput, ExecutionCallback, ExecutionStatus,
    ParentLink, SpawnedAgentLoop, WorkflowInput, WorkflowOutput,
};
use wf_tools::error::{ToolError, ToolResult};
use wf_types::Id;

use crate::error::AgentError;

use super::AgentLoopExecutor;

/// Map a typed agent error onto the tool-error channel so the agent-as-tool
/// path keeps cancellation, timeout and validation semantics distinguishable
/// instead of collapsing everything into one string bucket.
fn agent_error_to_tool_error(error: AgentError, tool_id: String, timeout_ms: u64) -> ToolError {
    match error {
        AgentError::Cancelled(_) => ToolError::Cancelled { tool_id },
        AgentError::ExecutionTimeout(_) => ToolError::Timeout {
            tool_id,
            timeout_ms,
        },
        AgentError::Validation(reason) => ToolError::ValidationFailed(reason),
        other => ToolError::ExecutionError(other.to_string()),
    }
}

#[async_trait]
impl ExecutionCallback for AgentLoopExecutor {
    async fn execute_agent_loop(
        &self,
        config: AgentLoopConfig,
        input: AgentLoopInput,
    ) -> ToolResult<AgentLoopOutput> {
        let tool_id = config.agent_id.to_string();
        let timeout_ms = config.max_execution_time.unwrap_or(0);
        self.execute_with_parent(config, input, None)
            .await
            .map_err(|e| agent_error_to_tool_error(e, tool_id.clone(), timeout_ms))
    }

    async fn execute_agent_loop_with_parent(
        &self,
        config: AgentLoopConfig,
        input: AgentLoopInput,
        parent: Option<ParentLink>,
    ) -> ToolResult<AgentLoopOutput> {
        let tool_id = config.agent_id.to_string();
        let timeout_ms = config.max_execution_time.unwrap_or(0);
        self.execute_with_parent(config, input, parent)
            .await
            .map_err(|e| agent_error_to_tool_error(e, tool_id.clone(), timeout_ms))
    }

    async fn spawn_agent_loop(
        &self,
        config: AgentLoopConfig,
        input: AgentLoopInput,
    ) -> ToolResult<SpawnedAgentLoop> {
        let tool_id = config.agent_id.to_string();
        let timeout_ms = config.max_execution_time.unwrap_or(0);
        self.spawn_with_parent(config, input, None)
            .await
            .map_err(|e| agent_error_to_tool_error(e, tool_id.clone(), timeout_ms))
    }

    async fn spawn_agent_loop_with_parent(
        &self,
        config: AgentLoopConfig,
        input: AgentLoopInput,
        parent: Option<ParentLink>,
    ) -> ToolResult<SpawnedAgentLoop> {
        let tool_id = config.agent_id.to_string();
        let timeout_ms = config.max_execution_time.unwrap_or(0);
        self.spawn_with_parent(config, input, parent)
            .await
            .map_err(|e| agent_error_to_tool_error(e, tool_id.clone(), timeout_ms))
    }

    async fn execute_workflow(
        &self,
        workflow_id: &str,
        _input: WorkflowInput,
    ) -> ToolResult<WorkflowOutput> {
        Err(ToolError::ExecutionError(format!(
            "workflow execution is not supported by AgentLoopExecutor: {}",
            workflow_id
        )))
    }

    async fn query_execution_status(
        &self,
        execution_id: &str,
        caller_root: &Id,
    ) -> ToolResult<ExecutionStatus> {
        self.control_query(execution_id, caller_root).await
    }

    async fn cancel_execution(&self, execution_id: &str, caller_root: &Id) -> ToolResult<()> {
        self.control_cancel(execution_id, caller_root).await
    }
}
