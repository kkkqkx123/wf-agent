use crate::metadata_builder::trigger_description;
use wf_types::checkpoint::base::{
    CheckpointRetentionConfig, CheckpointTrigger, CompressionStrategy, UnifiedCheckpointPolicy,
};

/// Configuration layer source, aligned with the TS `CheckpointConfigSource`
/// union (`"runtime" | "workflow" | "node" | "agent" | "global" | "default"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckpointConfigSource {
    Runtime,
    Workflow,
    Node,
    Agent,
    Global,
    Default,
}

impl CheckpointConfigSource {
    /// Lower value = higher priority in layered resolution.
    pub fn priority(&self) -> u8 {
        match self {
            Self::Runtime => 0,
            Self::Workflow => 1,
            Self::Node => 2,
            Self::Agent => 3,
            Self::Global => 4,
            Self::Default => 5,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Runtime => "runtime",
            Self::Workflow => "workflow",
            Self::Node => "node",
            Self::Agent => "agent",
            Self::Global => "global",
            Self::Default => "default",
        }
    }
}

/// A single checkpoint configuration layer, aligned with the TS
/// `CheckpointConfigLayer` (source + config).
#[derive(Debug, Clone)]
pub struct CheckpointConfigLayer {
    pub source: CheckpointConfigSource,
    pub policy: UnifiedCheckpointPolicy,
}

impl CheckpointConfigLayer {
    pub fn new(source: CheckpointConfigSource, policy: UnifiedCheckpointPolicy) -> Self {
        Self { source, policy }
    }
}

/// The resolved checkpoint configuration with the effective source tracked,
/// aligned with the TS `CheckpointConfigResult`.
#[derive(Debug, Clone)]
pub struct ResolvedCheckpointConfig {
    pub policy: UnifiedCheckpointPolicy,
    pub effective_source: CheckpointConfigSource,
    pub should_create: bool,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct CheckpointConfigResolver;

impl CheckpointConfigResolver {
    /// Resolve a list of layers with first-wins semantics: the highest
    /// priority layer (runtime > workflow > node > agent > global > default)
    /// that defines a field wins for that field. `enabled` must be explicit
    /// to take effect; otherwise the default (`false`) is used, mirroring
    /// the TS `resolve` behavior (first layer with explicit `enabled`, else
    /// default).
    pub fn resolve(layers: &[CheckpointConfigLayer]) -> ResolvedCheckpointConfig {
        let mut ordered: Vec<&CheckpointConfigLayer> = layers.iter().collect();
        ordered.sort_by_key(|l| l.source.priority());

        let mut policy = UnifiedCheckpointPolicy {
            enabled: false,
            triggers: Vec::new(),
            content: None,
            retention: None,
            error_handling: None,
        };
        let mut effective_source = CheckpointConfigSource::Default;

        for layer in ordered {
            if layer.policy.enabled {
                policy.enabled = true;
                if effective_source == CheckpointConfigSource::Default {
                    effective_source = layer.source;
                }
            }
            if !layer.policy.triggers.is_empty() && policy.triggers.is_empty() {
                policy.triggers = layer.policy.triggers.clone();
            }
            if policy.content.is_none() {
                policy.content = layer.policy.content.clone();
            }
            if policy.retention.is_none() {
                policy.retention = layer.policy.retention.clone();
            }
            if policy.error_handling.is_none() {
                policy.error_handling = layer.policy.error_handling.clone();
            }
        }

        ResolvedCheckpointConfig {
            should_create: policy.enabled,
            description: Self::build_description(&policy, None, None),
            policy,
            effective_source,
        }
    }

    /// Fill default triggers/retention when a user config omits them
    /// (retained from the legacy `resolve_from_user_config`).
    pub fn resolve_from_user_config(
        user_policy: &UnifiedCheckpointPolicy,
    ) -> UnifiedCheckpointPolicy {
        let mut policy = user_policy.clone();

        if policy.triggers.is_empty() {
            policy.triggers = vec![CheckpointTrigger::AfterExecute, CheckpointTrigger::OnError];
        }

        if policy.retention.is_none() {
            policy.retention = Some(CheckpointRetentionConfig {
                max_checkpoints: Some(10),
                max_age: None,
                compression: Some(CompressionStrategy::Auto),
            });
        }

        policy
    }

