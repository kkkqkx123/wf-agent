use crate::error::ConfigResult;
use crate::validator::validate_min;
use wf_types::config::file_checkpoint::{
    FileCheckpointConfig, FileCheckpointStorageConfig, FileCheckpointStorageType,
};

pub fn merge_file_checkpoint_with_defaults(user: &FileCheckpointConfig) -> FileCheckpointConfig {
    FileCheckpointConfig {
        enabled: user.enabled,
        workspace_root: user.workspace_root.clone(),
        max_delta_chain_length: user.max_delta_chain_length,
        custom_ignore_patterns: user.custom_ignore_patterns.clone(),
        storage: user.storage.as_ref().map(|s| FileCheckpointStorageConfig {
            storage_type: FileCheckpointStorageType::Sqlite,
            db_path: s.db_path.clone(),
        }),
        failure_behavior: user.failure_behavior,
        approval_policy: user.approval_policy,
        conflict_behavior: user.conflict_behavior,
        manual_watch: user.manual_watch,
    }
}

pub fn validate_file_checkpoint_config(config: &FileCheckpointConfig) -> ConfigResult<()> {
    validate_min(
        config.max_delta_chain_length as u64,
        1,
        "file_checkpoint.max_delta_chain_length",
    )?;
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
            max_delta_chain_length: 30,
            custom_ignore_patterns: Some(vec!["*.log".to_string()]),
            storage: None,
            failure_behavior: FailureBehavior::Error,
            approval_policy: wf_types::config::file_checkpoint::ApprovalPolicy::Manual,
            conflict_behavior: wf_types::config::file_checkpoint::ConflictBehavior::Fail,
            manual_watch: true,
        };
        let merged = merge_file_checkpoint_with_defaults(&user);
        assert!(merged.enabled);
        assert_eq!(merged.workspace_root, Some("/workspace".to_string()));
        assert_eq!(merged.max_delta_chain_length, 30);
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
            max_delta_chain_length: 0,
            custom_ignore_patterns: None,
            storage: None,
            failure_behavior: FailureBehavior::Warn,
            approval_policy: wf_types::config::file_checkpoint::ApprovalPolicy::default(),
            conflict_behavior: wf_types::config::file_checkpoint::ConflictBehavior::default(),
            manual_watch: false,
        };
        assert!(validate_file_checkpoint_config(&config).is_err());

        let config = FileCheckpointConfig {
            enabled: true,
            workspace_root: None,
            max_delta_chain_length: 10,
            custom_ignore_patterns: None,
            storage: None,
            failure_behavior: FailureBehavior::Warn,
            approval_policy: wf_types::config::file_checkpoint::ApprovalPolicy::default(),
            conflict_behavior: wf_types::config::file_checkpoint::ConflictBehavior::default(),
            manual_watch: false,
        };
        assert!(validate_file_checkpoint_config(&config).is_ok());
    }
}
