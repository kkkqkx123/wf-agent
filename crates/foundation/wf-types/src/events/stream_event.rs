//! Client-facing execution stream protocol.
//!
//! The vocabulary describes the execution itself (LLM streaming, tool
//! lifecycle, iteration boundaries, outcome) and is engine-neutral: agent
//! and workflow are peer engines behind this contract. Engine-internal loop
//! events are adapted to this protocol at the API boundary; engine lifecycle
//! events published on the shared bus arrive as [`ExecutionStreamEvent::Engine`].
//!
//! This lives in foundation so the server, the generated HTTP client, and the
//! CLI frontends share one wire contract without engine coupling.

use serde_json::Value;

use super::base::BaseEvent;

/// Event produced by a streaming execution.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExecutionStreamEvent {
    /// Engine lifecycle event published on the shared event bus
    /// (workflow/node/checkpoint lifecycle, ...).
    Engine(BaseEvent),
    /// One execution round started. `message_count` / `array_version`
    /// anchor the conversation ledger at the round boundary.
    IterationStart {
        iteration: u32,
        message_count: usize,
        array_version: u64,
    },
    /// Incremental LLM output text.
    LlmDelta { content: String },
    /// A tool invocation started.
    ToolStart {
        tool_call_id: String,
        tool_name: String,
    },
    /// A tool invocation finished.
    ToolEnd {
        tool_call_id: String,
        tool_name: String,
        success: bool,
        result: String,
        /// Structured rejection/execution reason extracted from the tool
        /// result when `success` is false; `None` when no reason is
        /// available.
        #[serde(default)]
        error: Option<String>,
        /// Frontend-facing summary produced alongside the full result.
        /// `None` on older servers; frontends fall back to name-based
        /// inference and generic truncation.
        #[serde(default)]
        display: Option<crate::tool::DisplayPayload>,
    },
    /// One execution round finished.
    IterationEnd {
        iteration: u32,
        message_count: usize,
        array_version: u64,
    },
    /// The execution was interrupted before reaching an outcome.
    Interrupted { reason: String },
    /// Terminal success payload.
    Completed { result: Value, iterations: u32 },
    /// Terminal failure payload.
    Failed { error: String },
    /// Incremental reasoning / thinking text delta (forwarded from the agent
    /// loop; rendered distinctly from the assistant answer).
    ReasoningDelta { content: String },
    /// Cumulative token usage + estimated cost for the run.
    Usage {
        prompt_tokens: u32,
        completion_tokens: u32,
        cost: Option<f64>,
    },
    /// A sub-agent (triggered child agent) started.
    SubAgentStarted { id: String, name: String },
    /// A sub-agent finished.
    SubAgentEnded {
        id: String,
        name: String,
        success: bool,
    },
}
