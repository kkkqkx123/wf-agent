use std::time::Duration;
use tokio::time::timeout;

use crate::error::{PluginError, PluginResult};

pub struct PluginGuard {
    timeout: Duration,
}

impl PluginGuard {
    pub fn new(timeout_ms: u64) -> Self {
        Self { timeout: Duration::from_millis(timeout_ms) }
    }

    pub async fn execute<F, T>(&self, plugin_id: &str, f: F) -> PluginResult<T>
    where
        F: std::future::Future<Output = PluginResult<T>> + Send,
    {
        timeout(self.timeout, f)
            .await
            .map_err(|_| PluginError::Timeout { plugin_id: plugin_id.to_owned() })?
    }
}

impl Default for PluginGuard {
    fn default() -> Self { Self::new(10000) }
}
