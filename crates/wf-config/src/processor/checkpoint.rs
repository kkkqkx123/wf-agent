use crate::error::ConfigResult;
use crate::validator::validate_min;
use wf_types::checkpoint::base::{
    CheckpointContentConfig, CheckpointErrorHandlingConfig, CheckpointRetentionConfig,
    CheckpointTiming, CompressionStrategy, UnifiedCheckpointPolicy,
};

pub fn merge_checkpoint_with_defaults(user: &UnifiedCheckpointPolicy) -> UnifiedCheckpointPolicy {
    UnifiedCheckpointPolicy {
        enabled: user.enabled,
        triggers: if user.triggers.is_empty() {
            vec![CheckpointTiming::AfterExecute, CheckpointTiming::OnError]
        } else {
            user.triggers.clone()
        },
        content: user.content.clone().or(Some(CheckpointContentConfig {
            include_state: Some(true),
            include_history: Some(true),
            include_statistics: Some(false),
            metadata: None,
            asynchronous: None,
        })),
        retention: user.retention.clone().or(Some(CheckpointRetentionConfig {
            max_checkpoints: Some(10),
            max_age: None,
            compression: Some(CompressionStrategy::Auto),
        })),
        error_handling: user
            .error_handling
            .clone()
            .or(Some(CheckpointErrorHandlingConfig {
                // checkpoint write failures are visible by default —
                // silently swallowed checkpoint errors hide history gaps.
                fail_on_checkpoint_error: Some(true),
                retry_on_failure: Some(true),
                max_retries: Some(3),
            })),
    }
}

pub fn validate_checkpoint_config(config: &UnifiedCheckpointPolicy) -> ConfigResult<()> {
    if let Some(ref retention) = config.retention {
        if let Some(max) = retention.max_checkpoints {
            validate_min(max as u64, 1, "checkpoint.retention.max_checkpoints")?;
        }
    }
    if let Some(ref error_handling) = config.error_handling {
        if let Some(max) = error_handling.max_retries {
            validate_min(max as u64, 0, "checkpoint.error_handling.max_retries")?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(merged.triggers.len(), 2);
        assert!(merged.content.is_some());
        assert!(merged.retention.is_some());
        assert!(merged.error_handling.is_some());
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
                fail_on_checkpoint_error: None,
                retry_on_failure: None,
                max_retries: Some(3),
            }),
        };
        assert!(validate_checkpoint_config(&config).is_ok());
    }
}
