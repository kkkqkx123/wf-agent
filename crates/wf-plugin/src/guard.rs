use std::time::Duration;

use futures::FutureExt;
use tokio::time::timeout;

use crate::error::{PluginError, PluginResult};

/// Plugin Guard - Provides stability guarantees for plugin execution.
///
/// This is NOT a security sandbox - plugins are considered trusted.
/// Responsibilities:
/// - Timeout enforcement (prevents infinite loops from blocking the engine)
/// - Panic isolation (a panicking plugin hook does not tear down the engine)
pub struct PluginGuard {
    timeout: Duration,
}

impl PluginGuard {
    pub fn new(timeout_ms: u64) -> Self {
        Self {
            timeout: Duration::from_millis(timeout_ms),
        }
    }

    /// Execute a plugin hook with timeout and panic isolation: a panic inside
    /// the future is caught and reported as [`PluginError::PluginPanic`]
    /// instead of propagating to the engine.
    pub async fn execute<F, T>(&self, plugin_id: &str, f: F) -> PluginResult<T>
    where
        F: std::future::Future<Output = PluginResult<T>> + Send,
    {
        let guarded = async {
            if self.timeout.as_millis() > 0 {
                timeout(self.timeout, f)
                    .await
                    .map_err(|_| PluginError::Timeout {
                        plugin_id: plugin_id.to_owned(),
                    })?
            } else {
                f.await
            }
        };
        match std::panic::AssertUnwindSafe(guarded).catch_unwind().await {
            Ok(result) => result,
            Err(_panic) => {
                tracing::error!(plugin_id, "plugin hook panicked; recovered by PluginGuard");
                Err(PluginError::PluginPanic {
                    plugin_id: plugin_id.to_owned(),
                })
            }
        }
    }
}

impl Default for PluginGuard {
    fn default() -> Self {
        Self::new(10000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn ok_future() -> PluginResult<u32> {
        Ok(42)
    }

    async fn error_future() -> PluginResult<u32> {
        Err(PluginError::Internal("boom".into()))
    }

    async fn panicking_future() -> PluginResult<u32> {
        panic!("plugin bug");
    }

    async fn slow_future() -> PluginResult<u32> {
        tokio::time::sleep(Duration::from_millis(200)).await;
        Ok(1)
    }

    #[tokio::test]
    async fn successful_execution_passes_through() {
        let guard = PluginGuard::new(1000);
        assert_eq!(guard.execute("p1", ok_future()).await.unwrap(), 42);
    }

    #[tokio::test]
    async fn plugin_errors_are_propagated() {
        let guard = PluginGuard::new(1000);
        assert!(matches!(
            guard.execute("p1", error_future()).await,
            Err(PluginError::Internal(_))
        ));
    }

    #[tokio::test]
    async fn panic_is_isolated_as_plugin_error() {
        let guard = PluginGuard::new(1000);
        assert!(matches!(
            guard.execute("p1", panicking_future()).await,
            Err(PluginError::PluginPanic { .. })
        ));
    }

    #[tokio::test]
    async fn timeout_is_reported_when_configured() {
        let guard = PluginGuard::new(50);
        assert!(matches!(
            guard.execute("p1", slow_future()).await,
            Err(PluginError::Timeout { .. })
        ));
    }

    #[tokio::test]
    async fn zero_timeout_disables_the_guard() {
        let guard = PluginGuard::new(0);
        assert_eq!(guard.execute("p1", ok_future()).await.unwrap(), 42);
    }
}
