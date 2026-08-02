use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

use crate::error::{ToolError, ToolResult};
use wf_types::Id;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HookConfig {
    pub hook_type: String,
    pub condition: Option<String>,
    pub enabled: bool,
    pub parallel: Option<bool>,
    pub continue_on_error: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct AgentLoopConfig {
    pub agent_id: Id,
    /// Profile id the agent loop runs against (mandatory).
    pub model: String,
    pub max_iterations: Option<u32>,
    pub max_execution_time: Option<u64>,
    pub hooks: Vec<HookConfig>,
    pub available_tool_names: Vec<String>,
    /// Protocol lock for tool calls (e.g. XML wrapping).
    pub tool_call_format: Option<wf_types::llm::tool_call_format::ToolCallFormatConfig>,
    /// Cumulative token limit for the agent conversation; 0 disables
    /// limit checks and warning events.
    pub token_limit: Option<u64>,
    /// Warning threshold percentage of the token limit (default 80).
    pub token_warning_threshold: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct AgentLoopInput {
    pub message: String,
    pub context: HashMap<String, Value>,
    /// Initial conversation imported into the agent session.
    pub conversation: Vec<wf_types::message::Message>,
}

#[derive(Debug, Clone)]
pub struct AgentLoopOutput {
    pub result: Value,
    pub iterations: u32,
    /// Final conversation exported from the agent session.
    pub conversation: Vec<wf_types::message::Message>,
}

#[derive(Debug, Clone)]
pub struct WorkflowInput {
    pub variables: HashMap<String, Value>,
}

#[derive(Debug, Clone)]
pub struct WorkflowOutput {
    pub execution_id: Id,
    pub result: Value,
}

#[derive(Debug, Clone)]
pub struct ExecutionStatus {
    pub execution_id: Id,
    pub status: String,
    pub progress: Option<f64>,
}

#[async_trait]
pub trait ExecutionCallback: Send + Sync {
    async fn execute_agent_loop(
        &self,
        config: AgentLoopConfig,
        input: AgentLoopInput,
    ) -> ToolResult<AgentLoopOutput>;

    async fn execute_workflow(
        &self,
        workflow_id: &str,
        input: WorkflowInput,
    ) -> ToolResult<WorkflowOutput>;

    async fn query_execution_status(&self, execution_id: &str) -> ToolResult<ExecutionStatus>;

    async fn cancel_execution(&self, execution_id: &str) -> ToolResult<()>;
}

static CALLBACK: once_cell::sync::OnceCell<Arc<dyn ExecutionCallback>> =
    once_cell::sync::OnceCell::new();

pub fn register_execution_callback(callback: Arc<dyn ExecutionCallback>) -> ToolResult<()> {
    CALLBACK
        .set(callback)
        .map_err(|_| ToolError::AlreadyRegistered)
}

pub fn get_execution_callback() -> Option<Arc<dyn ExecutionCallback>> {
    CALLBACK.get().cloned()
}

pub fn is_callback_registered() -> bool {
    CALLBACK.get().is_some()
}
