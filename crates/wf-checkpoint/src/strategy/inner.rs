use wf_types::checkpoint::{
    CheckpointContentConfig, CheckpointContext, CheckpointRetentionConfig, CheckpointTrigger,
    UnifiedCheckpointPolicy,
};

pub trait CheckpointStrategy: Send + Sync {
    fn should_checkpoint(&self, trigger: &CheckpointTrigger, context: &CheckpointContext) -> bool;
    fn content_config(&self) -> &CheckpointContentConfig;
    fn retention_config(&self) -> Option<&CheckpointRetentionConfig>;
}

#[derive(Debug, Clone)]
pub struct StandardStrategy {
    enabled: bool,
    triggers: Vec<CheckpointTrigger>,
    content: CheckpointContentConfig,
    retention: Option<CheckpointRetentionConfig>,
}

impl StandardStrategy {
    pub fn from_policy(policy: &UnifiedCheckpointPolicy) -> Self {
        Self {
            enabled: policy.enabled,
            triggers: policy.triggers.clone(),
            content: policy.content.clone().unwrap_or(CheckpointContentConfig {
                include_state: Some(true),
                include_history: Some(true),
                include_statistics: Some(true),
                metadata: None,
            }),
            retention: policy.retention.clone(),
        }
    }

    pub fn disabled() -> Self {
        Self {
            enabled: false,
            triggers: vec![],
            content: CheckpointContentConfig {
                include_state: None,
                include_history: None,
                include_statistics: None,
                metadata: None,
            },
            retention: None,
        }
    }
}

impl CheckpointStrategy for StandardStrategy {
    fn should_checkpoint(&self, trigger: &CheckpointTrigger, _context: &CheckpointContext) -> bool {
        if !self.enabled {
            return false;
        }
        if self.triggers.is_empty() {
            return false;
        }
        self.triggers.contains(trigger)
    }

    fn content_config(&self) -> &CheckpointContentConfig {
        &self.content
    }

    fn retention_config(&self) -> Option<&CheckpointRetentionConfig> {
        self.retention.as_ref()
    }
}

pub fn create_checkpoint_strategy(policy: &UnifiedCheckpointPolicy) -> StandardStrategy {
    if policy.enabled {
        StandardStrategy::from_policy(policy)
    } else {
        StandardStrategy::disabled()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_policy(triggers: Vec<CheckpointTrigger>) -> UnifiedCheckpointPolicy {
        UnifiedCheckpointPolicy {
            enabled: true,
            triggers,
            content: None,
            retention: None,
            error_handling: None,
        }
    }

    fn make_context() -> CheckpointContext {
        CheckpointContext {
            entity_type: "test".to_string(),
            entity_id: "test-1".to_string(),
            attempt: None,
            retry_count: None,
            error: None,
            fallback_used: None,
            metadata: None,
        }
    }

    #[test]
    fn standard_strategy_should_checkpoint() {
        let strategy = create_checkpoint_strategy(&make_policy(vec![
            CheckpointTrigger::BeforeExecute,
            CheckpointTrigger::AfterExecute,
        ]));
        let ctx = make_context();

        assert!(strategy.should_checkpoint(&CheckpointTrigger::BeforeExecute, &ctx));
        assert!(strategy.should_checkpoint(&CheckpointTrigger::AfterExecute, &ctx));
        assert!(!strategy.should_checkpoint(&CheckpointTrigger::OnError, &ctx));
    }

    #[test]
    fn disabled_strategy_never_checkpoints() {
        let strategy = create_checkpoint_strategy(&UnifiedCheckpointPolicy {
            enabled: false,
            triggers: vec![CheckpointTrigger::BeforeExecute],
            content: None,
            retention: None,
            error_handling: None,
        });
        let ctx = make_context();

        assert!(!strategy.should_checkpoint(&CheckpointTrigger::BeforeExecute, &ctx));
    }

    #[test]
    fn empty_triggers_never_checkpoints() {
        let strategy = create_checkpoint_strategy(&make_policy(vec![]));
        let ctx = make_context();

        assert!(!strategy.should_checkpoint(&CheckpointTrigger::BeforeExecute, &ctx));
    }

    #[test]
    fn content_config_defaults() {
        let strategy = create_checkpoint_strategy(&make_policy(vec![CheckpointTrigger::Manual]));
        let config = strategy.content_config();

        assert_eq!(config.include_state, Some(true));
        assert_eq!(config.include_history, Some(true));
    }
}
