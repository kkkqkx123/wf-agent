//! Background health monitoring: the maintenance loop that health-checks
//! keep-alive servers, disconnects idle servers and the monitor facade
//! over those operations.

use std::sync::Arc;
use std::time::Duration;

use crate::error::ToolResult;

use super::connection::McpConnectionManager;
use super::registry::McpLifecycleMode;

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

    pub(super) fn spawn_health_check_loop(&self, server_name: String, interval: Duration) {
        let manager = self.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                let entry = manager.registry.get(&server_name);
                let keep_alive = entry
                    .map(|e| e.lifecycle == McpLifecycleMode::KeepAlive)
                    .unwrap_or(false);
                if !keep_alive {
                    break;
                }
                if manager.clients.contains_key(&server_name) {
                    let healthy = manager.check_health(&server_name).await.unwrap_or(false);
                    if !healthy {
                        let _ = manager.reconnect(&server_name).await;
                    }
                }
            }
        });
    }

    pub(super) fn spawn_idle_watcher(&self, server_name: String, idle_timeout: Duration) {
        let manager = self.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(idle_timeout);
            loop {
                ticker.tick().await;
                let entry = manager.registry.get(&server_name);
                let still_eager = entry
                    .map(|e| e.lifecycle == McpLifecycleMode::Eager)
                    .unwrap_or(false);
                if !still_eager {
                    break;
                }
                let idle_since = manager.last_activity.get(&server_name).map(|a| *a.value());
                if let Some(since) = idle_since {
                    if since.elapsed() >= idle_timeout {
                        let _ = manager.disconnect(&server_name).await;
                        break;
                    }
                }
            }
        });
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
