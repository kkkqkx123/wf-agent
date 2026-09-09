use crate::error::ConfigResult;
use wf_types::config::file_checkpoint::{
    FileCheckpointConfig, FileCheckpointStorageConfig, FileCheckpointStorageType,
};

pub fn merge_file_checkpoint_with_defaults(user: &FileCheckpointConfig) -> FileCheckpointConfig {
    FileCheckpointConfig {
        enabled: user.enabled,
        workspace_root: user.workspace_root.clone(),
        custom_ignore_patterns: user.custom_ignore_patterns.clone(),
        storage: user.storage.as_ref().map(|s| FileCheckpointStorageConfig {
            storage_type: FileCheckpointStorageType::Sqlite,
            db_path: s.db_path.clone(),
        }),
        failure_behavior: user.failure_behavior,
        approval_policy: user.approval_policy,
        conflict_behavior: user.conflict_behavior,
        full_snapshot_threshold: user.full_snapshot_threshold,
        manual_watch: user.manual_watch,
        gc_interval_secs: user.gc_interval_secs,
        gc_retention: user.gc_retention,
    }
}

pub fn validate_file_checkpoint_config(config: &FileCheckpointConfig) -> ConfigResult<()> {
    if let Some(threshold) = config.full_snapshot_threshold {
        if !(0.0..=1.0).contains(&threshold) {
            return Err(crate::error::ConfigError::Validation(format!(
                "file_checkpoint.full_snapshot_threshold must be within [0.0, 1.0], got {threshold}"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::config::file_checkpoint::FailureBehavior;

    #[test]
    fn test_merge_file_checkpoint_with_defaults() {
        let user = FileCheckpointConfig {
            enabled: true,
            workspace_root: Some("/workspace".to_string()),
            custom_ignore_patterns: Some(vec!["*.log".to_string()]),
            storage: None,
            failure_behavior: FailureBehavior::Error,
            approval_policy: wf_types::config::file_checkpoint::ApprovalPolicy::Manual,
            conflict_behavior: wf_types::config::file_checkpoint::ConflictBehavior::Fail,
            full_snapshot_threshold: Some(0.7),
            manual_watch: true,
            gc_interval_secs: None,
            gc_retention: None,
        };
        let merged = merge_file_checkpoint_with_defaults(&user);
        assert!(merged.enabled);
        assert_eq!(merged.workspace_root, Some("/workspace".to_string()));
        assert_eq!(merged.full_snapshot_threshold, Some(0.7));
        assert_eq!(merged.failure_behavior, FailureBehavior::Error);
        assert_eq!(
            merged.approval_policy,
            wf_types::config::file_checkpoint::ApprovalPolicy::Manual
        );
        assert_eq!(
            merged.conflict_behavior,
            wf_types::config::file_checkpoint::ConflictBehavior::Fail
        );
        assert!(merged.manual_watch);
    }

    #[test]
    fn test_validate_file_checkpoint_config() {
        let config = FileCheckpointConfig {
            enabled: false,
            workspace_root: None,
            custom_ignore_patterns: None,
            storage: None,
            failure_behavior: FailureBehavior::Warn,
            approval_policy: wf_types::config::file_checkpoint::ApprovalPolicy::default(),
            conflict_behavior: wf_types::config::file_checkpoint::ConflictBehavior::default(),
            full_snapshot_threshold: Some(2.0),
            manual_watch: false,
            gc_interval_secs: None,
            gc_retention: None,
        };
        assert!(validate_file_checkpoint_config(&config).is_err());

        let config = FileCheckpointConfig {
            enabled: true,
            workspace_root: None,
            custom_ignore_patterns: None,
            storage: None,
            failure_behavior: FailureBehavior::Warn,
            approval_policy: wf_types::config::file_checkpoint::ApprovalPolicy::default(),
            conflict_behavior: wf_types::config::file_checkpoint::ConflictBehavior::default(),
            full_snapshot_threshold: Some(0.5),
            manual_watch: false,
            gc_interval_secs: None,
            gc_retention: None,
        };
        assert!(validate_file_checkpoint_config(&config).is_ok());
    }
}
