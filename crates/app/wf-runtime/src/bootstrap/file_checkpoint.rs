use std::sync::Arc;

use tracing::info;

use wf_types::config::file_checkpoint::FileCheckpointConfig;

use crate::error::RuntimeResult;

pub fn init_file_checkpoint_manager(
    config: &FileCheckpointConfig,
    event_bus: Arc<wf_core::event::EventBus>,
) -> RuntimeResult<(
    Option<checkpoint_file::file::FileCheckpointManager>,
    Option<tokio::task::JoinHandle<()>>,
)> {
    if !config.enabled {
        return Ok((None, None));
    }
    match checkpoint_file::file::FileCheckpointManager::open_from_config(config) {
        Ok(manager) => {
            info!("File checkpoint manager initialized (layertwine Sqlite)");
            let bus = wf_checkpoint::event::CheckpointEventBus::new();
            let handle = crate::checkpoint_event_bridge::spawn(event_bus, bus.clone());
            let manager = manager.with_event_bus(bus);
            Ok((Some(manager), Some(handle)))
        }
        Err(err) => Err(crate::error::RuntimeError::Config(format!(
            "Failed to initialize file checkpoint storage: {err}"
        ))),
    }
}

pub fn init_manual_change_service(
    config: &FileCheckpointConfig,
    manager: Option<&checkpoint_file::file::FileCheckpointManager>,
) -> RuntimeResult<Option<checkpoint_file::watcher::ManualChangeService>> {
    let Some(manager) = manager else {
        return Ok(None);
    };
    let Some(root) = config.workspace_root.as_deref() else {
        return Ok(None);
    };
    if !config.enabled || !config.manual_watch {
        return Ok(None);
    }
    let poll_ms = config.manual_poll_ms.unwrap_or(2000);
    match checkpoint_file::watcher::ManualChangeService::start(manager.clone(), root, poll_ms) {
        Ok(service) => {
            info!(root = %root, "Manual file watcher started");
            Ok(Some(service))
        }
        Err(err) => Err(crate::error::RuntimeError::Config(format!(
            "Failed to start the manual file watcher: {err}"
        ))),
    }
}

pub fn init_gc_timer(
    config: &FileCheckpointConfig,
    manager: Option<&checkpoint_file::file::FileCheckpointManager>,
) -> Option<tokio::task::JoinHandle<()>> {
    let interval_secs = config.gc_interval_secs?;
    if interval_secs == 0 {
        return None;
    }
    let manager = manager?.clone();
    let retention = config
        .gc_retention
        .map(|r| checkpoint_file::gc::GcRetention {
            keep_recent_heads: r.keep_recent_heads,
        })
        .unwrap_or_default();
    let interval = std::time::Duration::from_secs(interval_secs);
    info!(interval_secs, "Periodic GC timer started");
    Some(wf_common::spawn_ticker(
        interval,
        tokio_util::sync::CancellationToken::new(),
        move || {
            let manager = manager.clone();
            async move {
                match manager.run_gc(retention) {
                    Ok(stats) => {
                        info!(
                            removed_checkpoints = stats.removed_checkpoints,
                            removed_snapshots = stats.removed_snapshots,
                            "Periodic GC completed"
                        );
                    }
                    Err(err) => {
                        tracing::warn!("Periodic GC failed: {err}");
                    }
                }
            }
        },
    ))
}

/// Assembled file-checkpoint stack produced by [`init_file_checkpoint_stack`]:
/// the layertwine-backed manager plus its background tasks, kept alive for
/// the runtime lifetime.
pub struct FileCheckpointStack {
    pub manager: Option<checkpoint_file::file::FileCheckpointManager>,
    pub event_bridge_handle: Option<tokio::task::JoinHandle<()>>,
    pub manual_change_service: Option<checkpoint_file::watcher::ManualChangeService>,
    pub gc_timer_handle: Option<tokio::task::JoinHandle<()>>,
}

/// Bootstrap stage: build the file-checkpoint manager (workspace root +
/// scan rules) when enabled, start the manual watcher when a workspace root
/// with manual watching is configured, and arm the periodic GC timer. The
/// manager is attached to the API context so workflow/agent executions
/// create and restore file snapshots through it and script handlers capture
/// workspace changes.
pub fn init_file_checkpoint_stack(
    config: &FileCheckpointConfig,
    event_bus: Arc<wf_core::event::EventBus>,
) -> RuntimeResult<FileCheckpointStack> {
    let (manager, event_bridge_handle) = init_file_checkpoint_manager(config, event_bus)?;
    let manual_change_service = init_manual_change_service(config, manager.as_ref())?;
    let gc_timer_handle = init_gc_timer(config, manager.as_ref());
    Ok(FileCheckpointStack {
        manager,
        event_bridge_handle,
        manual_change_service,
        gc_timer_handle,
    })
}
