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
        /// Structured background linkage for tools that hand a running
        /// session back instead of blocking. `None` for foreground tools
        /// and older servers; frontends must not sniff the result text.
        #[serde(default)]
        background: Option<crate::tool::BackgroundMarker>,
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
    /// A sub-agent (triggered child agent) started. `id` is the child
    /// execution id. The linkage fields are `None` on older servers;
    /// consumers fall back to name-only display.
    SubAgentStarted {
        id: String,
        name: String,
        /// Outer tool call that dispatched the child (`call_agent` /
        /// `execute_workflow`), linking the child to its host card.
        #[serde(default)]
        tool_call_id: Option<String>,
        /// Nesting depth of the child (root = 0).
        #[serde(default)]
        depth: Option<u32>,
        /// Truncated task preview (prompt first line / workflow id).
        #[serde(default)]
        preview: Option<String>,
        /// True when dispatched without waiting (background handle).
        #[serde(default)]
        background: Option<bool>,
    },
    /// A sub-agent finished.
    SubAgentEnded {
        id: String,
        name: String,
        success: bool,
        /// Outer tool call that dispatched the child, for card correlation.
        #[serde(default)]
        tool_call_id: Option<String>,
        /// True when the child was dispatched without waiting.
        #[serde(default)]
        background: Option<bool>,
    },
    /// Milestone progress of a running sub-agent (iteration / tool
    /// summary / token usage). Ephemeral like usage deltas: renderers
    /// update the host card in place and never persist it.
    SubAgentProgress {
        id: String,
        /// Outer tool call that dispatched the child, for card correlation.
        #[serde(default)]
        tool_call_id: Option<String>,
        /// Human-readable activity summary (latest milestone).
        #[serde(default)]
        activity: Option<String>,
        /// Cumulative output tokens reported by the child, when known.
        #[serde(default)]
        output_tokens: Option<u32>,
        /// Tool calls made by the child so far, when known.
        #[serde(default)]
        tool_uses: Option<u32>,
    },
}
