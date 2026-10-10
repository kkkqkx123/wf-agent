use crate::error::{ConfigError, ConfigResult};
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
        manual_watch: user.manual_watch,
        gc_interval_secs: user.gc_interval_secs,
        gc_retention: user.gc_retention,
    }
}

/// Fail-fast validation for the file-checkpoint config: reject empty
/// workspace roots and database paths, illegal ignore patterns, and a
/// manual watcher without a workspace to watch. Value ranges that carry
/// an explicit disabled meaning (`gc_interval_secs = 0`) are accepted so
/// the bootstrap keeps a single interpretation of them.
pub fn validate_file_checkpoint_config(config: &FileCheckpointConfig) -> ConfigResult<()> {
    if let Some(root) = config.workspace_root.as_deref() {
        if root.trim().is_empty() {
            return Err(ConfigError::Validation(
                "file_checkpoint.workspace_root must not be empty".to_string(),
            ));
        }
    }
    if config.enabled
        && config.manual_watch
        && config.workspace_root.as_deref().is_none_or(|root| root.trim().is_empty())
    {
        return Err(ConfigError::Validation(
            "file_checkpoint.manual_watch requires file_checkpoint.workspace_root".to_string(),
        ));
    }
    if let Some(patterns) = config.custom_ignore_patterns.as_deref() {
        for pattern in patterns {
            if pattern.is_empty() || pattern.len() > 256 {
                return Err(ConfigError::Validation(format!(
                    "file_checkpoint.custom_ignore_patterns entries must be 1..=256 chars, got '{pattern}'"
                )));
            }
            if pattern.contains('\0') {
                return Err(ConfigError::Validation(
                    "file_checkpoint.custom_ignore_patterns entries must not contain NUL"
                        .to_string(),
                ));
            }
        }
    }
    if let Some(storage) = config.storage.as_ref() {
        if let Some(db_path) = storage.db_path.as_deref() {
            if db_path.trim().is_empty() {
                return Err(ConfigError::Validation(
                    "file_checkpoint.storage.db_path must not be empty".to_string(),
                ));
            }
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
            manual_watch: true,
            gc_interval_secs: None,
            gc_retention: None,
        };
        let merged = merge_file_checkpoint_with_defaults(&user);
        assert!(merged.enabled);
        assert_eq!(merged.workspace_root, Some("/workspace".to_string()));
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
            manual_watch: false,
            gc_interval_secs: None,
            gc_retention: None,
        };
        assert!(validate_file_checkpoint_config(&config).is_ok());
    }

    fn enabled_config() -> FileCheckpointConfig {
        FileCheckpointConfig {
            enabled: true,
            workspace_root: Some("/workspace".to_string()),
            custom_ignore_patterns: None,
            storage: None,
            failure_behavior: FailureBehavior::Warn,
            approval_policy: wf_types::config::file_checkpoint::ApprovalPolicy::default(),
            conflict_behavior: wf_types::config::file_checkpoint::ConflictBehavior::default(),
            manual_watch: false,
            gc_interval_secs: None,
            gc_retention: None,
        }
    }

    #[test]
    fn test_validate_accepts_enabled_config() {
        assert!(validate_file_checkpoint_config(&enabled_config()).is_ok());
    }

    #[test]
    fn test_validate_rejects_empty_workspace_root() {
        let mut config = enabled_config();
        config.workspace_root = Some("   ".to_string());
        assert!(validate_file_checkpoint_config(&config).is_err());
    }

    #[test]
    fn test_validate_rejects_manual_watch_without_root() {
        let mut config = enabled_config();
        config.workspace_root = None;
        config.manual_watch = true;
        assert!(validate_file_checkpoint_config(&config).is_err());
    }

    #[test]
    fn test_validate_rejects_illegal_ignore_patterns() {
        let mut config = enabled_config();
        config.custom_ignore_patterns = Some(vec!["".to_string()]);
        assert!(validate_file_checkpoint_config(&config).is_err());
        config.custom_ignore_patterns = Some(vec!["a\0b".to_string()]);
        assert!(validate_file_checkpoint_config(&config).is_err());
    }

    #[test]
    fn test_validate_rejects_empty_db_path() {
        let mut config = enabled_config();
        config.storage = Some(FileCheckpointStorageConfig {
            storage_type: wf_types::config::file_checkpoint::FileCheckpointStorageType::Sqlite,
            db_path: Some("  ".to_string()),
        });
        assert!(validate_file_checkpoint_config(&config).is_err());
    }
}
