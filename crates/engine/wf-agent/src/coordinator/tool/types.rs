use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use wf_common::retry::RetryBudget;
use wf_metrics::MetricsRegistry;
use wf_tools::failure_protection::ToolFailureProtectionState;
use wf_tools::registry::ToolRegistry;
use wf_types::message::Message;

use crate::error::AgentResult;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ToolExecutionMode {
    #[default]
    Sequential,
    Parallel,
}

/// Per-tool-call decision produced by the approval engine.
#[derive(Debug, Clone)]
pub(crate) enum ApprovalOutcome {
    Execute { edited_parameters: Option<Value> },
    Rejected { reason: String },
}

/// Phase of a single tool execution reported through the progress channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolProgressStatus {
    Started,
    Completed,
    Failed,
    Cancelled,
}

/// A progress event emitted for a single tool call.
#[derive(Debug, Clone)]
pub struct ToolProgressEvent {
    pub tool_call_id: String,
    pub status: ToolProgressStatus,
    pub partial: Option<Value>,
}

/// Optional visibility gate applied before tool execution. Used to hide
/// tools from an execution without removing them from the registry.
#[async_trait]
pub trait ToolVisibilityStore: Send + Sync {
    async fn is_tool_visible(&self, execution_id: &str, tool_name: &str) -> bool;
}

/// Optional execution-state snapshot callback invoked around tool
/// executions that opt in via `ToolMetadata::create_checkpoint`.
/// Terminology: this snapshots the execution record (tool `create_checkpoint`
/// timing) and is unrelated to file-content checkpoints; file changes use
/// the file observer (`with_file_observer`), never this callback.
#[async_trait]
pub trait ToolCheckpointHandler: Send + Sync {
    async fn create_checkpoint(&self, execution_id: &str, reason: &str) -> AgentResult<()>;
}

/// Per-task outcome produced by the parallel execution path.
pub(crate) enum TaskOutcome {
    Ok(Message),
    /// Gate rejection surfaced as a rejection message (same shape as the
    /// sequential path), not an execution failure: it never triggers the
    /// batch abort of sibling tasks.
    Rejected(Message),
    Failed(wf_tools::error::ToolError),
}

/// Immutable execution context shared by sequential and parallel tool runs.
#[derive(Clone)]
pub(crate) struct ToolRunCtx {
    pub(crate) registry: Arc<ToolRegistry>,
    pub(crate) metrics: Option<Arc<MetricsRegistry>>,
    pub(crate) progress_tx: Option<tokio::sync::mpsc::Sender<ToolProgressEvent>>,
    /// Approval wiring snapshot for the `general` inner path: the proxy
    /// re-runs the same approval gate per inner tool so high-risk tools
    /// cannot bypass approval through the discoverable path. The snapshot
    /// is taken when the execution context is built (approval must be
    /// wired before the `general` invoker is installed).
    pub(crate) approval_options: Option<wf_types::tool::approval::ToolApprovalOptions>,
    pub(crate) approval_handler: Option<Arc<dyn crate::approval::ToolApprovalHandler>>,
    /// Execution-state snapshot callback (tool `create_checkpoint` timing).
    /// This is unrelated to file-content checkpoints: it snapshots the
    /// execution record, never file bytes. File changes use `file_observer`.
    pub(crate) checkpoint_handler: Option<Arc<dyn ToolCheckpointHandler>>,
    pub(crate) failure_protection: Option<Arc<ToolFailureProtectionState>>,
    pub(crate) visibility_store: Option<Arc<dyn ToolVisibilityStore>>,
    pub(crate) general_invoker: Option<Arc<dyn wf_tools::general::GeneralToolInvoker>>,
    pub(crate) retry_budget: Option<Arc<RetryBudget>>,
    /// File-content observation (agent actor partition). Injected separately
    /// from `checkpoint_handler` so callers cannot confuse execution-state
    /// snapshots with file-content checkpoints.
    pub(crate) checkpoint_session: Option<wf_checkpoint::CheckpointSession>,
    /// Abort signal observed while a tool runs; `None` keeps plain tool
    /// behavior. Set from the owning entity before dispatch.
    pub(crate) cancellation: Option<tokio_util::sync::CancellationToken>,
}
