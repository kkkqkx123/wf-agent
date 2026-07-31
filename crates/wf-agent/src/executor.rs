use async_trait::async_trait;

use wf_llm::LlmWrapper;
use wf_tools::callback::{
    AgentLoopConfig, AgentLoopInput, AgentLoopOutput, ExecutionCallback, ExecutionStatus,
    WorkflowInput, WorkflowOutput,
};
use wf_tools::error::ToolResult;
use wf_tools::registry::ToolRegistry;

use crate::coordinator::lifecycle::AgentLoopCoordinator;
use crate::error::AgentResult;

pub struct AgentLoopExecutor {
    llm_wrapper: std::sync::Arc<LlmWrapper>,
    registry: std::sync::Arc<ToolRegistry>,
    max_iterations: u32,
}

impl AgentLoopExecutor {
    pub fn new(
        llm_wrapper: std::sync::Arc<LlmWrapper>,
        registry: std::sync::Arc<ToolRegistry>,
    ) -> Self {
        Self {
            llm_wrapper,
            registry,
            max_iterations: 10,
        }
    }

    pub fn with_max_iterations(mut self, max: u32) -> Self {
        self.max_iterations = max;
        self
    }

    pub async fn execute(
        &self,
        config: AgentLoopConfig,
        input: AgentLoopInput,
    ) -> AgentResult<AgentLoopOutput> {
        let coordinator =
            AgentLoopCoordinator::new(self.llm_wrapper.clone(), self.registry.clone());
        coordinator.execute(config, input).await
    }
}

#[async_trait]
impl ExecutionCallback for AgentLoopExecutor {
    async fn execute_agent_loop(
        &self,
        config: AgentLoopConfig,
        input: AgentLoopInput,
    ) -> ToolResult<AgentLoopOutput> {
        self.execute(config, input)
            .await
            .map_err(|e| wf_tools::error::ToolError::ExecutionError(e.to_string()))
    }

    async fn execute_workflow(
        &self,
        workflow_id: &str,
        _input: WorkflowInput,
    ) -> ToolResult<WorkflowOutput> {
        Ok(WorkflowOutput {
            execution_id: workflow_id.to_string(),
            result: serde_json::Value::String(format!(
                "Workflow {} execution not yet implemented",
                workflow_id
            )),
            performance: None,
        })
    }

    async fn query_execution_status(&self, execution_id: &str) -> ToolResult<ExecutionStatus> {
        Ok(ExecutionStatus {
            execution_id: execution_id.to_string(),
            status: "unknown".to_string(),
            progress: None,
        })
    }

    async fn cancel_execution(&self, execution_id: &str) -> ToolResult<()> {
        tracing::info!("Cancel execution: {}", execution_id);
        Ok(())
    }
}
