use serde::{Deserialize, Serialize};

/// Which executor produced a session.
///
/// The executors differ in what they can do, not in what they are called:
/// the agent loop runs tools across iterations, the chat executor answers
/// multi-turn questions without tools, and the workflow executor runs a
/// graph. A session record names the executor so a replay or an audit can
/// tell what actually ran.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionExecutionType {
    /// Full agent loop: tools, iterations, checkpoints and approvals.
    #[default]
    Agent,
    /// Tool-free multi-turn question answering.
    Chat,
    /// Graph execution driven by a workflow template.
    Workflow,
}

impl SessionExecutionType {
    /// Stable wire name of the executor.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Chat => "chat",
            Self::Workflow => "workflow",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_is_the_default_executor() {
        assert_eq!(SessionExecutionType::default(), SessionExecutionType::Agent);
    }

    #[test]
    fn wire_names_match_the_serialized_form() {
        for kind in [
            SessionExecutionType::Agent,
            SessionExecutionType::Chat,
            SessionExecutionType::Workflow,
        ] {
            let json = serde_json::to_string(&kind).expect("serializes");
            assert_eq!(json, format!("\"{}\"", kind.as_str()));
            let back: SessionExecutionType = serde_json::from_str(&json).expect("deserializes");
            assert_eq!(back, kind);
        }
    }
}
