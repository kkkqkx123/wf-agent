use wf_checkpoint::strategy::cadenced::{CadencedCheckpointStrategy, CheckpointTimingVariant};
use wf_types::checkpoint::{
    CheckpointContentConfig, CheckpointRetentionConfig, CheckpointTiming, UnifiedCheckpointPolicy,
};

/// Agent-level checkpoint timing variants.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AgentCheckpointTiming {
    BeforeIteration,
    AfterIteration,
    OnIterationError,
    OnAgentStart,
    OnAgentEnd,
    OnAgentPause,
    OnAgentCancel,
    OnAgentTimeout,
    BeforeTool,
    AfterTool,
    BeforeCompression,
    AfterCompression,
    /// Message-count backstop for extra-long turns. Gated by the caller on
    /// the session message count; the strategy only decides enablement.
    MessageInterval,
}

impl CheckpointTimingVariant for AgentCheckpointTiming {
    fn to_trigger(&self) -> CheckpointTiming {
        match self {
            AgentCheckpointTiming::BeforeIteration => CheckpointTiming::BeforeExecute,
            AgentCheckpointTiming::AfterIteration => CheckpointTiming::AfterExecute,
            AgentCheckpointTiming::OnIterationError => CheckpointTiming::OnError,
            AgentCheckpointTiming::OnAgentStart => CheckpointTiming::Manual,
            AgentCheckpointTiming::OnAgentEnd => CheckpointTiming::OnComplete,
            AgentCheckpointTiming::OnAgentPause => CheckpointTiming::OnPause,
            AgentCheckpointTiming::OnAgentCancel => CheckpointTiming::OnCancel,
            AgentCheckpointTiming::OnAgentTimeout => CheckpointTiming::OnTimeout,
            AgentCheckpointTiming::BeforeTool => CheckpointTiming::ToolBefore,
            AgentCheckpointTiming::AfterTool => CheckpointTiming::ToolAfter,
            AgentCheckpointTiming::BeforeCompression => CheckpointTiming::BeforeCompression,
            AgentCheckpointTiming::AfterCompression => CheckpointTiming::AfterCompression,
            AgentCheckpointTiming::MessageInterval => CheckpointTiming::Interval,
        }
    }
}

fn map_trigger(t: &CheckpointTiming) -> Option<AgentCheckpointTiming> {
    match t {
        CheckpointTiming::BeforeExecute => Some(AgentCheckpointTiming::BeforeIteration),
        CheckpointTiming::AfterExecute => Some(AgentCheckpointTiming::AfterIteration),
        CheckpointTiming::OnError => Some(AgentCheckpointTiming::OnIterationError),
        CheckpointTiming::Manual => Some(AgentCheckpointTiming::OnAgentStart),
        CheckpointTiming::OnComplete => Some(AgentCheckpointTiming::OnAgentEnd),
        CheckpointTiming::OnPause => Some(AgentCheckpointTiming::OnAgentPause),
        CheckpointTiming::OnCancel => Some(AgentCheckpointTiming::OnAgentCancel),
        CheckpointTiming::OnTimeout => Some(AgentCheckpointTiming::OnAgentTimeout),
        CheckpointTiming::ToolBefore => Some(AgentCheckpointTiming::BeforeTool),
        CheckpointTiming::ToolAfter => Some(AgentCheckpointTiming::AfterTool),
        CheckpointTiming::BeforeCompression => Some(AgentCheckpointTiming::BeforeCompression),
        CheckpointTiming::AfterCompression => Some(AgentCheckpointTiming::AfterCompression),
        CheckpointTiming::Interval => Some(AgentCheckpointTiming::MessageInterval),
        _ => None,
    }
}

/// Agent-level checkpoint strategy.
#[derive(Debug, Clone)]
pub struct AgentCheckpointStrategy {
    inner: CadencedCheckpointStrategy<AgentCheckpointTiming>,
}

