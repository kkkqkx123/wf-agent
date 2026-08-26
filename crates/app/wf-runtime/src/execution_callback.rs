//! Runtime assembly of the engine execution callbacks into one
//! [`ExecutionCallback`] covering agent and workflow dispatch.
//!
//! The composite is registered both on the global callback singleton and on
//! the shared tool registry at bootstrap, so builtin dispatch tools
//! (`call_agent` / `execute_workflow` / `query_workflow_status` /
//! `cancel_workflow`) reach the engines in production instead of failing
//! with `CallbackNotRegistered`.

use std::sync::Arc;

use async_trait::async_trait;
use wf_agent::executor::AgentLoopExecutor;
use wf_agent::registry::AgentLoopRegistry;
use wf_tools::callback::{
    AgentLoopConfig, AgentLoopInput, AgentLoopOutput, ExecutionCallback, ExecutionStatus,
    SpawnedAgentLoop, SpawnedWorkflow, WorkflowInput, WorkflowOutput,
};
use wf_tools::error::{ToolError, ToolResult};
use wf_types::Id;
use wf_workflow::execution_callback::WorkflowExecutionCallback;

/// Composite `ExecutionCallback`: routes each method family to the engine
/// that owns it. Query/cancel resolve agent executions first and fall back
/// to workflow executions (an execution id belongs to exactly one engine).
pub struct CompositeExecutionCallback {
    agent: Option<Arc<AgentLoopExecutor>>,
    workflow: Option<Arc<WorkflowExecutionCallback>>,
}

impl CompositeExecutionCallback {
    pub fn new() -> Self {
        Self {
            agent: None,
            workflow: None,
        }
    }

    /// Configure the agent engine. The executor brings its own shared
    /// registry, which the composite uses to route query/cancel.
    pub fn with_agent(mut self, executor: Arc<AgentLoopExecutor>) -> Self {
        self.agent = Some(executor);
        self
    }

    /// Configure the workflow engine.
    pub fn with_workflow(mut self, callback: Arc<WorkflowExecutionCallback>) -> Self {
        self.workflow = Some(callback);
        self
    }

    fn require_agent(&self) -> ToolResult<&Arc<AgentLoopExecutor>> {
        self.agent
            .as_ref()
            .ok_or_else(|| ToolError::ExecutionError("agent engine is not configured".to_string()))
    }

    fn require_workflow(&self) -> ToolResult<&Arc<WorkflowExecutionCallback>> {
        self.workflow.as_ref().ok_or_else(|| {
            ToolError::ExecutionError("workflow engine is not configured".to_string())
        })
    }

    /// The shared agent loop registry of the configured agent engine, when
    /// one is wired (used by the runtime to share the execution view with
    /// the API context).
    pub fn agent_registry(&self) -> Option<Arc<AgentLoopRegistry>> {
        self.agent.as_ref().map(|a| a.agent_registry().clone())
    }
}

impl Default for CompositeExecutionCallback {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ExecutionCallback for CompositeExecutionCallback {
    async fn execute_agent_loop(
        &self,
        config: AgentLoopConfig,
        input: AgentLoopInput,
    ) -> ToolResult<AgentLoopOutput> {
        self.require_agent()?
            .execute_agent_loop(config, input)
            .await
    }

    async fn spawn_agent_loop(
        &self,
        config: AgentLoopConfig,
        input: AgentLoopInput,
    ) -> ToolResult<SpawnedAgentLoop> {
        self.require_agent()?
            .spawn_agent_loop(config, input)
            .await
            .map_err(|e| ToolError::ExecutionError(e.to_string()))
    }

    async fn execute_workflow(
        &self,
        workflow_id: &str,
        input: WorkflowInput,
    ) -> ToolResult<WorkflowOutput> {
        self.require_workflow()?
            .execute_workflow(workflow_id, input)
            .await
    }

    async fn spawn_workflow(
        &self,
        workflow_id: &str,
        input: WorkflowInput,
    ) -> ToolResult<SpawnedWorkflow> {
        self.require_workflow()?
            .spawn_workflow(workflow_id, input)
            .await
            .map_err(|e| ToolError::ExecutionError(e.to_string()))
    }

    async fn query_execution_status(&self, execution_id: &str) -> ToolResult<ExecutionStatus> {
        let id = Id::from(execution_id.to_string());
        // Agent executions take precedence; workflow executions fall back.
        if let Some(agent) = &self.agent {
            if agent.agent_registry().has(&id) {
                return agent.query_execution_status(execution_id).await;
            }
        }
        self.require_workflow()?
            .query_execution_status(execution_id)
            .await
    }

    async fn cancel_execution(&self, execution_id: &str) -> ToolResult<()> {
        let id = Id::from(execution_id.to_string());
        if let Some(agent) = &self.agent {
            if agent.agent_registry().has(&id) {
                return agent.cancel_execution(execution_id).await;
            }
        }
        self.require_workflow()?
            .cancel_execution(execution_id)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unconfigured_engines_report_explicit_errors() {
        let composite = CompositeExecutionCallback::new();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let err = rt.block_on(composite.execute_agent_loop(
            AgentLoopConfig {
                agent_id: Id::from("a".to_string()),
                model: "m".to_string(),
                max_iterations: None,
                max_execution_time: None,
                hooks: Vec::new(),
                available_tool_names: Vec::new(),
                initial_tool_names: Vec::new(),
                discoverable_tool_names: Vec::new(),
                enable_general_tool: None,
                activated_tool_names: Vec::new(),
                hidden_tool_names: Vec::new(),
                tool_call_format: None,
                token_limit: None,
                token_warning_threshold: None,
                enable_token_tracking: None,
                general_description: None,
                discoverable_metadata_block: None,
            },
            AgentLoopInput {
                message: "x".to_string(),
                context: std::collections::HashMap::new(),
                conversation: Vec::new(),
            },
        ));
        assert!(matches!(err, Err(ToolError::ExecutionError(_))));
    }
}
