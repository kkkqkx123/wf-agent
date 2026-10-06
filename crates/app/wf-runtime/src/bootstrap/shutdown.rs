use tracing::info;

use super::runtime::Runtime;

/// Clears the process-wide active-shutdown marker when dropped. `Runtime::run`
/// and `Runtime::shutdown` own one for their whole body so the marker covers
/// every teardown step but does not leak into later runtimes in one process.
struct ActiveShutdownScope;

impl Drop for ActiveShutdownScope {
    fn drop(&mut self) {
        wf_common::shutdown::clear_active_shutdown();
    }
}

impl Runtime {
    pub async fn shutdown(mut self) -> crate::error::RuntimeResult<()> {
        // Mark the close as active before aborting driver tasks: executions
        // that settle during teardown are cancelled instead of recorded as
        // failed, so quitting never leaves spurious `Failed` executions or
        // dispatches behind. Cleared again when teardown finishes so later
        // runtimes in the same process are unaffected.
        wf_common::shutdown::begin_active_shutdown();
        let _shutdown_scope = ActiveShutdownScope;
        if let Some(metrics) = self.metrics.take() {
            metrics.shutdown().await;
        }

        if let Some(handle) = self.trigger_listener_handle.take() {
            if let Some(token) = self.trigger_listener_shutdown.take() {
                token.cancel();
            }
            let _ = tokio::time::timeout(std::time::Duration::from_secs(2), handle).await;
            info!("Trigger listener stopped");
        }

        // Stop the supervised code-context service after its consumers
        // (compression chain, retrieval tools) are down.
        if let Some(sidecar) = self.code_context_sidecar.take() {
            sidecar.shutdown().await;
            info!("Code-context sidecar stopped");
        }

        // Abort detached execution driver tasks (workflow `stream()` drivers,
        // callback forwarders) before the storage layer closes underneath
        // them.
        if let Some(ctx) = self.api_ctx.get() {
            ctx.shutdown();
        }

        // Flush buffered event persistence before the storage layer closes.
        if let Some(persistence) = self.event_persistence.take() {
            let _ = persistence.shutdown().await;
        }

        // Stop the manual file watcher before the storage layer closes.
        if let Some(mut service) = self.manual_change_service.take() {
            service.stop().await;
        }

        // Stop the checkpoint event bridge (it holds a broadcast receiver on
        // the checkpoint bus; aborting it is safe once the watcher stopped).
        if let Some(handle) = self.checkpoint_event_bridge_handle.take() {
            handle.abort();
        }

        // Stop the periodic GC timer before the storage layer closes.
        if let Some(handle) = self.gc_timer_handle.take() {
            handle.abort();
        }

        if let Some(manager) = self.mcp_manager.take() {
            let servers = manager.connected_servers();
            for server in servers {
                let _ = manager.disconnect(&server).await;
            }
            info!("MCP connections closed");
        }

        #[cfg(feature = "plugins")]
        if let Some(engine) = self.plugin_engine.take() {
            engine.shutdown().await;
        }

        let stats = self.agent_registry.gate_stats();
        info!(
            "Agent capacity gate final stats: active={}, available={}",
            stats.active_count, stats.available_permits
        );

        self.storage_manager.close().await?;
        info!("Runtime shutdown complete");
        Ok(())
    }
}