impl Default for AgentCheckpointStrategy {
    fn default() -> Self {
        Self::every_iteration()
    }
}

impl AgentCheckpointStrategy {
    pub fn never() -> Self {
        Self {
            inner: CadencedCheckpointStrategy::disabled(),
        }
    }

    pub fn every_iteration() -> Self {
        Self::from_policy(&UnifiedCheckpointPolicy {
            enabled: true,
            triggers: vec![CheckpointTiming::AfterExecute],
            content: None,
            retention: None,
            error_handling: None,
        })
    }

    pub fn every_n_iterations(n: u32) -> Self {
        Self {
            inner: CadencedCheckpointStrategy::from_policy(
                &UnifiedCheckpointPolicy {
                    enabled: true,
                    triggers: vec![CheckpointTiming::AfterExecute],
                    content: None,
                    retention: None,
                    error_handling: None,
                },
                map_trigger,
            )
            .with_cadence(AgentCheckpointTiming::AfterIteration, n),
        }
    }

    pub fn on_error() -> Self {
        Self::from_policy(&UnifiedCheckpointPolicy {
            enabled: true,
            triggers: vec![CheckpointTiming::OnError],
            content: None,
            retention: None,
            error_handling: None,
        })
    }

    /// Assemble the strategy from the agent checkpoint configuration in one
    /// place: iteration cadence, error snapshots, tool-call boundaries,
    /// compression boundaries and the optional message-count backstop.
    /// Tool and compression boundaries default to on; the message backstop
    /// stays off unless `message_interval` is set. Lifecycle boundaries
    /// (start, complete, pause, terminal) are always on so routing them
    /// through the gate preserves the previous direct-write behavior while
    /// centralizing the decision.
    pub fn from_agent_config(
        interval_iterations: u32,
        on_error: bool,
        on_tool_call: bool,
        on_compression: bool,
        message_interval: Option<u32>,
    ) -> Self {
        let mut triggers = vec![
            CheckpointTiming::AfterExecute,
            CheckpointTiming::Manual,
            CheckpointTiming::OnComplete,
            CheckpointTiming::OnPause,
            CheckpointTiming::OnCancel,
            CheckpointTiming::OnTimeout,
            CheckpointTiming::OnStopped,
            CheckpointTiming::OnFailure,
        ];
        if on_error {
            triggers.push(CheckpointTiming::OnError);
        }
        if on_tool_call {
            triggers.push(CheckpointTiming::ToolBefore);
            triggers.push(CheckpointTiming::ToolAfter);
        }
        if on_compression {
            triggers.push(CheckpointTiming::BeforeCompression);
            triggers.push(CheckpointTiming::AfterCompression);
        }
        if message_interval.unwrap_or(0) > 0 {
            triggers.push(CheckpointTiming::Interval);
        }
        Self {
            inner: CadencedCheckpointStrategy::from_policy(
                &UnifiedCheckpointPolicy {
                    enabled: true,
                    triggers,
                    content: None,
                    retention: None,
                    error_handling: None,
                },
                map_trigger,
            )
            .with_cadence(
                AgentCheckpointTiming::AfterIteration,
                interval_iterations.max(1),
            ),
        }
    }

    pub fn from_policy(policy: &UnifiedCheckpointPolicy) -> Self {
        Self {
            inner: CadencedCheckpointStrategy::from_policy(policy, map_trigger),
        }
    }

    pub fn should_checkpoint(&self, timing: &AgentCheckpointTiming, iteration_count: u32) -> bool {
        self.inner
            .should_checkpoint(timing, "agent_loop", "", iteration_count)
    }

    pub fn content_config(&self) -> &CheckpointContentConfig {
        self.inner.content_config()
    }

