use wf_config::processor::infrastructure::merge_metrics_with_defaults;
use wf_types::config::metrics::MetricsConfig;
use wf_types::config::output::OutputConfig;
use wf_types::config::storage::StorageConfig;
use wf_types::config::timeout::TimeoutConfig;

#[derive(Debug, Clone, Default)]
pub struct SdkOptions {
    pub storage: Option<StorageConfig>,
    pub timeout: Option<TimeoutConfig>,
    pub metrics: Option<MetricsConfig>,
    pub output: Option<OutputConfig>,
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

    pub fn with_graceful_shutdown_timeout(mut self, timeout_ms: u64) -> Self {
        self.graceful_shutdown_timeout = Some(timeout_ms);
        self
    }

    pub fn with_recovery(mut self, enable: bool) -> Self {
        self.enable_recovery = Some(enable);
        self
    }

    /// Merged metrics configuration, or `None` when no metrics config was set.
    pub fn metrics_config(&self) -> Option<MetricsConfig> {
        self.metrics.as_ref().map(merge_metrics_with_defaults)
    }

    /// Whether the metrics system should be initialized (default: enabled
    /// when a metrics config is present).
    pub fn metrics_enabled(&self) -> bool {
        self.metrics_config()
            .map(|c| c.enabled.unwrap_or(false))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::config::metrics::MetricCollectorConfig;

    #[test]
    fn metrics_disabled_without_config() {
        let options = SdkOptions::new();
        assert!(!options.metrics_enabled());
        assert!(options.metrics_config().is_none());
    }

    #[test]
    fn metrics_enabled_by_default_with_config() {
        let options = SdkOptions::new().with_metrics(MetricsConfig::default());
        assert!(options.metrics_enabled());
        let merged = options.metrics_config().unwrap();
        assert_eq!(merged.enabled, Some(true));
        assert_eq!(merged.reporting_interval, Some(10000));
    }

    #[test]
    fn metrics_explicitly_disabled() {
        let options = SdkOptions::new().with_metrics(MetricsConfig {
            enabled: Some(false),
            ..Default::default()
        });
        assert!(!options.metrics_enabled());
    }

    #[test]
    fn metrics_config_merges_collector_defaults() {
        let options = SdkOptions::new().with_metrics(MetricsConfig {
            workflow_metrics: Some(MetricCollectorConfig {
                flush_interval: Some(1000),
                ..Default::default()
            }),
            ..Default::default()
        });
        let merged = options.metrics_config().unwrap();
        let workflow = merged.workflow_metrics.unwrap();
        assert_eq!(workflow.flush_interval, Some(1000));
        assert_eq!(workflow.buffer_size, Some(100));
        assert_eq!(workflow.reporting_interval, Some(10000));
        assert_eq!(workflow.max_age, Some(3600000));
    }
}
