use serde::{Deserialize, Serialize};

/// Execution mode of one agent loop run, carried on the run config.
///
/// `None` (including all persisted configs written before this field
/// existed) means a full agent run. A temporary mode never changes the
/// executor: the row stays `execution_type = agent`, the checkpoint and
/// history machinery stays the same shape, only the exposure is forced
/// and, for the ephemeral tier, checkpointing is switched off.
///
/// Persistence note: both tiers keep messages, variables and execution
/// records queryable so the transcript stays replayable. The ephemeral
/// tier only keeps the checkpoint chain empty; a crash there has no
/// snapshot to resume from, so callers re-ask instead of resuming.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TemporaryMode {
    /// Restricted first turns of a draft: the server forces the tool
    /// exposure onto the temporary allowlist (read-only inspection,
    /// interaction and network lookup), no matter what lists the caller
    /// sent. The run persists like any agent run and is promoted by
    /// rerunning the same execution id without the flag.
    Restricted,
    /// Short read-only Q&A: the server forces the exposure onto the
    /// ephemeral allowlist (read-only inspection and interaction only,
    /// no network, no side effects) and takes no checkpoints at all —
    /// no interval/strategy checkpoints, no lifecycle checkpoints, no
    /// hook checkpoints, no file history session, and manual creation is
    /// rejected. Messages and execution records still persist, so the
    /// transcript stays replayable; only the checkpoint chain stays empty.
    /// Promotion never reuses the same execution id: start a new execution
    /// with explicit history instead of continuing the ephemeral id.
    Ephemeral,
}

impl TemporaryMode {
    /// Stable wire name of the mode.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Restricted => "restricted",
            Self::Ephemeral => "ephemeral",
        }
    }

    /// Parse a wire name back into its mode. Strict: unknown names are
    /// rejected rather than aliased onto a full run.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "restricted" => Some(Self::Restricted),
            "ephemeral" => Some(Self::Ephemeral),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_roundtrip_through_the_wire() {
        for mode in [TemporaryMode::Restricted, TemporaryMode::Ephemeral] {
            assert_eq!(TemporaryMode::parse(mode.as_str()), Some(mode));
            let json = serde_json::to_string(&mode).expect("serializes");
            assert_eq!(json, format!("\"{}\"", mode.as_str()));
            let back: TemporaryMode = serde_json::from_str(&json).expect("deserializes");
            assert_eq!(back, mode);
        }
    }

    #[test]
    fn unknown_modes_are_rejected() {
        assert_eq!(TemporaryMode::parse("full"), None);
        assert_eq!(TemporaryMode::parse(""), None);
        assert_eq!(TemporaryMode::parse("RESTRICTED "), None);
    }
}
