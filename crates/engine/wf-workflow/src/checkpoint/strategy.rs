use wf_checkpoint::strategy::cadenced::{CadencedCheckpointStrategy, CheckpointTimingVariant};
use wf_types::checkpoint::{
    CheckpointContentConfig, CheckpointRetentionConfig, CheckpointTiming, NodeCheckpointConfig,
    NodeCheckpointTiming, UnifiedCheckpointPolicy,
};

/// Node-level checkpoint timing variants.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WorkflowCheckpointTiming {
    BeforeNode,
    AfterNode,
    OnNodeError,
    OnWorkflowStart,
    OnWorkflowEnd,
}

impl CheckpointTimingVariant for WorkflowCheckpointTiming {
    fn to_trigger(&self) -> CheckpointTiming {
        match self {
            WorkflowCheckpointTiming::BeforeNode => CheckpointTiming::BeforeExecute,
            WorkflowCheckpointTiming::AfterNode => CheckpointTiming::AfterExecute,
            WorkflowCheckpointTiming::OnNodeError => CheckpointTiming::OnError,
            WorkflowCheckpointTiming::OnWorkflowStart => CheckpointTiming::Manual,
            WorkflowCheckpointTiming::OnWorkflowEnd => CheckpointTiming::OnComplete,
        }
    }
}

fn map_trigger(t: &CheckpointTiming) -> Option<WorkflowCheckpointTiming> {
    match t {
        CheckpointTiming::BeforeExecute => Some(WorkflowCheckpointTiming::BeforeNode),
        CheckpointTiming::AfterExecute => Some(WorkflowCheckpointTiming::AfterNode),
        CheckpointTiming::OnError => Some(WorkflowCheckpointTiming::OnNodeError),
        CheckpointTiming::Manual => Some(WorkflowCheckpointTiming::OnWorkflowStart),
        CheckpointTiming::OnComplete => Some(WorkflowCheckpointTiming::OnWorkflowEnd),
        _ => None,
    }
}

/// Node-level checkpoint strategy.
#[derive(Debug, Clone)]
pub struct NodeCheckpointStrategy {
    inner: CadencedCheckpointStrategy<WorkflowCheckpointTiming>,
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
                CheckpointTiming::BeforeExecute,
                CheckpointTiming::AfterExecute,
                CheckpointTiming::OnError,
                CheckpointTiming::Manual,
                CheckpointTiming::OnComplete,
            ],
            content: None,
            retention: None,
            error_handling: None,
        })
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

    pub fn every_node() -> Self {
        Self::from_policy(&UnifiedCheckpointPolicy {
            enabled: true,
            triggers: vec![CheckpointTiming::AfterExecute],
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
                    triggers: vec![
                        CheckpointTiming::BeforeExecute,
                        CheckpointTiming::AfterExecute,
                    ],
                    content: None,
                    retention: None,
                    error_handling: None,
                },
                map_trigger,
            )
            .with_cadence(WorkflowCheckpointTiming::BeforeNode, n)
            .with_cadence(WorkflowCheckpointTiming::AfterNode, n),
        }
    }

    pub fn from_policy(policy: &UnifiedCheckpointPolicy) -> Self {
        Self {
            inner: CadencedCheckpointStrategy::from_policy(policy, map_trigger),
        }
    }

    pub fn should_checkpoint(&self, timing: &WorkflowCheckpointTiming, node_count: u32) -> bool {
        self.inner
            .should_checkpoint(timing, "workflow_execution", "", node_count)
    }

    /// Whether the timing belongs to the enabled set, ignoring cadence.
    fn timing_enabled(&self, timing: &WorkflowCheckpointTiming) -> bool {
        self.inner.timing_enabled(timing)
    }

    /// The configured cadence for the timing (defaults to 1 when unset).
    fn cadence(&self, timing: &WorkflowCheckpointTiming) -> u32 {
        self.inner.cadence(timing)
    }

    /// Resolve the effective strategy for a node: node-level checkpoint
    /// config wins where explicit, unspecified aspects fall back to this
    /// workflow-level strategy
    /// layering. Workflow-scope timings (start/end) are never affected by
    /// node config. `None` node config returns the workflow strategy as-is.
    pub fn resolve(&self, node_config: Option<&NodeCheckpointConfig>) -> Self {
        let Some(cfg) = node_config else {
            return self.clone();
        };

        let mut triggers: Vec<CheckpointTiming> = Vec::new();
        let mut cadences: Vec<(WorkflowCheckpointTiming, u32)> = Vec::new();

        // Workflow-scope timings are outside the node's control.
        if self.timing_enabled(&WorkflowCheckpointTiming::OnWorkflowStart) {
            triggers.push(CheckpointTiming::Manual);
        }
        if self.timing_enabled(&WorkflowCheckpointTiming::OnWorkflowEnd) {
            triggers.push(CheckpointTiming::OnComplete);
        }

        let node_timings = cfg.triggers.as_ref();
        if cfg.enabled != Some(false) {
            for timing in [
                WorkflowCheckpointTiming::BeforeNode,
                WorkflowCheckpointTiming::AfterNode,
                WorkflowCheckpointTiming::OnNodeError,
            ] {
                let on = match node_timings {
                    Some(list) => list.iter().any(|t| matches_timing(t, &timing)),
                    None => self.timing_enabled(&timing),
                };
                if !on {
                    continue;
                }
                // Cadence only applies to Before/After; error checkpoints
                // always fire when enabled.
                if timing != WorkflowCheckpointTiming::OnNodeError {
                    let cadence = match node_timings {
                        // Explicit node timing set: node cadence (default 1).
                        Some(_) => cfg.every_n_nodes.unwrap_or(1),
                        // Fallback to workflow policy per timing; an explicit
                        // node cadence still overrides the workflow cadence.
                        None => cfg.every_n_nodes.unwrap_or_else(|| self.cadence(&timing)),
                    };
                    cadences.push((timing.clone(), cadence));
                }
                triggers.push(timing.to_trigger());
            }
        }

        let mut resolved = Self::from_policy(&UnifiedCheckpointPolicy {
            enabled: true,
            triggers,
            content: None,
            retention: None,
            error_handling: None,
        });
        for (timing, n) in cadences {
            resolved = resolved.with_cadence(timing, n);
        }
        resolved
    }

    /// Chain setters for the resolved strategy (cadence / timing enablement).
    fn with_cadence(mut self, timing: WorkflowCheckpointTiming, n: u32) -> Self {
        self.inner = self.inner.with_cadence(timing, n);
        self
    }

    pub fn content_config(&self) -> &CheckpointContentConfig {
        self.inner.content_config()
    }

    pub fn retention_config(&self) -> Option<&CheckpointRetentionConfig> {
        self.inner.retention_config()
    }
}

