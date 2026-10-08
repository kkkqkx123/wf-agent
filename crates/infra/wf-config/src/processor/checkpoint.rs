use crate::error::ConfigResult;
use crate::validator::validate_min;
use wf_types::checkpoint::base::{
    CheckpointRetentionConfig, CompressionStrategy, UnifiedCheckpointPolicy,
};

pub fn merge_checkpoint_with_defaults(user: &UnifiedCheckpointPolicy) -> UnifiedCheckpointPolicy {
    UnifiedCheckpointPolicy {
        enabled: user.enabled,
        // Empty trigger set means checkpointing is disabled; no default fill.
        triggers: user.triggers.clone(),
        // Content defaults are materialized solely in
        // StandardStrategy::from_policy; this layer must not duplicate them.
        content: user.content.clone(),
        retention: user.retention.clone().or(Some(CheckpointRetentionConfig {
            max_checkpoints: Some(10),
            max_age: None,
            compression: Some(CompressionStrategy::Auto),
        })),
        error_handling: user.error_handling.clone(),
    }
}

pub fn validate_checkpoint_config(config: &UnifiedCheckpointPolicy) -> ConfigResult<()> {
    if let Some(ref retention) = config.retention {
        if let Some(max) = retention.max_checkpoints {
            validate_min(max as u64, 1, "checkpoint.retention.max_checkpoints")?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::checkpoint::base::CheckpointErrorHandlingConfig;
    use wf_types::checkpoint::base::CheckpointTiming;

    #[test]
    fn test_merge_checkpoint_with_defaults() {
        let user = UnifiedCheckpointPolicy {
            enabled: true,
            triggers: vec![],
            content: None,
            retention: None,
            error_handling: None,
        };
        let merged = merge_checkpoint_with_defaults(&user);
        assert!(merged.enabled);
        // Empty trigger set stays empty: it means checkpointing is disabled.
        assert!(merged.triggers.is_empty());
        // Content defaults are no longer materialized here: they live only
        // in the strategy layer (`from_policy`).
        assert!(merged.content.is_none());
        assert!(merged.retention.is_some());
        // unconfigured error handling stays absent: the handler default
        // (surface the failure to the caller) applies.
        assert!(merged.error_handling.is_none());
        assert_eq!(merged.retention.as_ref().unwrap().max_checkpoints, Some(10));
    }

    #[test]
    fn test_validate_checkpoint_config() {
        let config = UnifiedCheckpointPolicy {
            enabled: true,
            triggers: vec![CheckpointTiming::AfterExecute],
            content: None,
            retention: Some(CheckpointRetentionConfig {
                max_checkpoints: Some(0),
                max_age: None,
                compression: None,
            }),
            error_handling: None,
        };
        assert!(validate_checkpoint_config(&config).is_err());

        let config = UnifiedCheckpointPolicy {
            enabled: true,
            triggers: vec![CheckpointTiming::AfterExecute],
            content: None,
            retention: Some(CheckpointRetentionConfig {
                max_checkpoints: Some(10),
                max_age: None,
                compression: None,
            }),
            error_handling: Some(CheckpointErrorHandlingConfig {
                fail_on_checkpoint_error: Some(true),
            }),
        };
        assert!(validate_checkpoint_config(&config).is_ok());
    }
}
