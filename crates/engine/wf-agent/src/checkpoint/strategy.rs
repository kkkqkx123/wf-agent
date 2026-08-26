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
}

impl CheckpointTimingVariant for AgentCheckpointTiming {
    fn to_trigger(&self) -> CheckpointTiming {
        match self {
            AgentCheckpointTiming::BeforeIteration => CheckpointTiming::BeforeExecute,
            AgentCheckpointTiming::AfterIteration => CheckpointTiming::AfterExecute,
            AgentCheckpointTiming::OnIterationError => CheckpointTiming::OnError,
            AgentCheckpointTiming::OnAgentStart => CheckpointTiming::Manual,
            AgentCheckpointTiming::OnAgentEnd => CheckpointTiming::OnComplete,
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