    /// Whether a checkpoint should be created for the trigger under the
    /// resolved policy. `Never` short-circuits to false; disabled policies
    /// never checkpoint (aligned with TS `shouldCreateCheckpoint`).
    pub fn should_create_checkpoint(
        &self,
        resolved: &ResolvedCheckpointConfig,
        trigger: &CheckpointTrigger,
    ) -> bool {
        if !resolved.policy.enabled {
            return false;
        }
        if resolved
            .policy
            .triggers
            .contains(&CheckpointTrigger::Never)
        {
            return false;
        }
        resolved.policy.triggers.is_empty() || resolved.policy.triggers.contains(trigger)
    }

    /// Agent-loop cadence semantics: with `interval > 1` a checkpoint fires
    /// only every `interval` iterations; with `on_error_only` only when the
    /// current iteration errored (aligned with TS `AgentLoopCheckpointConfigResolver`).
    pub fn evaluate_agent_trigger(
        &self,
        resolved: &ResolvedCheckpointConfig,
        trigger: &CheckpointTrigger,
        current_iteration: u32,
        has_error: bool,
        interval: Option<u32>,
        on_error_only: Option<bool>,
    ) -> bool {
        if !self.should_create_checkpoint(resolved, trigger) {
            return false;
        }
        if on_error_only.unwrap_or(false) && !has_error {
            return false;
        }
        if let Some(interval) = interval {
            if interval > 1 && !current_iteration.is_multiple_of(interval) {
                return false;
            }
        }
        true
    }

    /// Build the checkpoint description: agent cadence descriptions use
    /// "Iteration {n} checkpoint" / "Error checkpoint", other triggers use
    /// the trigger-based description (aligned with TS `buildDescription`).
    pub fn build_description(
        policy: &UnifiedCheckpointPolicy,
        trigger: Option<&CheckpointTrigger>,
        iteration: Option<u32>,
    ) -> String {
        if let Some(description) = policy
            .content
            .as_ref()
            .and_then(|c| c.metadata.as_ref())
            .and_then(|m| m.description.clone())
        {
            return description;
        }
        match (trigger, iteration) {
            (Some(CheckpointTrigger::OnError), _) => "Error checkpoint".to_string(),
            (_, Some(n)) if n > 0 => format!("Iteration {} checkpoint", n),
            (Some(trigger), _) => trigger_description(trigger),
            (None, _) => "Checkpoint".to_string(),
        }
    }

    pub fn should_checkpoint_before_node(triggers: &[CheckpointTrigger]) -> bool {
        triggers.contains(&CheckpointTrigger::BeforeExecute)
    }

    pub fn should_checkpoint_after_node(triggers: &[CheckpointTrigger]) -> bool {
        triggers.contains(&CheckpointTrigger::AfterExecute)
    }

    pub fn should_checkpoint_on_error(triggers: &[CheckpointTrigger]) -> bool {
        triggers.contains(&CheckpointTrigger::OnError)
    }

    pub fn should_checkpoint_on_pause(triggers: &[CheckpointTrigger]) -> bool {
        triggers.contains(&CheckpointTrigger::OnPause)
    }

    pub fn should_checkpoint_on_tool(triggers: &[CheckpointTrigger]) -> bool {
        triggers.contains(&CheckpointTrigger::ToolBefore)
            || triggers.contains(&CheckpointTrigger::ToolAfter)
    }
}

/// Convenience builders for the config layer inputs.
impl CheckpointConfigLayer {
    pub fn global(policy: UnifiedCheckpointPolicy) -> Self {
        Self::new(CheckpointConfigSource::Global, policy)
    }

    pub fn node(policy: UnifiedCheckpointPolicy) -> Self {
        Self::new(CheckpointConfigSource::Node, policy)
    }

    pub fn agent(policy: UnifiedCheckpointPolicy) -> Self {
        Self::new(CheckpointConfigSource::Agent, policy)
    }

