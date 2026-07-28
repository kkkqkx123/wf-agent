use wf_types::config::storage::StorageConfig;
use wf_types::config::timeout::TimeoutConfig;
use wf_types::config::metrics::MetricsConfig;
use wf_types::config::output::OutputConfig;

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
}
