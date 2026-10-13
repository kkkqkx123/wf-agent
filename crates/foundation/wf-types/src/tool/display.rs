//! Display payload describing how a tool result should be presented.
//!
//! This channel is orthogonal to context folding: folding rewrites the
//! model-visible history to save tokens, while display travels only on the
//! stream event for frontends and never enters model context.

use serde::{Deserialize, Serialize};

/// Presentation kind selected by the tool that produced the result.
/// Frontends dispatch on this kind instead of matching tool names, so new
/// tools only need to pick the closest kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisplayKind {
    /// Command output: the tail carries the signal.
    ShellTail,
    /// Unified diff: `+`/`-`/`@@` rows carry the signal.
    Diff,
    /// Retrieval output: a one-line count summary replaces the body.
    Count,
    /// Generic text.
    #[default]
    Text,
}

/// Frontend-facing summary of one tool result, produced alongside the full
/// result. `preview` is already collapsed to a small excerpt; the full text
/// stays in the sibling `result` field for on-demand viewing.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DisplayPayload {
    #[serde(default)]
    pub kind: DisplayKind,
    #[serde(default)]
    pub preview: String,
    #[serde(default)]
    pub truncated: bool,
    #[serde(default)]
    pub hidden_lines: usize,
}
