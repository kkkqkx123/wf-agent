use crate::error::{ConfigError, ConfigResult};
use crate::validator::validate_min;

use wf_types::config::metrics::MetricsConfig;
use wf_types::config::output::OutputConfig;
use wf_types::config::storage::StorageConfig;
use wf_types::config::timeout::TimeoutConfig;
use wf_types::script::sandbox::{ResourceLimits, SandboxConfig, SandboxMode};

pub const WAIT_FOREVER: i64 = -1;

pub fn merge_timeout_with_defaults(user: &TimeoutConfig) -> TimeoutConfig {
    TimeoutConfig {
        workflow_execution_completion: user.workflow_execution_completion.or(Some(30000)),
        workflow_execution_pause: user.workflow_execution_pause.or(Some(5000)),
        workflow_execution_cancel: user.workflow_execution_cancel.or(Some(10000)),
        workflow_execution_resume: user.workflow_execution_resume.or(Some(5000)),
        child_execution_wait: user.child_execution_wait.or(Some(30000)),
        cascade_cancel: user.cascade_cancel.or(Some(30000)),
        node_completion: user.node_completion.or(Some(30000)),
        node_failed: user.node_failed.or(Some(30000)),
        sync_branch_wait: user.sync_branch_wait.or(Some(60000)),
        join_completion: user.join_completion.or(Some(60000)),
        lifecycle_event: user.lifecycle_event.or(Some(5000)),
        polling_wait: user.polling_wait.or(Some(30000)),
        polling_interval: user.polling_interval.or(Some(100)),
        default: user.default.or(Some(30000)),
        max_allowed: user.max_allowed.or(Some(300000)),
    }
}

pub fn validate_timeout(timeout: i64, context: &str) -> ConfigResult<()> {
    if timeout < 0 && timeout != WAIT_FOREVER {
        return Err(ConfigError::Validation(format!(
            "Invalid timeout for {context}: {timeout}ms (must be non-negative or WAIT_FOREVER)"
        )));
    }
    Ok(())
}

pub fn merge_metrics_with_defaults(user: &MetricsConfig) -> MetricsConfig {
    user.clone()
}

pub fn merge_output_with_defaults(user: &OutputConfig) -> OutputConfig {
    user.clone()
}

pub fn merge_storage_with_defaults(user: &StorageConfig) -> StorageConfig {
    user.clone()
}

pub fn merge_sandbox_with_defaults(user: &SandboxConfig) -> SandboxConfig {
    SandboxConfig {
        mode: user.mode.clone().or(Some(SandboxMode::Strict)),
        policy: user.policy.clone(),
        shell_strategy: user.shell_strategy.clone(),
        python_strategy: user.python_strategy.clone(),
        javascript_strategy: user.javascript_strategy.clone(),
        lua_strategy: user.lua_strategy.clone(),
        vfs: user.vfs.clone(),
        legacy_type: user.legacy_type.clone(),
        image: user.image.clone(),
        resource_limits: user.resource_limits.clone().or(Some(ResourceLimits {
            cpu: None,
            memory: Some(512),
            disk: Some(1024),
        })),
        network_enabled: user.network_enabled,
        allowed_paths: user.allowed_paths.clone(),
    }
}