    pub fn retention_config(&self) -> Option<&CheckpointRetentionConfig> {
        self.inner.retention_config()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_strategy() {
        let s = AgentCheckpointStrategy::never();
        assert!(!s.should_checkpoint(&AgentCheckpointTiming::AfterIteration, 1));
    }

    #[test]
    fn every_iteration() {
        let s = AgentCheckpointStrategy::every_iteration();
        assert!(s.should_checkpoint(&AgentCheckpointTiming::AfterIteration, 1));
        assert!(s.should_checkpoint(&AgentCheckpointTiming::AfterIteration, 5));
    }

    #[test]
    fn every_n_iterations() {
        let s = AgentCheckpointStrategy::every_n_iterations(3);
        assert!(!s.should_checkpoint(&AgentCheckpointTiming::AfterIteration, 1));
        assert!(!s.should_checkpoint(&AgentCheckpointTiming::AfterIteration, 2));
        assert!(s.should_checkpoint(&AgentCheckpointTiming::AfterIteration, 3));
        assert!(s.should_checkpoint(&AgentCheckpointTiming::AfterIteration, 6));
    }

    #[test]
    fn on_error_only() {
        let s = AgentCheckpointStrategy::on_error();
        assert!(!s.should_checkpoint(&AgentCheckpointTiming::AfterIteration, 1));
        assert!(s.should_checkpoint(&AgentCheckpointTiming::OnIterationError, 1));
    }

    #[test]
    fn from_agent_config_defaults() {
        let s = AgentCheckpointStrategy::from_agent_config(2, true, true, true, None);
        // Iteration cadence applies.
        assert!(!s.should_checkpoint(&AgentCheckpointTiming::AfterIteration, 1));
        assert!(s.should_checkpoint(&AgentCheckpointTiming::AfterIteration, 2));
        // Error, tool and compression boundaries default to on.
        assert!(s.should_checkpoint(&AgentCheckpointTiming::OnIterationError, 1));
        assert!(s.should_checkpoint(&AgentCheckpointTiming::BeforeTool, 1));
        assert!(s.should_checkpoint(&AgentCheckpointTiming::AfterTool, 1));
        assert!(s.should_checkpoint(&AgentCheckpointTiming::BeforeCompression, 1));
        assert!(s.should_checkpoint(&AgentCheckpointTiming::AfterCompression, 1));
        // Lifecycle boundaries stay on so gating preserves direct-write
        // behavior while centralizing the decision.
        assert!(s.should_checkpoint(&AgentCheckpointTiming::OnAgentStart, 1));
        assert!(s.should_checkpoint(&AgentCheckpointTiming::OnAgentEnd, 1));
        // Message backstop stays off unless configured.
        assert!(!s.should_checkpoint(&AgentCheckpointTiming::MessageInterval, 10));
    }

    #[test]
    fn from_agent_config_toggles() {
        let s = AgentCheckpointStrategy::from_agent_config(1, false, false, false, Some(5));
        assert!(s.should_checkpoint(&AgentCheckpointTiming::AfterIteration, 1));
        assert!(!s.should_checkpoint(&AgentCheckpointTiming::OnIterationError, 1));
        assert!(!s.should_checkpoint(&AgentCheckpointTiming::BeforeTool, 1));
        assert!(!s.should_checkpoint(&AgentCheckpointTiming::AfterTool, 1));
        assert!(!s.should_checkpoint(&AgentCheckpointTiming::BeforeCompression, 1));
        assert!(!s.should_checkpoint(&AgentCheckpointTiming::AfterCompression, 1));
        assert!(s.should_checkpoint(&AgentCheckpointTiming::MessageInterval, 10));
    }

    #[test]
    fn to_trigger_mapping() {
        assert_eq!(
            AgentCheckpointTiming::BeforeIteration.to_trigger(),
            CheckpointTiming::BeforeExecute
        );
        assert_eq!(
            AgentCheckpointTiming::AfterIteration.to_trigger(),
            CheckpointTiming::AfterExecute
        );
        assert_eq!(
            AgentCheckpointTiming::OnIterationError.to_trigger(),
            CheckpointTiming::OnError
        );
    }
}
