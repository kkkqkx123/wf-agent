//! Shared maintenance entry: one pass health-checks keep-alive servers
//! and disconnects idle servers. Hosts drive it explicitly or through the
//! spawned loop; no per-server background task exists.

use std::sync::Arc;
use std::time::Duration;

use crate::error::ToolResult;

use super::connection::McpConnectionManager;

impl McpConnectionManager {
    /// One maintenance pass: health checks for keep-alive servers and idle
    /// disconnects for servers with an idle timeout.
    pub async fn run_maintenance(&self) {
        let _ = self.check_all().await;
        let _ = self.disconnect_idle().await;
    }

    /// Spawn the periodic maintenance loop (keep-alive health checks + idle
    /// disconnects). Runs until the returned handle is aborted.
    pub async fn start_maintenance_loop(
        self: Arc<Self>,
        interval: Duration,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                self.run_maintenance().await;
            }
        })
    }
}

/// Background health monitor over a connection manager.
pub struct McpHealthMonitor {
    manager: Arc<McpConnectionManager>,
}

impl McpHealthMonitor {
    pub fn new(manager: Arc<McpConnectionManager>) -> Self {
        Self { manager }
    }

    pub fn manager(&self) -> &Arc<McpConnectionManager> {
        &self.manager
    }

    pub fn record_activity(&self, server_name: &str) {
        self.manager.record_activity(server_name);
    }

    pub async fn check_health(&self, server_name: &str) -> ToolResult<bool> {
        self.manager.check_health(server_name).await
    }

    pub async fn check_all(&self) -> Vec<(String, bool)> {
        self.manager.check_all().await
    }

    pub async fn disconnect_idle(&self) -> Vec<String> {
        // Idle handling is driven by per-server configured timeouts inside
        // the manager; kept for API compatibility with the previous design.
        self.manager.disconnect_idle().await
    }

    pub async fn reconnect(&self, server_name: &str) -> ToolResult<()> {
        self.manager.reconnect(server_name).await
    }

    pub async fn start_health_check_loop(self: Arc<Self>, config: McpHealthCheckConfig) {
        let manager = self.manager.clone();
        let mut interval = tokio::time::interval(config.interval);
        loop {
            interval.tick().await;
            manager.run_maintenance().await;
        }
    }
}

pub struct McpHealthCheckConfig {
    pub interval: Duration,
}

impl Default for McpHealthCheckConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(30),
        }
    }
}