    pub fn runtime(policy: UnifiedCheckpointPolicy) -> Self {
        Self::new(CheckpointConfigSource::Runtime, policy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(enabled: bool, triggers: Vec<CheckpointTrigger>) -> UnifiedCheckpointPolicy {
        UnifiedCheckpointPolicy {
            enabled,
            triggers,
            content: None,
            retention: None,
            error_handling: None,
        }
    }

    #[test]
    fn test_resolve_defaults() {
        let user = UnifiedCheckpointPolicy {
            enabled: true,
            triggers: vec![],
            content: None,
            retention: None,
            error_handling: None,
        };
        let resolved = CheckpointConfigResolver::resolve_from_user_config(&user);
        assert_eq!(resolved.triggers.len(), 2);
        assert!(resolved.retention.is_some());
    }

    #[test]
    fn test_should_checkpoint() {
        let triggers = vec![CheckpointTrigger::BeforeExecute, CheckpointTrigger::OnError];
        assert!(CheckpointConfigResolver::should_checkpoint_before_node(
            &triggers
        ));
        assert!(!CheckpointConfigResolver::should_checkpoint_after_node(
            &triggers
        ));
        assert!(CheckpointConfigResolver::should_checkpoint_on_error(
            &triggers
        ));
    }

    #[test]
    fn resolve_first_wins_highest_priority() {
        let layers = vec![
            CheckpointConfigLayer::global(policy(false, vec![CheckpointTrigger::OnError])),
            CheckpointConfigLayer::node(policy(
                true,
                vec![CheckpointTrigger::BeforeExecute],
            )),
            CheckpointConfigLayer::runtime(policy(
                true,
                vec![CheckpointTrigger::AfterExecute],
            )),
        ];
        let resolved = CheckpointConfigResolver::resolve(&layers);
        assert!(resolved.policy.enabled);
        assert_eq!(resolved.effective_source, CheckpointConfigSource::Runtime);
        assert_eq!(
            resolved.policy.triggers,
            vec![CheckpointTrigger::AfterExecute]
        );
    }

    #[test]
    fn resolve_lower_layer_fields_fill_gaps() {
        let mut node_policy = policy(true, Vec::new());
        node_policy.retention = Some(CheckpointRetentionConfig {
            max_checkpoints: Some(5),
            max_age: None,
            compression: None,
        });
        let layers = vec![
            CheckpointConfigLayer::global(policy(true, vec![CheckpointTrigger::OnError])),
            CheckpointConfigLayer::node(node_policy),
        ];
        let resolved = CheckpointConfigResolver::resolve(&layers);
        assert_eq!(
            resolved.policy.retention.unwrap().max_checkpoints,
            Some(5)
        );
    }

    #[test]
    fn resolve_disabled_default_when_no_explicit_enable() {
        let resolved = CheckpointConfigResolver::resolve(&[]);
        assert!(!resolved.should_create);
        assert_eq!(resolved.effective_source, CheckpointConfigSource::Default);
    }

    #[test]
    fn never_trigger_short_circuits() {
        let resolver = CheckpointConfigResolver;
        let resolved = CheckpointConfigResolver::resolve(&[CheckpointConfigLayer::global(
            policy(true, vec![CheckpointTrigger::Never]),
        )]);
        assert!(!resolver.should_create_checkpoint(&resolved, &CheckpointTrigger::OnError));
        assert!(!resolver.should_create_checkpoint(&resolved, &CheckpointTrigger::Manual));
    }

    #[test]
    fn agent_cadence_semantics() {
        let resolver = CheckpointConfigResolver;
        let resolved = CheckpointConfigResolver::resolve(&[CheckpointConfigLayer::agent(
            policy(true, vec![CheckpointTrigger::IterationEnd]),
        )]);

        assert!(resolver.evaluate_agent_trigger(
            &resolved,
            &CheckpointTrigger::IterationEnd,
            2,
            false,
            Some(2),
            None,
        ));
        assert!(!resolver.evaluate_agent_trigger(
            &resolved,
            &CheckpointTrigger::IterationEnd,
            3,
            false,
            Some(2),
            None,
        ));

        let error_only = CheckpointConfigResolver::resolve(&[CheckpointConfigLayer::agent(
            policy(true, vec![CheckpointTrigger::OnError]),
        )]);
        assert!(!resolver.evaluate_agent_trigger(
            &error_only,
            &CheckpointTrigger::OnError,
            1,
            false,
            None,
            Some(true),
        ));
        assert!(resolver.evaluate_agent_trigger(
            &error_only,
            &CheckpointTrigger::OnError,
            1,
            true,
            None,
            Some(true),
        ));
    }

    #[test]
    fn build_description_variants() {
        assert_eq!(
            CheckpointConfigResolver::build_description(
                &policy(true, Vec::new()),
                Some(&CheckpointTrigger::OnError),
                None,
            ),
            "Error checkpoint"
        );
        assert_eq!(
            CheckpointConfigResolver::build_description(
                &policy(true, Vec::new()),
                Some(&CheckpointTrigger::IterationEnd),
                Some(7),
            ),
            "Iteration 7 checkpoint"
        );
        assert_eq!(
            CheckpointConfigResolver::build_description(
                &policy(true, Vec::new()),
                Some(&CheckpointTrigger::Manual),
                None,
            ),
            "Manual checkpoint"
        );
    }
}
