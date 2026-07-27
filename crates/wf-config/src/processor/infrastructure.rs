use crate::error::{ConfigError, ConfigResult};
use crate::validator::validate_min;

use wf_types::config::metrics::MetricsConfig;
use wf_types::config::output::OutputConfig;
use wf_types::config::schemas::StorageConfig;
use wf_types::config::timeout::TimeoutConfig;
use wf_types::script::sandbox::{ResourceLimits, SandboxConfig, SandboxMode};

pub const WAIT_FOREVER: i64 = -1;

pub fn merge_timeout_with_defaults(user: &TimeoutConfig) -> TimeoutConfig {
    TimeoutConfig {
        default_node_timeout_seconds: user
            .default_node_timeout_seconds
            .or(Some(30)),
        default_workflow_timeout_seconds: user
            .default_workflow_timeout_seconds
            .or(Some(300)),
        max_timeout_seconds: user.max_timeout_seconds.or(Some(3000)),
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
    MetricsConfig {
        enabled: user.enabled,
        export_interval_seconds: user.export_interval_seconds.or(Some(60)),
        include_node_metrics: user.include_node_metrics.or(Some(true)),
        include_llm_metrics: user.include_llm_metrics.or(Some(true)),
        include_tool_metrics: user.include_tool_metrics.or(Some(true)),
    }
}

pub fn merge_output_with_defaults(user: &OutputConfig) -> OutputConfig {
    OutputConfig {
        format: user.format.clone().or(Some("json".to_string())),
        include_metadata: user.include_metadata.or(Some(true)),
        include_errors: user.include_errors.or(Some(true)),
        max_output_size: user.max_output_size.or(Some(10_000_000)),
    }
}

pub fn merge_storage_with_defaults(user: &StorageConfig) -> StorageConfig {
    StorageConfig {
        storage_type: user.storage_type.clone(),
        connection_string: user.connection_string.clone(),
        max_connections: user.max_connections.or(Some(10)),
    }
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
            default_node_timeout_seconds: Some(60),
            default_workflow_timeout_seconds: None,
            max_timeout_seconds: None,
        };
        let merged = merge_timeout_with_defaults(&user);
        assert_eq!(merged.default_node_timeout_seconds, Some(60));
        assert_eq!(merged.default_workflow_timeout_seconds, Some(300));
        assert_eq!(merged.max_timeout_seconds, Some(3000));
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
        let user = MetricsConfig {
            enabled: false,
            export_interval_seconds: None,
            include_node_metrics: None,
            include_llm_metrics: None,
            include_tool_metrics: None,
        };
        let merged = merge_metrics_with_defaults(&user);
        assert!(!merged.enabled);
        assert_eq!(merged.export_interval_seconds, Some(60));
    }

    #[test]
    fn test_merge_output_with_defaults() {
        let user = OutputConfig {
            format: None,
            include_metadata: None,
            include_errors: None,
            max_output_size: None,
        };
        let merged = merge_output_with_defaults(&user);
        assert_eq!(merged.format, Some("json".to_string()));
        assert_eq!(merged.include_metadata, Some(true));
    }

    #[test]
    fn test_merge_storage_with_defaults() {
        let user = StorageConfig {
            storage_type: "sqlite".to_string(),
            connection_string: None,
            max_connections: None,
        };
        let merged = merge_storage_with_defaults(&user);
        assert_eq!(merged.storage_type, "sqlite");
        assert_eq!(merged.max_connections, Some(10));
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