/// Map a `NodeCheckpointTiming` onto the runtime checkpoint timing.
fn matches_timing(t: &NodeCheckpointTiming, timing: &WorkflowCheckpointTiming) -> bool {
    matches!(
        (t, timing),
        (
            NodeCheckpointTiming::Before,
            WorkflowCheckpointTiming::BeforeNode
        ) | (
            NodeCheckpointTiming::After,
            WorkflowCheckpointTiming::AfterNode
        ) | (
            NodeCheckpointTiming::OnError,
            WorkflowCheckpointTiming::OnNodeError
        )
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_strategy_checkpoints_nothing() {
        let s = NodeCheckpointStrategy::never();
        assert!(!s.should_checkpoint(&WorkflowCheckpointTiming::AfterNode, 1));
        assert!(!s.should_checkpoint(&WorkflowCheckpointTiming::OnNodeError, 1));
    }

    #[test]
    fn every_node_checks_after_each() {
        let s = NodeCheckpointStrategy::every_node();
        assert!(s.should_checkpoint(&WorkflowCheckpointTiming::AfterNode, 1));
        assert!(s.should_checkpoint(&WorkflowCheckpointTiming::AfterNode, 2));
        assert!(!s.should_checkpoint(&WorkflowCheckpointTiming::BeforeNode, 1));
    }

    #[test]
    fn every_n_nodes_modulo() {
        let s = NodeCheckpointStrategy::every_n_nodes(3);
        assert!(!s.should_checkpoint(&WorkflowCheckpointTiming::AfterNode, 1));
        assert!(!s.should_checkpoint(&WorkflowCheckpointTiming::AfterNode, 2));
        assert!(s.should_checkpoint(&WorkflowCheckpointTiming::AfterNode, 3));
        assert!(!s.should_checkpoint(&WorkflowCheckpointTiming::AfterNode, 4));
        assert!(s.should_checkpoint(&WorkflowCheckpointTiming::AfterNode, 6));
    }

    #[test]
    fn every_n_nodes_before_node_cadence() {
        let s = NodeCheckpointStrategy::every_n_nodes(3);
        // BeforeNode follows the same cadence as AfterNode.
        assert!(!s.should_checkpoint(&WorkflowCheckpointTiming::BeforeNode, 1));
        assert!(!s.should_checkpoint(&WorkflowCheckpointTiming::BeforeNode, 2));
        assert!(s.should_checkpoint(&WorkflowCheckpointTiming::BeforeNode, 3));
        assert!(!s.should_checkpoint(&WorkflowCheckpointTiming::BeforeNode, 4));
    }

    #[test]
    fn always_strategy_includes_before_node() {
        let s = NodeCheckpointStrategy::always();
        assert!(s.should_checkpoint(&WorkflowCheckpointTiming::BeforeNode, 0));
        assert!(s.should_checkpoint(&WorkflowCheckpointTiming::BeforeNode, 3));
        assert!(s.should_checkpoint(&WorkflowCheckpointTiming::AfterNode, 1));
    }

    #[test]
    fn on_error_only() {
        let s = NodeCheckpointStrategy::on_error();
        assert!(!s.should_checkpoint(&WorkflowCheckpointTiming::AfterNode, 1));
        assert!(s.should_checkpoint(&WorkflowCheckpointTiming::OnNodeError, 1));
    }

    #[test]
    fn to_trigger_mapping() {
        assert_eq!(
            WorkflowCheckpointTiming::BeforeNode.to_trigger(),
            CheckpointTiming::BeforeExecute
        );
        assert_eq!(
            WorkflowCheckpointTiming::AfterNode.to_trigger(),
            CheckpointTiming::AfterExecute
        );
        assert_eq!(
            WorkflowCheckpointTiming::OnNodeError.to_trigger(),
            CheckpointTiming::OnError
        );
        assert_eq!(
            WorkflowCheckpointTiming::OnWorkflowStart.to_trigger(),
            CheckpointTiming::Manual
        );
        assert_eq!(
            WorkflowCheckpointTiming::OnWorkflowEnd.to_trigger(),
            CheckpointTiming::OnComplete
        );
    }

    fn config(
        enabled: Option<bool>,
        triggers: Option<Vec<NodeCheckpointTiming>>,
        every_n_nodes: Option<u32>,
    ) -> NodeCheckpointConfig {
        NodeCheckpointConfig {
            enabled,
            triggers,
            description: None,
            every_n_nodes: every_n_nodes.map(|n| n.max(1)),
        }
    }

    #[test]
    fn resolve_without_node_config_keeps_workflow_strategy() {
        let workflow = NodeCheckpointStrategy::every_node();
        let resolved = workflow.resolve(None);
        assert!(resolved.should_checkpoint(&WorkflowCheckpointTiming::AfterNode, 1));
        assert!(resolved.should_checkpoint(&WorkflowCheckpointTiming::AfterNode, 2));
        assert!(!resolved.should_checkpoint(&WorkflowCheckpointTiming::BeforeNode, 1));
        assert!(!resolved.should_checkpoint(&WorkflowCheckpointTiming::OnNodeError, 1));
    }

    #[test]
    fn node_disabled_disables_node_checkpoints_only() {
        // `always` includes workflow-scope start/end plus all node timings.
        let workflow = NodeCheckpointStrategy::always();
        let resolved = workflow.resolve(Some(&config(Some(false), None, None)));

        // Node-level timings are all off.
        assert!(!resolved.should_checkpoint(&WorkflowCheckpointTiming::BeforeNode, 0));
        assert!(!resolved.should_checkpoint(&WorkflowCheckpointTiming::AfterNode, 1));
        assert!(!resolved.should_checkpoint(&WorkflowCheckpointTiming::OnNodeError, 1));
        // Workflow-scope timings are untouched.
        assert!(resolved.should_checkpoint(&WorkflowCheckpointTiming::OnWorkflowStart, 0));
        assert!(resolved.should_checkpoint(&WorkflowCheckpointTiming::OnWorkflowEnd, 0));
    }

    #[test]
    fn node_triggers_override_workflow_policy() {
        // Workflow snapshots after every node; the node asks for Before only.
        let workflow = NodeCheckpointStrategy::every_node();
        let resolved = workflow.resolve(Some(&config(
            None,
            Some(vec![NodeCheckpointTiming::Before]),
            None,
        )));

        assert!(resolved.should_checkpoint(&WorkflowCheckpointTiming::BeforeNode, 1));
        assert!(!resolved.should_checkpoint(&WorkflowCheckpointTiming::AfterNode, 1));
        assert!(!resolved.should_checkpoint(&WorkflowCheckpointTiming::OnNodeError, 1));
    }

    #[test]
    fn node_on_error_timing() {
        let workflow = NodeCheckpointStrategy::always();
        let resolved = workflow.resolve(Some(&config(
            None,
            Some(vec![NodeCheckpointTiming::OnError]),
            None,
        )));

        assert!(resolved.should_checkpoint(&WorkflowCheckpointTiming::OnNodeError, 1));
        assert!(!resolved.should_checkpoint(&WorkflowCheckpointTiming::BeforeNode, 0));
        assert!(!resolved.should_checkpoint(&WorkflowCheckpointTiming::AfterNode, 1));
        assert!(resolved.should_checkpoint(&WorkflowCheckpointTiming::OnWorkflowStart, 0));
    }

    #[test]
    fn node_every_n_nodes_throttles_before_and_after() {
        let workflow = NodeCheckpointStrategy::always();
        let resolved = workflow.resolve(Some(&config(
            None,
            Some(vec![
                NodeCheckpointTiming::Before,
                NodeCheckpointTiming::After,
            ]),
            Some(3),
        )));

        assert!(!resolved.should_checkpoint(&WorkflowCheckpointTiming::BeforeNode, 1));
        assert!(!resolved.should_checkpoint(&WorkflowCheckpointTiming::AfterNode, 1));
        assert!(resolved.should_checkpoint(&WorkflowCheckpointTiming::BeforeNode, 3));
        assert!(resolved.should_checkpoint(&WorkflowCheckpointTiming::AfterNode, 3));
        assert!(!resolved.should_checkpoint(&WorkflowCheckpointTiming::AfterNode, 4));
        assert!(resolved.should_checkpoint(&WorkflowCheckpointTiming::AfterNode, 6));
        // OnError is not part of the explicit trigger set.
        assert!(!resolved.should_checkpoint(&WorkflowCheckpointTiming::OnNodeError, 1));
    }

    #[test]
    fn node_on_error_never_throttled_by_every_n_nodes() {
        let workflow = NodeCheckpointStrategy::always();
        let resolved = workflow.resolve(Some(&config(
            None,
            Some(vec![
                NodeCheckpointTiming::Before,
                NodeCheckpointTiming::After,
                NodeCheckpointTiming::OnError,
            ]),
            Some(3),
        )));

        // Cadence applies to Before/After; OnError fires on every failure.
        assert!(!resolved.should_checkpoint(&WorkflowCheckpointTiming::BeforeNode, 1));
        assert!(resolved.should_checkpoint(&WorkflowCheckpointTiming::OnNodeError, 1));
        assert!(resolved.should_checkpoint(&WorkflowCheckpointTiming::OnNodeError, 5));
    }

    #[test]
    fn resolve_fallback_preserves_workflow_cadence() {
        let workflow = NodeCheckpointStrategy::every_n_nodes(2);
        // Node config without triggers and without cadence: the workflow
        // cadence (every 2) must survive for node-level timings.
        let resolved = workflow.resolve(Some(&config(None, None, None)));

        assert!(!resolved.should_checkpoint(&WorkflowCheckpointTiming::AfterNode, 1));
        assert!(resolved.should_checkpoint(&WorkflowCheckpointTiming::AfterNode, 2));
        assert!(!resolved.should_checkpoint(&WorkflowCheckpointTiming::AfterNode, 3));
        assert!(resolved.should_checkpoint(&WorkflowCheckpointTiming::AfterNode, 4));
    }

    #[test]
    fn resolve_node_cadence_overrides_workflow_cadence() {
        let workflow = NodeCheckpointStrategy::every_n_nodes(3);
        // Fallback timing set, but the node cadence replaces the workflow one.
        let resolved = workflow.resolve(Some(&config(None, None, Some(2))));

        assert!(!resolved.should_checkpoint(&WorkflowCheckpointTiming::AfterNode, 1));
        assert!(resolved.should_checkpoint(&WorkflowCheckpointTiming::AfterNode, 2));
        assert!(!resolved.should_checkpoint(&WorkflowCheckpointTiming::AfterNode, 3));
        assert!(resolved.should_checkpoint(&WorkflowCheckpointTiming::AfterNode, 4));
        assert!(resolved.should_checkpoint(&WorkflowCheckpointTiming::BeforeNode, 2));
    }
}
