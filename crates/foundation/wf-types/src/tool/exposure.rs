//! Tool exposure semantics.
//!
//! Declares how a tool is surfaced to the model during per-turn assembly.
//! wf-agent has no Code Mode, so the full six-state Codex model is reduced to
//! four states; `CodeModeOnly` and `DeferredModelOnly` are intentionally
//! absent.

use serde::{Deserialize, Serialize};

/// How a registered tool is exposed to the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ToolExposure {
    /// In the initial visible set (default).
    #[default]
    Direct,
    /// In the visible set but not callable by flows / sub-agents
    /// automatically (semantic reservation).
    DirectModelOnly,
    /// Not in the initial schema; only metadata is injected into the prompt
    /// and the tool is invoked through the `general` tool. Formally injected
    /// into the schema only when activated via TOOL_VISIBILITY unblock.
    Discoverable,
    /// Registered but never exposed to the model.
    Hidden,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_direct() {
        assert_eq!(ToolExposure::default(), ToolExposure::Direct);
    }

    #[test]
    fn test_serde_roundtrip_snake_case() {
        for (value, json) in [
            (ToolExposure::Direct, r#""direct""#),
            (ToolExposure::DirectModelOnly, r#""direct_model_only""#),
            (ToolExposure::Discoverable, r#""discoverable""#),
            (ToolExposure::Hidden, r#""hidden""#),
        ] {
            let serialized = serde_json::to_string(&value).unwrap();
            assert_eq!(serialized, json);
            let parsed: ToolExposure = serde_json::from_str(json).unwrap();
            assert_eq!(parsed, value);
        }
    }
}
