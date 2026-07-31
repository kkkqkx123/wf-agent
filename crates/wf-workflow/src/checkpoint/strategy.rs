use wf_checkpoint::strategy::cadenced::{
    CadencedCheckpointStrategy, CheckpointTiming as CadencedCheckpointTiming,
};
use wf_types::checkpoint::{
    CheckpointContentConfig, CheckpointRetentionConfig, CheckpointTrigger, UnifiedCheckpointPolicy,
};

/// Node-level checkpoint timing variants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckpointTiming {
    BeforeNode,
    AfterNode,
    OnNodeError,
    OnWorkflowStart,
    OnWorkflowEnd,
}

impl CadencedCheckpointTiming for CheckpointTiming {
    fn to_trigger(&self) -> CheckpointTrigger {
        match self {
            CheckpointTiming::BeforeNode => CheckpointTrigger::BeforeExecute,
            CheckpointTiming::AfterNode => CheckpointTrigger::AfterExecute,
            CheckpointTiming::OnNodeError => CheckpointTrigger::OnError,
            CheckpointTiming::OnWorkflowStart => CheckpointTrigger::Manual,
            CheckpointTiming::OnWorkflowEnd => CheckpointTrigger::OnComplete,
        }
    }
}

fn map_trigger(t: &CheckpointTrigger) -> Option<CheckpointTiming> {
    match t {
        CheckpointTrigger::BeforeExecute => Some(CheckpointTiming::BeforeNode),
        CheckpointTrigger::AfterExecute => Some(CheckpointTiming::AfterNode),
        CheckpointTrigger::OnError => Some(CheckpointTiming::OnNodeError),
        CheckpointTrigger::Manual => Some(CheckpointTiming::OnWorkflowStart),
        CheckpointTrigger::OnComplete => Some(CheckpointTiming::OnWorkflowEnd),
        _ => None,
    }
}

/// Node-level checkpoint strategy.
#[derive(Debug, Clone)]
pub struct NodeCheckpointStrategy {
    inner: CadencedCheckpointStrategy<CheckpointTiming>,
}

impl Default for NodeCheckpointStrategy {
    fn default() -> Self {
        Self::every_node()
    }
}

impl NodeCheckpointStrategy {
    pub fn never() -> Self {
        Self {
            inner: CadencedCheckpointStrategy::disabled(),
        }
    }

    pub fn always() -> Self {
        Self::from_policy(&UnifiedCheckpointPolicy {
            enabled: true,
            triggers: vec![
                CheckpointTrigger::BeforeExecute,
                CheckpointTrigger::AfterExecute,
                CheckpointTrigger::OnError,
                CheckpointTrigger::Manual,
                CheckpointTrigger::OnComplete,
            ],
            content: None,
            retention: None,
            error_handling: None,
        })
    }

    pub fn on_error() -> Self {
        Self::from_policy(&UnifiedCheckpointPolicy {
            enabled: true,
            triggers: vec![CheckpointTrigger::OnError],
            content: None,
            retention: None,
            error_handling: None,
        })
    }

    pub fn every_node() -> Self {
        Self::from_policy(&UnifiedCheckpointPolicy {
            enabled: true,
            triggers: vec![CheckpointTrigger::AfterExecute],
            content: None,
            retention: None,
            error_handling: None,
        })
    }

    pub fn every_n_nodes(n: u32) -> Self {
        Self {
            inner: CadencedCheckpointStrategy::from_policy(
                &UnifiedCheckpointPolicy {
                    enabled: true,
                    triggers: vec![CheckpointTrigger::AfterExecute],
                    content: None,
                    retention: None,
                    error_handling: None,
                },
                map_trigger,
            )
            .with_cadence(CheckpointTiming::AfterNode, n),
        }
    }

    pub fn from_policy(policy: &UnifiedCheckpointPolicy) -> Self {
        Self {
            inner: CadencedCheckpointStrategy::from_policy(policy, map_trigger),
        }
    }

    pub fn should_checkpoint(&self, timing: &CheckpointTiming, node_count: u32) -> bool {
        self.inner
            .should_checkpoint(timing, "workflow_execution", "", node_count)
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
    fn never_strategy_checkpoints_nothing() {
        let s = NodeCheckpointStrategy::never();
        assert!(!s.should_checkpoint(&CheckpointTiming::AfterNode, 1));
        assert!(!s.should_checkpoint(&CheckpointTiming::OnNodeError, 1));
    }

    #[test]
    fn every_node_checks_after_each() {
        let s = NodeCheckpointStrategy::every_node();
        assert!(s.should_checkpoint(&CheckpointTiming::AfterNode, 1));
        assert!(s.should_checkpoint(&CheckpointTiming::AfterNode, 2));
        assert!(!s.should_checkpoint(&CheckpointTiming::BeforeNode, 1));
    }

    #[test]
    fn every_n_nodes_modulo() {
        let s = NodeCheckpointStrategy::every_n_nodes(3);
        assert!(!s.should_checkpoint(&CheckpointTiming::AfterNode, 1));
        assert!(!s.should_checkpoint(&CheckpointTiming::AfterNode, 2));
        assert!(s.should_checkpoint(&CheckpointTiming::AfterNode, 3));
        assert!(!s.should_checkpoint(&CheckpointTiming::AfterNode, 4));
        assert!(s.should_checkpoint(&CheckpointTiming::AfterNode, 6));
    }

    #[test]
    fn on_error_only() {
        let s = NodeCheckpointStrategy::on_error();
        assert!(!s.should_checkpoint(&CheckpointTiming::AfterNode, 1));
        assert!(s.should_checkpoint(&CheckpointTiming::OnNodeError, 1));
    }

    #[test]
    fn to_trigger_mapping() {
        assert_eq!(
            CheckpointTiming::BeforeNode.to_trigger(),
            CheckpointTrigger::BeforeExecute
        );
        assert_eq!(
            CheckpointTiming::AfterNode.to_trigger(),
            CheckpointTrigger::AfterExecute
        );
        assert_eq!(
            CheckpointTiming::OnNodeError.to_trigger(),
            CheckpointTrigger::OnError
        );
        assert_eq!(
            CheckpointTiming::OnWorkflowStart.to_trigger(),
            CheckpointTrigger::Manual
        );
        assert_eq!(
            CheckpointTiming::OnWorkflowEnd.to_trigger(),
            CheckpointTrigger::OnComplete
        );
    }
}
