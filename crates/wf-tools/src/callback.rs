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
    /// Optional name of a runtime-registered hook receiver; the engine
    /// notifies it synchronously at this hook point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receiver: Option<String>,
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
    /// Tools visible in the initial schema; when absent all available tools
    /// are initially visible.
    pub initial_tool_names: Vec<String>,
    /// Discoverable tools: only metadata is injected into the prompt and
    /// calls go through the `general` tool; schema injection happens only
    /// when a tool is activated via TOOL_VISIBILITY unblock.
    pub discoverable_tool_names: Vec<String>,
    /// Escape hatch controlling `general` tool exposure. Defaults to auto:
    /// exposed iff the discoverable list is non-empty.
    pub enable_general_tool: Option<bool>,
    /// Tools formally activated before the loop starts (seeded from the
    /// workflow's TOOL_VISIBILITY unblock markers); gated tools in this set
    /// enter the visible schema from the first iteration.
    pub activated_tool_names: Vec<String>,
    /// Explicitly hidden tools: registered but never exposed to the model
    /// (supplements runtime visibility blocking).
    pub hidden_tool_names: Vec<String>,
    /// Protocol lock for tool calls (e.g. XML wrapping).
    pub tool_call_format: Option<wf_types::llm::tool_call_format::ToolCallFormatConfig>,
    /// Cumulative token limit for the agent conversation; 0 disables
    /// limit checks and warning events.
    pub token_limit: Option<u64>,
    /// Warning threshold percentage of the token limit (default 80).
    pub token_warning_threshold: Option<u32>,
    /// Token usage tracking switch; enabled by default and only disabled
    /// by an explicit `false`. When disabled,
    /// usage is not recorded and no token events are emitted.
    pub enable_token_tracking: Option<bool>,
    /// Pre-rendered description for the `general` tool, rendered at loop
    /// assembly time from the `tool-visibility.general_description` resource
    /// template (so the text follows custom resource overrides and the tool
    /// call format). `None` keeps the builtin static description.
    pub general_description: Option<String>,
    /// Pre-rendered discoverable-tool metadata block (from the
    /// `tool-visibility.discoverable_metadata` resource template). The
    /// engine injects it into the system prompt at request assembly time
    /// (placeholder replacement or tail append). `None` falls back to the
    /// built-in metadata generation.
    pub discoverable_metadata_block: Option<String>,
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
    /// Per-run agent loop id (independent of the agent definition id).
    pub agent_loop_id: Id,
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
    /// Final result carried by terminal (completed/failed) executions;
    /// `None` while the execution is still running.
    pub result: Option<Value>,
}

/// Handle returned by an asynchronous agent dispatch: the execution runs in
/// the background and its result is retrieved through
/// [`ExecutionCallback::query_execution_status`].
#[derive(Debug, Clone)]
pub struct SpawnedAgentLoop {
    pub agent_loop_id: Id,
    pub execution_id: Id,
    pub status: String,
}

/// Handle returned by an asynchronous workflow dispatch: the execution runs
/// in the background and its result is retrieved through
/// [`ExecutionCallback::query_execution_status`].
#[derive(Debug, Clone)]
pub struct SpawnedWorkflow {
    pub execution_id: Id,
    pub status: String,
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

    /// Dispatch an agent loop in the background and return immediately. The
    /// completion result is later retrieved via `query_execution_status`.
    async fn spawn_agent_loop(
        &self,
        _config: AgentLoopConfig,
        _input: AgentLoopInput,
    ) -> ToolResult<SpawnedAgentLoop> {
        Err(ToolError::ExecutionError(
            "spawn_agent_loop is not supported by this callback".to_string(),
        ))
    }

    /// Dispatch a workflow in the background and return immediately. The
    /// completion result is later retrieved via `query_execution_status`.
    async fn spawn_workflow(
        &self,
        _workflow_id: &str,
        _input: WorkflowInput,
    ) -> ToolResult<SpawnedWorkflow> {
        Err(ToolError::ExecutionError(
            "spawn_workflow is not supported by this callback".to_string(),
        ))
    }
}

/// The global execution callback slot. Unlike a one-shot cell it allows
/// re-registration (last wins): a runtime bootstrap replaces any previous
/// composite, which keeps embedded runtimes and test processes working.
/// Every registration is expected to cover the full method family; a
/// partial stub would silently degrade the other engines' paths.
static CALLBACK: std::sync::Mutex<Option<Arc<dyn ExecutionCallback>>> = std::sync::Mutex::new(None);

pub fn register_execution_callback(callback: Arc<dyn ExecutionCallback>) -> ToolResult<()> {
    *wf_common::lock::lock_ok(CALLBACK.lock()) = Some(callback);
    Ok(())
}

pub fn get_execution_callback() -> Option<Arc<dyn ExecutionCallback>> {
    wf_common::lock::lock_ok(CALLBACK.lock()).clone()
}

pub fn is_callback_registered() -> bool {
    wf_common::lock::lock_ok(CALLBACK.lock()).is_some()
}
