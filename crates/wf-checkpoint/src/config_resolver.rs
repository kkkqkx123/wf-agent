use wf_types::checkpoint::base::{CheckpointRetentionConfig, CheckpointTrigger, UnifiedCheckpointPolicy};

#[derive(Debug, Clone)]
pub struct CheckpointConfigResolver;

impl CheckpointConfigResolver {
    pub fn resolve_from_user_config(user_policy: &UnifiedCheckpointPolicy) -> UnifiedCheckpointPolicy {
        let mut policy = user_policy.clone();

        if policy.triggers.is_empty() {
            policy.triggers = vec![
                CheckpointTrigger::AfterExecute,
                CheckpointTrigger::OnError,
            ];
        }

        if policy.retention.is_none() {
            policy.retention = Some(CheckpointRetentionConfig {
                max_checkpoints: Some(10),
                max_age: None,
                compression: Some(true),
            });
        }

        policy
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(CheckpointConfigResolver::should_checkpoint_before_node(&triggers));
        assert!(!CheckpointConfigResolver::should_checkpoint_after_node(&triggers));
        assert!(CheckpointConfigResolver::should_checkpoint_on_error(&triggers));
    }
}