pub fn validate_sandbox_config(config: &SandboxConfig) -> ConfigResult<()> {
    if let Some(ref limits) = config.resource_limits {
        if let Some(mem) = limits.memory {
            validate_min(mem, 1, "resource_limits.memory")?;
        }
        if let Some(disk) = limits.disk {
            validate_min(disk, 1, "resource_limits.disk")?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_timeout_with_defaults() {
        let user = TimeoutConfig {
            workflow_execution_completion: Some(60000),
            workflow_execution_pause: None,
            workflow_execution_cancel: None,
            workflow_execution_resume: None,
            child_execution_wait: None,
            cascade_cancel: None,
            node_completion: None,
            node_failed: None,
            sync_branch_wait: None,
            join_completion: None,
            lifecycle_event: None,
            polling_wait: None,
            polling_interval: None,
            default: None,
            max_allowed: None,
        };
        let merged = merge_timeout_with_defaults(&user);
        assert_eq!(merged.workflow_execution_completion, Some(60000));
        assert_eq!(merged.workflow_execution_pause, Some(5000));
        assert_eq!(merged.default, Some(30000));
        assert_eq!(merged.max_allowed, Some(300000));
    }

    #[test]
    fn test_validate_timeout() {
        assert!(validate_timeout(1000, "test").is_ok());
        assert!(validate_timeout(0, "test").is_ok());
        assert!(validate_timeout(WAIT_FOREVER, "test").is_ok());
        assert!(validate_timeout(-2, "test").is_err());
    }

    #[test]
    fn test_merge_metrics_with_defaults() {
        let user = MetricsConfig::default();
        let merged = merge_metrics_with_defaults(&user);
        assert_eq!(merged.enabled, None);
    }

    #[test]
    fn test_merge_output_with_defaults() {
        let user = OutputConfig {
            dir: "/logs".to_string(),
            log_file_pattern: "app.log".to_string(),
            enable_log_terminal: true,
            enable_sdk_logs: true,
            sdk_log_level: wf_types::config::output::SdkLogLevel::Info,
        };
        let merged = merge_output_with_defaults(&user);
        assert_eq!(merged.dir, "/logs");
        assert_eq!(merged.sdk_log_level, wf_types::config::output::SdkLogLevel::Info);
    }

    #[test]
    fn test_merge_storage_with_defaults() {
        let user = StorageConfig {
            storage_type: wf_types::config::storage::StorageType::Sqlite,
            sqlite: None,
            postgres: None,
        };
        let merged = merge_storage_with_defaults(&user);
        assert_eq!(merged.storage_type, wf_types::config::storage::StorageType::Sqlite);
    }

    #[test]
    fn test_merge_sandbox_with_defaults() {
        let user = SandboxConfig {
            mode: Some(SandboxMode::Lenient),
            policy: None,
            shell_strategy: None,
            python_strategy: None,
            javascript_strategy: None,
            lua_strategy: None,
            vfs: None,
            legacy_type: None,
            image: None,
            resource_limits: None,
            network_enabled: Some(false),
            allowed_paths: Some(vec!["/tmp".to_string()]),
        };
        let merged = merge_sandbox_with_defaults(&user);
        assert_eq!(merged.mode, Some(SandboxMode::Lenient));
        assert_eq!(merged.allowed_paths, Some(vec!["/tmp".to_string()]));
        assert_eq!(
            merged.resource_limits.as_ref().unwrap().memory,
            Some(512)
        );
    }

    #[test]
    fn test_validate_sandbox_config() {
        let config = SandboxConfig {
            mode: Some(SandboxMode::Strict),
            policy: None,
            shell_strategy: None,
            python_strategy: None,
            javascript_strategy: None,
            lua_strategy: None,
            vfs: None,
            legacy_type: None,
            image: None,
            resource_limits: Some(ResourceLimits {
                cpu: None,
                memory: Some(0),
                disk: Some(100),
            }),
            network_enabled: None,
            allowed_paths: None,
        };
        assert!(validate_sandbox_config(&config).is_err());

        let config = SandboxConfig {
            mode: Some(SandboxMode::Strict),
            policy: None,
            shell_strategy: None,
            python_strategy: None,
            javascript_strategy: None,
            lua_strategy: None,
            vfs: None,
            legacy_type: None,
            image: None,
            resource_limits: Some(ResourceLimits {
                cpu: None,
                memory: Some(512),
                disk: Some(1024),
            }),
            network_enabled: None,
            allowed_paths: None,
        };
        assert!(validate_sandbox_config(&config).is_ok());
    }
}
