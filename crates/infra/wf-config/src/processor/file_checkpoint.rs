use crate::error::{ConfigError, ConfigResult};
use crate::processor::file_scope::{
    expand_file_scope, validate_file_scope, validate_optional_scope_path,
    validate_resolved_file_scope,
};
use wf_types::config::file_checkpoint::{
    FileCheckpointConfig, FileCheckpointStorageConfig, FileCheckpointStorageType,
};

/// Treat blank optional paths as unset so empty-string placeholders in
/// example files inherit instead of failing validation.
fn normalize_optional_path(value: Option<String>) -> Option<String> {
    value.filter(|path| !path.trim().is_empty())
}

pub fn merge_file_checkpoint_with_defaults(user: &FileCheckpointConfig) -> FileCheckpointConfig {
    FileCheckpointConfig {
        enabled: user.enabled,
        workspace_root: normalize_optional_path(user.workspace_root.clone()),
        custom_ignore_patterns: user.custom_ignore_patterns.clone(),
        storage: user.storage.as_ref().map(|s| FileCheckpointStorageConfig {
            storage_type: FileCheckpointStorageType::Sqlite,
            db_path: normalize_optional_path(s.db_path.clone()),
        }),
        failure_behavior: user.failure_behavior,
        approval_policy: user.approval_policy,
        conflict_behavior: user.conflict_behavior,
        manual_watch: user.manual_watch,
        manual_poll_ms: user.manual_poll_ms,
        gc_interval_secs: user.gc_interval_secs,
        gc_retention: user.gc_retention,
        // A missing default scope stays missing; it is never materialized
        // into an empty (deny-all) scope here.
        default_scope: user.default_scope.clone(),
        store_in_workspace: user.store_in_workspace,
        max_file_size: user.max_file_size,
    }
}

