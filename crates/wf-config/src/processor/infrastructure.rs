use crate::error::{ConfigError, ConfigResult};
use crate::validator::validate_min;

use wf_metrics::collectors::ConfigMetricsCollector;
use wf_types::config::metrics::{MetricCollectorConfig, MetricsConfig};
use wf_types::config::output::OutputConfig;
use wf_types::config::storage::StorageConfig;
use wf_types::config::timeout::TimeoutConfig;
use wf_types::script::sandbox::{ResourceLimits, SandboxConfig, SandboxMode};

pub const WAIT_FOREVER: i64 = -1;

/// Optional config metrics hook; `None` keeps the merge path zero-overhead.
pub type ConfigMetricsHook<'a> = Option<&'a ConfigMetricsCollector>;

fn record_access(hook: ConfigMetricsHook<'_>) {
    if let Some(metrics) = hook {
        metrics.record_access();
    }
}

pub fn merge_timeout_with_defaults(
    user: &TimeoutConfig,
    config_metrics: ConfigMetricsHook,
) -> TimeoutConfig {
    record_access(config_metrics);
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

fn merge_collector_with_defaults(
    cfg: Option<&MetricCollectorConfig>,
) -> Option<MetricCollectorConfig> {
    cfg.map(|c| MetricCollectorConfig {
        buffer_size: c.buffer_size.or(Some(100)),
        flush_interval: c.flush_interval.or(Some(5000)),
        enable_periodic_reporting: c.enable_periodic_reporting.or(Some(false)),
        reporting_interval: c.reporting_interval.or(Some(10000)),
        max_age: c.max_age.or(Some(3600000)),
    })
}

pub fn merge_metrics_with_defaults(
    user: &MetricsConfig,
    config_metrics: ConfigMetricsHook,
) -> MetricsConfig {
    record_access(config_metrics);
    MetricsConfig {
        enabled: user.enabled.or(Some(true)),
        reporting_interval: user.reporting_interval.or(Some(10000)),
        enable_periodic_reporting: user.enable_periodic_reporting.or(Some(false)),
        workflow_metrics: merge_collector_with_defaults(user.workflow_metrics.as_ref()),
        node_metrics: merge_collector_with_defaults(user.node_metrics.as_ref()),
        agent_metrics: merge_collector_with_defaults(user.agent_metrics.as_ref()),
        event_metrics: merge_collector_with_defaults(user.event_metrics.as_ref()),
        tool_metrics: merge_collector_with_defaults(user.tool_metrics.as_ref()),
        token_metrics: merge_collector_with_defaults(user.token_metrics.as_ref()),
        template_metrics: merge_collector_with_defaults(user.template_metrics.as_ref()),
        config_metrics: merge_collector_with_defaults(user.config_metrics.as_ref()),
        error_metrics: merge_collector_with_defaults(user.error_metrics.as_ref()),
        resource_metrics: merge_collector_with_defaults(user.resource_metrics.as_ref()),
        agent_loop_metrics: merge_collector_with_defaults(user.agent_loop_metrics.as_ref()),
        subgraph_metrics: merge_collector_with_defaults(user.subgraph_metrics.as_ref()),
        http_addr: user.http_addr.clone(),
    }
}

pub fn merge_output_with_defaults(
    user: &OutputConfig,
    config_metrics: ConfigMetricsHook,
) -> OutputConfig {
    record_access(config_metrics);
    user.clone()
}

pub fn merge_storage_with_defaults(
    user: &StorageConfig,
    config_metrics: ConfigMetricsHook,
) -> StorageConfig {
    record_access(config_metrics);
    user.clone()
}

pub fn merge_sandbox_with_defaults(
    user: &SandboxConfig,
    config_metrics: ConfigMetricsHook,
) -> SandboxConfig {
    record_access(config_metrics);
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
        let merged = merge_timeout_with_defaults(&user, None);
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
        let merged = merge_metrics_with_defaults(&user, None);
        assert_eq!(merged.enabled, Some(true));
        assert_eq!(merged.reporting_interval, Some(10000));
        assert_eq!(merged.enable_periodic_reporting, Some(false));
        assert_eq!(merged.workflow_metrics, None);
    }

    #[test]
    fn test_merge_metrics_fills_collector_defaults() {
        let user = MetricsConfig {
            workflow_metrics: Some(MetricCollectorConfig {
                buffer_size: Some(50),
                ..Default::default()
            }),
            token_metrics: Some(MetricCollectorConfig::default()),
            ..Default::default()
        };
        let merged = merge_metrics_with_defaults(&user, None);
        let workflow = merged.workflow_metrics.unwrap();
        assert_eq!(workflow.buffer_size, Some(50));
        assert_eq!(workflow.flush_interval, Some(5000));
        assert_eq!(workflow.reporting_interval, Some(10000));
        assert_eq!(workflow.max_age, Some(3600000));
        let token = merged.token_metrics.unwrap();
        assert_eq!(token.buffer_size, Some(100));
        assert_eq!(token.flush_interval, Some(5000));
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
        let merged = merge_output_with_defaults(&user, None);
        assert_eq!(merged.dir, "/logs");
        assert_eq!(
            merged.sdk_log_level,
            wf_types::config::output::SdkLogLevel::Info
        );
    }

    #[test]
    fn test_merge_storage_with_defaults() {
        let user = StorageConfig {
            storage_type: wf_types::config::storage::StorageType::Sqlite,
            sqlite: None,
            postgres: None,
        };
        let merged = merge_storage_with_defaults(&user, None);
        assert_eq!(
            merged.storage_type,
            wf_types::config::storage::StorageType::Sqlite
        );
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
        let merged = merge_sandbox_with_defaults(&user, None);
        assert_eq!(merged.mode, Some(SandboxMode::Lenient));
        assert_eq!(merged.allowed_paths, Some(vec!["/tmp".to_string()]));
        assert_eq!(merged.resource_limits.as_ref().unwrap().memory, Some(512));
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
