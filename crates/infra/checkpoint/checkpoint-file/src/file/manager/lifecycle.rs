//! Manager construction and configuration: opening storage, binding a
//! workspace's bare repository, clock/event wiring and shared-state accessors.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use dashmap::DashMap;

use crate::event::CheckpointEventBus;
use crate::manager_store::{ManagerPolicy, ManagerStore};
use crate::scan::ScanConfig;
use crate::storage::{MetadataStore, SqliteStorage};
use checkpoint_base::actor::cache::ActorCache;
use checkpoint_base::clock::CheckpointClock;
use checkpoint_base::error::CheckpointError;

use super::FileCheckpointManager;

impl FileCheckpointManager {
    /// Drive checkpoint creation timestamps from an explicit clock instead
    /// of the system clock. Also swaps the write-attribution registry onto
    /// the same clock so window tests advance a single time source.
    pub fn with_clock(mut self, clock: CheckpointClock) -> Self {
        self.clock = clock;
        self
    }

    /// Attach the change-event bus: every recorded agent/manual edit
    /// publishes a `CheckpointEvent::FileChanged` carrying the snapshot id,
    /// file path and source label.
    pub fn with_event_bus(mut self, bus: CheckpointEventBus) -> Self {
        self.event_bus = Some(bus);
        self
    }

    /// The attached change-event bus, if any.
    pub fn event_bus(&self) -> Option<&CheckpointEventBus> {
        self.event_bus.as_ref()
    }

    /// Open a manager from the file-checkpoint storage config (the
    /// bootstrap entry point for `wf-runtime`).
    pub fn open(
        config: &wf_types::config::file_checkpoint::FileCheckpointStorageConfig,
    ) -> Result<Self, CheckpointError> {
        let storage = match &config.db_path {
            Some(path) => SqliteStorage::new_full(Path::new(path)),
            None => SqliteStorage::new_full_in_memory(),
        }?;
        Ok(Self {
            store: ManagerStore::with_sqlite(Arc::new(storage)),
            policy: ManagerPolicy::default(),
            clock: CheckpointClock::system(),
            event_bus: None,
            workspace_root: None,
            actor_index: ActorCache::new(),
            session_scopes: Arc::new(crate::scope::SessionScopeRegistry::default()),
            pending_batches: Arc::new(DashMap::new()),
            redo_stacks: Arc::new(DashMap::new()),
            checkpoint_metrics: Arc::new(std::sync::Mutex::new(None)),
        })
    }

    /// Open a manager from the full file-checkpoint config: storage backend
    /// plus the workspace context (workspace root, ignore patterns and
    /// per-file failure behavior) used by scoped captures (script diff /
    /// manual watcher).
    pub fn open_from_config(
        config: &wf_types::config::file_checkpoint::FileCheckpointConfig,
    ) -> Result<Self, CheckpointError> {
        let mut manager = match &config.storage {
            Some(storage) => Self::open(storage)?,
            None => Self::new_in_memory()?,
        };
        manager.workspace_root = config.workspace_root.as_ref().map(PathBuf::from);
        manager.policy.scan_config = ScanConfig {
            custom_ignore_patterns: config.custom_ignore_patterns.clone().unwrap_or_default(),
            failure_behavior: config.failure_behavior,
        };
        manager.policy.approval_policy = config.approval_policy;
        manager.policy.conflict_behavior = config.conflict_behavior;
        manager.policy.gc_interval_secs = config.gc_interval_secs;
        manager.policy.gc_retention = config.gc_retention.map(|r| crate::gc::GcRetention {
            keep_recent_heads: r.keep_recent_heads,
        });
        manager.check_workspace_root_binding(config)?;
        if let Some(root) = &config.workspace_root {
            manager.bind_git_for_workspace(Path::new(root))?;
        }
        Ok(manager)
    }

    /// Guard against opening a persistent DB with a workspace root that
    /// differs from the one recorded when the DB was first opened (catches
    /// a wrong `db_path` for a workspace). The normalized workspace root is
    /// stored in DB metadata on first open; later opens compare against it
    /// and fail on mismatch. In-memory stores and configs without a
    /// workspace root (legacy single-workspace) are not bound.
    fn check_workspace_root_binding(
        &self,
        config: &wf_types::config::file_checkpoint::FileCheckpointConfig,
    ) -> Result<(), CheckpointError> {
        let (Some(storage_cfg), Some(root)) = (&config.storage, &config.workspace_root) else {
            return Ok(());
        };
        if storage_cfg.db_path.is_none() {
            return Ok(());
        }
        let normalized = crate::file::util::normalize_workspace_key(Path::new(root));
        let storage = self.storage_ref()?;
        match storage.load_metadata(checkpoint_base::metadata::keys::WORKSPACE_ROOT_KEY)? {
            Some(existing) if existing != normalized => Err(CheckpointError::Validation {
                reason: format!(
                    "db_path is bound to workspace root '{existing}', cannot open with '{normalized}'"
                ),
            }),
            Some(_) => Ok(()),
            None => storage.store_metadata(
                checkpoint_base::metadata::keys::WORKSPACE_ROOT_KEY,
                &normalized,
            ),
        }
    }

    /// In-memory backend for tests and tooling. Storage is created here (the
    /// coordinator entry point) and injected downward into the store.
    pub fn new_in_memory() -> Result<Self, CheckpointError> {
        Ok(Self {
            store: ManagerStore::new_in_memory_backend(Arc::new(
                SqliteStorage::new_full_in_memory()?,
            ))?,
            policy: ManagerPolicy::default(),
            clock: CheckpointClock::system(),
            event_bus: None,
            workspace_root: None,
            actor_index: ActorCache::new(),
            session_scopes: Arc::new(crate::scope::SessionScopeRegistry::default()),
            pending_batches: Arc::new(DashMap::new()),
            redo_stacks: Arc::new(DashMap::new()),
            checkpoint_metrics: Arc::new(std::sync::Mutex::new(None)),
        })
    }

    /// Current creation timestamp from the manager clock, or an explicit
    /// error when the clock is unavailable. Checkpoint creation never falls
    /// back to a sentinel timestamp.
    pub(crate) fn creation_timestamp(&self) -> Result<i64, CheckpointError> {
        self.clock.now_ms().ok_or_else(|| {
            CheckpointError::Internal(
                "checkpoint clock unavailable; refusing to stamp a checkpoint".to_string(),
            )
        })
    }

    /// Shared scoped-shell sampling registry (foreground scopes +
    /// background sessions). Cloned sessions observe the same state.
    pub(crate) fn session_scopes(&self) -> Arc<crate::scope::SessionScopeRegistry> {
        self.session_scopes.clone()
    }

    pub(crate) fn storage_ref(&self) -> Result<&SqliteStorage, CheckpointError> {
        self.store.storage_ref()
    }

    /// Independent bare Git object store for this workspace. Fails with an
    /// explicit uninitialized error when no workspace is bound; Git
    /// operations never silently fall back to the legacy content tables.
    pub(crate) fn git_ref(&self) -> Result<&crate::git_store::GitStore, CheckpointError> {
        self.store.git_ref()
    }

    /// Bind (creating if needed) the bare repository for a workspace root.
    pub fn bind_git_for_workspace(&mut self, workspace_root: &Path) -> Result<(), CheckpointError> {
        let git = crate::git_store::GitStore::init_for_workspace(workspace_root).map_err(|e| {
            CheckpointError::Internal(format!("failed to init checkpoint git store: {e}"))
        })?;
        self.store.git = Some(Arc::new(git));
        Ok(())
    }
}