/// Fail-fast validation for the file-checkpoint config: reject blank
/// workspace roots and database paths, illegal ignore patterns, and a
/// manual watcher without a workspace to watch. Value ranges that carry
/// an explicit disabled meaning (`gc_interval_secs = 0`) are accepted so
/// the bootstrap keeps a single interpretation of them.
pub fn validate_file_checkpoint_config(config: &FileCheckpointConfig) -> ConfigResult<()> {
    if let Some(root) = config.workspace_root.as_deref() {
        validate_optional_scope_path(Some(root), "file_checkpoint.workspace_root")?;
    }
    if config.enabled
        && config.manual_watch
        && config
            .workspace_root
            .as_deref()
            .is_none_or(|root| root.trim().is_empty())
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
            if let Err(e) = glob::Pattern::new(pattern) {
                return Err(ConfigError::Validation(format!(
                    "file_checkpoint.custom_ignore_patterns entry '{pattern}' is not a valid glob: {e}"
                )));
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
            validate_optional_scope_path(Some(db_path), "file_checkpoint.storage.db_path")?;
            if let Some(root) = config.workspace_root.as_deref() {
                if root.starts_with('/') && db_path.starts_with('/') {
                    let root_norm = wf_types::config::file_scope::normalize_scope_path(root);
                    let db_norm = wf_types::config::file_scope::normalize_scope_path(db_path);
                    if !wf_types::config::file_scope::scope_path_contains(&root_norm, &db_norm) {
                        return Err(ConfigError::Validation(format!(
                            "file_checkpoint.storage.db_path '{db_path}' is outside workspace root '{root}'"
                        )));
                    }
                } else if root.starts_with('/') {
                    let root_norm = wf_types::config::file_scope::normalize_scope_path(root);
                    let joined = wf_types::config::file_scope::normalize_scope_path(&format!(
                        "{root_norm}/{db_path}"
                    ));
                    if !wf_types::config::file_scope::scope_path_contains(&root_norm, &joined) {
                        return Err(ConfigError::Validation(format!(
                            "file_checkpoint.storage.db_path '{db_path}' escapes workspace root '{root}'"
                        )));
                    }
                }
            }
        }
    }
    if let Some(poll_ms) = config.manual_poll_ms {
        if poll_ms == 0 {
            return Err(ConfigError::Validation(
                "file_checkpoint.manual_poll_ms must not be zero".to_string(),
            ));
        }
    }
    if let Some(scope) = config.default_scope.as_ref() {
        validate_file_scope(scope)?;
        let expanded = expand_file_scope(scope, None, config.workspace_root.as_deref());
        validate_resolved_file_scope(&expanded)
            .map_err(|e| ConfigError::Validation(format!("file_checkpoint.default_scope: {e}")))?;
        if let Some(root) = config.workspace_root.as_deref() {
            if root.starts_with('/') {
                let root_norm = wf_types::config::file_scope::normalize_scope_path(root);
                let all: Vec<&String> = expanded
                    .read_prefixes
                    .iter()
                    .chain(expanded.write_prefixes.iter())
                    .chain(expanded.checkpoint_prefixes.iter())
                    .collect();
                for prefix in all {
                    if !wf_types::config::file_scope::scope_path_contains(&root_norm, prefix) {
                        return Err(ConfigError::Validation(format!(
                            "file_checkpoint.default_scope entry '{prefix}' is outside workspace root '{root}'"
                        )));
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::config::file_checkpoint::FailureBehavior;
    use wf_types::config::file_scope::ExecutionFileScope;

    fn base_config() -> FileCheckpointConfig {
        FileCheckpointConfig {
            enabled: true,
            workspace_root: Some("/workspace".to_string()),
            custom_ignore_patterns: Some(vec!["*.log".to_string()]),
            storage: None,
            failure_behavior: FailureBehavior::Error,
            approval_policy: wf_types::config::file_checkpoint::ApprovalPolicy::Manual,
            conflict_behavior: wf_types::config::file_checkpoint::ConflictBehavior::Fail,
            manual_watch: true,
            manual_poll_ms: None,
            gc_interval_secs: None,
            gc_retention: None,
            default_scope: None,
            store_in_workspace: None,
            max_file_size: None,
        }
    }

    #[test]
    fn test_merge_file_checkpoint_with_defaults() {
        let user = base_config();
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
        assert!(merged.default_scope.is_none());
    }

    #[test]
    fn test_merge_normalizes_blank_paths_to_unset() {
        let mut user = base_config();
        user.workspace_root = Some("   ".to_string());
        user.storage = Some(FileCheckpointStorageConfig {
            storage_type: wf_types::config::file_checkpoint::FileCheckpointStorageType::Sqlite,
            db_path: Some(String::new()),
        });
        let merged = merge_file_checkpoint_with_defaults(&user);
        assert!(merged.workspace_root.is_none());
        assert!(merged.storage.as_ref().unwrap().db_path.is_none());
    }

    #[test]
    fn test_merge_passes_default_scope_through() {
        let mut user = base_config();
        user.default_scope = Some(ExecutionFileScope {
            enabled: Some(false),
            read_prefixes: None,
            write_prefixes: Some(vec![]),
            checkpoint_prefixes: Some(vec![]),
            workdir: None,
        });
        let merged = merge_file_checkpoint_with_defaults(&user);
        let scope = merged.default_scope.expect("scope survives merge");
        assert_eq!(scope.enabled, Some(false));
        assert_eq!(scope.write_prefixes, Some(vec![]));
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
            manual_poll_ms: None,
            gc_interval_secs: None,
            gc_retention: None,
            default_scope: None,
            store_in_workspace: None,
            max_file_size: None,
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
            manual_poll_ms: None,
            gc_interval_secs: None,
            gc_retention: None,
            default_scope: None,
            store_in_workspace: None,
            max_file_size: None,
        }
    }

    #[test]
    fn test_validate_accepts_enabled_config() {
        assert!(validate_file_checkpoint_config(&enabled_config()).is_ok());
    }

    #[test]
    fn test_validate_accepts_relative_workspace_root() {
        let mut config = enabled_config();
        config.workspace_root = Some("work/alias".to_string());
        assert!(validate_file_checkpoint_config(&config).is_ok());
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
    fn test_validate_rejects_invalid_glob_pattern() {
        let mut config = enabled_config();
        config.custom_ignore_patterns = Some(vec!["[unclosed".to_string()]);
        let err = validate_file_checkpoint_config(&config).expect_err("bad glob must fail");
        assert!(err.to_string().contains("[unclosed"), "error: {err}");
    }

    #[test]
    fn test_validate_accepts_recursive_globs() {
        let mut config = enabled_config();
        config.custom_ignore_patterns = Some(vec![
            "node_modules/**".to_string(),
            ".git/**".to_string(),
            "dist/**".to_string(),
        ]);
        assert!(validate_file_checkpoint_config(&config).is_ok());
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

    #[test]
    fn test_validate_accepts_relative_db_path_inside_root() {
        let mut config = enabled_config();
        config.storage = Some(FileCheckpointStorageConfig {
            storage_type: wf_types::config::file_checkpoint::FileCheckpointStorageType::Sqlite,
            db_path: Some("state/checkpoint.db".to_string()),
        });
        assert!(validate_file_checkpoint_config(&config).is_ok());
    }

    #[test]
    fn test_validate_rejects_escaping_db_path() {
        let mut config = enabled_config();
        config.storage = Some(FileCheckpointStorageConfig {
            storage_type: wf_types::config::file_checkpoint::FileCheckpointStorageType::Sqlite,
            db_path: Some("../outside.db".to_string()),
        });
        assert!(validate_file_checkpoint_config(&config).is_err());
        config.storage = Some(FileCheckpointStorageConfig {
            storage_type: wf_types::config::file_checkpoint::FileCheckpointStorageType::Sqlite,
            db_path: Some("/outside.db".to_string()),
        });
        assert!(validate_file_checkpoint_config(&config).is_err());
    }

    #[test]
    fn test_validate_accepts_explicitly_closed_default_scope() {
        let mut config = enabled_config();
        config.default_scope = Some(ExecutionFileScope {
            enabled: Some(false),
            read_prefixes: Some(vec![]),
            write_prefixes: Some(vec![]),
            checkpoint_prefixes: Some(vec![]),
            workdir: None,
        });
        assert!(validate_file_checkpoint_config(&config).is_ok());
    }

    #[test]
    fn test_validate_rejects_out_of_bounds_default_scope() {
        let mut config = enabled_config();
        config.default_scope = Some(ExecutionFileScope {
            enabled: Some(true),
            read_prefixes: Some(vec!["/workspace".to_string()]),
            write_prefixes: Some(vec!["/workspace/app".to_string()]),
            checkpoint_prefixes: Some(vec!["/workspace/other".to_string()]),
            workdir: None,
        });
        assert!(validate_file_checkpoint_config(&config).is_err());
    }
}
