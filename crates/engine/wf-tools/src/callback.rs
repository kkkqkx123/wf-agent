use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

use crate::error::{ToolError, ToolResult};
use wf_types::Id;

pub use wf_types::agent_execution::{AgentLoopConfig, HookConfig};

/// Typed parent execution link carried across the tool boundary.
///
/// The hierarchy manager handle is captured at the call site, so the child
/// links to the parent that was live when the tool ran instead of
/// re-resolving a bare id against a registry later. A missing link means
/// the child runs as a root execution.
#[derive(Clone)]
pub struct ParentLink {
    pub execution_id: Id,
    pub manager: Arc<wf_core::hierarchy::manager::ExecutionHierarchyManager>,
    /// Abort signal of the parent execution; stopping the parent stops the
    /// child. `None` keeps plain behavior with no cross-execution cancel.
    pub cancellation: Option<tokio_util::sync::CancellationToken>,
}

impl std::fmt::Debug for ParentLink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParentLink")
            .field("execution_id", &self.execution_id)
            .field("cancellation", &self.cancellation.is_some())
            .finish()
    }
}
#[derive(Debug, Clone)]
pub struct AgentLoopInput {
    pub message: String,
    pub context: HashMap<String, Value>,
    /// Initial conversation imported into the agent session.
    pub conversation: Vec<wf_types::message::Message>,
}

/// Why an agent loop run ended. Machine-readable terminal classification that
/// consumers must not have to recover from the result content string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopFinishReason {
    /// The model produced a final answer (or `attempt_completion`).
    Completed,
    /// The iteration budget was exhausted without a final answer.
    MaxIterationsReached,
    /// The run stopped early on an interruption (pause/cancel race) rather
    /// than on model output.
    Interrupted,
}

impl LoopFinishReason {
    /// Stable wire name for payloads and logs.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::MaxIterationsReached => "max_iterations_reached",
            Self::Interrupted => "interrupted",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AgentLoopOutput {
    /// Per-run agent loop id (independent of the agent definition id).
    pub agent_loop_id: Id,
    pub result: Value,
    pub iterations: u32,
    /// Terminal classification of the run (see `LoopFinishReason`).
    pub finish_reason: LoopFinishReason,
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

    /// Synchronous agent dispatch linked under a live parent execution. The
    /// default ignores the link and runs as a root; engines that track
    /// hierarchies override it.
    async fn execute_agent_loop_with_parent(
        &self,
        config: AgentLoopConfig,
        input: AgentLoopInput,
        _parent: Option<ParentLink>,
    ) -> ToolResult<AgentLoopOutput> {
        self.execute_agent_loop(config, input).await
    }

    /// Background agent dispatch linked under a live parent execution. The
    /// default ignores the link and runs as a root; engines that track
    /// hierarchies override it.
    async fn spawn_agent_loop_with_parent(
        &self,
        config: AgentLoopConfig,
        input: AgentLoopInput,
        _parent: Option<ParentLink>,
    ) -> ToolResult<SpawnedAgentLoop> {
        self.spawn_agent_loop(config, input).await
    }

    /// Synchronous workflow dispatch linked under a live parent execution.
    /// The default ignores the link and runs as a root; engines that track
    /// hierarchies override it.
    async fn execute_workflow_with_parent(
        &self,
        workflow_id: &str,
        input: WorkflowInput,
        _parent: Option<ParentLink>,
    ) -> ToolResult<WorkflowOutput> {
        self.execute_workflow(workflow_id, input).await
    }

    /// Background workflow dispatch linked under a live parent execution.
    /// The default ignores the link and runs as a root; engines that track
    /// hierarchies override it.
    async fn spawn_workflow_with_parent(
        &self,
        workflow_id: &str,
        input: WorkflowInput,
        _parent: Option<ParentLink>,
    ) -> ToolResult<SpawnedWorkflow> {
        self.spawn_workflow(workflow_id, input).await
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
