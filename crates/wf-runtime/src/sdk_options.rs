use wf_config::orchestrator::ConfigOverrides;
use wf_types::config::metrics::MetricsConfig;
use wf_types::config::output::OutputConfig;
use wf_types::config::storage::StorageConfig;
use wf_types::config::timeout::TimeoutConfig;
use wf_types::script::sandbox::SandboxConfig;

#[derive(Debug, Clone, Default)]
pub struct SdkOptions {
    pub storage: Option<StorageConfig>,
    pub timeout: Option<TimeoutConfig>,
    pub metrics: Option<MetricsConfig>,
    pub output: Option<OutputConfig>,
    pub sandbox: Option<SandboxConfig>,
    pub graceful_shutdown_timeout: Option<u64>,
    pub enable_recovery: Option<bool>,
    pub log_level: Option<String>,
}

impl SdkOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_storage(mut self, config: StorageConfig) -> Self {
        self.storage = Some(config);
        self
    }

    pub fn with_timeout(mut self, config: TimeoutConfig) -> Self {
        self.timeout = Some(config);
        self
    }

    pub fn with_metrics(mut self, config: MetricsConfig) -> Self {
        self.metrics = Some(config);
        self
    }

    pub fn with_output(mut self, config: OutputConfig) -> Self {
        self.output = Some(config);
        self
    }

    pub fn with_sandbox(mut self, config: SandboxConfig) -> Self {
        self.sandbox = Some(config);
        self
    }

    pub fn with_graceful_shutdown_timeout(mut self, timeout_ms: u64) -> Self {
        self.graceful_shutdown_timeout = Some(timeout_ms);
        self
    }

    pub fn with_recovery(mut self, enable: bool) -> Self {
        self.enable_recovery = Some(enable);
        self
    }

    /// Convert into `ConfigOverrides` for use with the orchestrator.
    pub fn into_overrides(self) -> ConfigOverrides {
        ConfigOverrides {
            storage: self.storage,
            timeout: self.timeout,
            metrics: self.metrics,
            output: self.output,
            sandbox: None, // SandboxConfig != SandboxGlobalConfig; keep separate
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_disabled_without_config() {
        let options = SdkOptions::new();
        assert!(options.metrics.is_none());
        let overrides = options.into_overrides();
        assert!(overrides.metrics.is_none());
    }

    #[test]
    fn into_overrides_preserves_fields() {
        let options = SdkOptions::new()
            .with_storage(StorageConfig {
                storage_type: wf_types::config::storage::StorageType::Sqlite,
                ..Default::default()
            })
            .with_timeout(TimeoutConfig {
                default: Some(99999),
                ..Default::default()
            })
            .with_metrics(MetricsConfig {
                enabled: Some(false),
                ..Default::default()
            });

        let overrides = options.into_overrides();
        assert!(overrides.storage.is_some());
        assert!(overrides.timeout.is_some());
        assert!(overrides.metrics.is_some());
        assert!(overrides.output.is_none());
    }
}
