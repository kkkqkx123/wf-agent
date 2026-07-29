#[derive(Debug, Clone)]
pub struct PersistenceConfig {
    pub auto_flush_interval_ms: Option<u64>,
    pub buffer_size: Option<usize>,
    pub enable_compression: bool,
}

impl Default for PersistenceConfig {
    fn default() -> Self {
        Self {
            auto_flush_interval_ms: Some(5000),
            buffer_size: Some(100),
            enable_compression: true,
        }
    }
}

pub struct PersistenceLayer {
    config: PersistenceConfig,
}

impl PersistenceLayer {
    pub fn new(config: PersistenceConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &PersistenceConfig {
        &self.config
    }

    pub fn is_compression_enabled(&self) -> bool {
        self.config.enable_compression
    }

    pub fn auto_flush_interval(&self) -> Option<u64> {
        self.config.auto_flush_interval_ms
    }

    pub fn buffer_size(&self) -> Option<usize> {
        self.config.buffer_size
    }
}
