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
        let storage = Arc::new(match &config.db_path {
            Some(path) => SqliteStorage::new_full(Path::new(path)),
            None => SqliteStorage::new_full_in_memory(),
        }?);
        Ok(Self {
            store: ManagerStore::with_sqlite(storage.clone()),
            policy: ManagerPolicy::default(),
            clock: CheckpointClock::system(),
            event_bus: None,
            workspace_root: None,
            default_scope: None,
            actor_index: ActorCache::new(),
            session_scopes: Arc::new(crate::scope::SessionScopeRegistry::default()),
            pending_batches: Arc::new(DashMap::new()),
            redo_stacks: Arc::new(crate::file::redo_store::RedoStackStore::new(storage)?),
            checkpoint_metrics: Arc::new(std::sync::Mutex::new(None)),
        })
    }

    /// Open a manager from the full file-checkpoint config: storage backend
    /// plus the workspace context (workspace root, ignore patterns and
    /// per-file failure behavior) used by scoped captures (script diff /
    /// manual watcher). Without an explicit storage config a workspace-root
    /// binding allocates the metadata database in the global checkpoint
    /// data dir instead of falling back to an in-memory store, so history
    /// survives restarts.
    pub fn open_from_config(
        config: &wf_types::config::file_checkpoint::FileCheckpointConfig,
    ) -> Result<Self, CheckpointError> {
        let mut manager = match &config.storage {
            Some(storage) => {
                // A relative database path is anchored at the workspace root
                // so the open-time location matches the validation-time
                // containment promise; without a root it stays relative to
                // the process working directory as `open` documents.
                let effective = match (&storage.db_path, &config.workspace_root) {
                    (Some(db), Some(root))
                        if Path::new(db).is_relative() && !root.trim().is_empty() =>
                    {
                        wf_types::config::file_checkpoint::FileCheckpointStorageConfig {
                            storage_type: storage.storage_type.clone(),
                            db_path: Some(Path::new(root).join(db).to_string_lossy().into_owned()),
                        }
                    }
                    _ => storage.clone(),
                };
                Self::open(&effective)?
            }
            None => Self::open(&default_storage_config(
                &config.workspace_root,
                config.store_in_workspace.unwrap_or(false),
            )?)?,
        };
        manager.workspace_root = config.workspace_root.as_ref().map(PathBuf::from);
        manager.default_scope = config.default_scope.clone().or_else(|| {
            config.workspace_root.clone().map(|root| {
                wf_types::config::file_scope::ExecutionFileScope {
                    enabled: Some(true),
                    read_prefixes: Some(vec![root.clone()]),
                    write_prefixes: Some(vec![root.clone()]),
                    checkpoint_prefixes: Some(vec![root]),
                    workdir: None,
                }
            })
        });
        manager.policy.scan_config = ScanConfig {
            custom_ignore_patterns: config.custom_ignore_patterns.clone().unwrap_or_default(),
            failure_behavior: config.failure_behavior,
            max_file_size: config
                .max_file_size
                .unwrap_or(crate::scan::DEFAULT_MAX_FILE_SIZE),
        };
        manager.policy.approval_policy = config.approval_policy;
        manager.policy.conflict_behavior = config.conflict_behavior;
        manager.policy.gc_interval_secs = config.gc_interval_secs;
        manager.policy.gc_retention = config.gc_retention.map(|r| crate::gc::GcRetention {
            keep_recent_heads: r.keep_recent_heads,
        });
        manager.policy.sync = config.sync.clone();
        manager.policy.store_in_workspace = config.store_in_workspace.unwrap_or(false);
        manager.policy.baseline_mode = config
            .baseline
            .as_ref()
            .map(|baseline| baseline.mode);
        manager
            .session_scopes
            .set_scope_serialization(config.scope_serialization.as_ref());
        manager.check_workspace_root_binding(config)?;
        if let Some(root) = &config.workspace_root {
            let in_workspace = config.store_in_workspace.unwrap_or(false);
            manager.bind_git_for_workspace(Path::new(root), in_workspace)?;
            manager.seed_baseline_from_config(config)?;
        }
        Ok(manager)
    }

    /// Bind a persistent DB to the current workspace root. The normalized
    /// root is stored in DB metadata on first open; later opens with a
    /// different root fail closed so two workspaces never share one
    /// database. In-memory stores and configs without a workspace root are
    /// not bound.
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
                    "checkpoint database is bound to workspace '{existing}', refusing different root '{normalized}'"
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
        let storage = Arc::new(SqliteStorage::new_full_in_memory()?);
        Ok(Self {
            store: ManagerStore::new_in_memory_backend(storage.clone())?,
            policy: ManagerPolicy::default(),
            clock: CheckpointClock::system(),
            event_bus: None,
            workspace_root: None,
            default_scope: None,
            actor_index: ActorCache::new(),
            session_scopes: Arc::new(crate::scope::SessionScopeRegistry::default()),
            pending_batches: Arc::new(DashMap::new()),
            redo_stacks: Arc::new(crate::file::redo_store::RedoStackStore::new(storage)?),
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
    /// `in_workspace` keeps the legacy `<workspace>/.wf-checkpoint-git`
    /// layout; the default resolves into the global checkpoint data dir.
    pub fn bind_git_for_workspace(
        &mut self,
        workspace_root: &Path,
        in_workspace: bool,
    ) -> Result<(), CheckpointError> {
        let git = if in_workspace {
            crate::git_store::GitStore::init_in_workspace(workspace_root)
        } else {
            crate::git_store::GitStore::init_for_workspace(workspace_root)
        }
        .map_err(|e| {
            CheckpointError::Internal(format!("failed to init checkpoint git store: {e}"))
        })?;
        self.store.git = Some(Arc::new(git));
        Ok(())
    }

    /// Seed the baseline once for a fresh store when configured, then
    /// validate the pinned anchor. Failures follow `failure_behavior`:
    /// error blocks opening, warn logs and continues without a baseline,
    /// ignore continues silently.
    fn seed_baseline_from_config(
        &self,
        config: &wf_types::config::file_checkpoint::FileCheckpointConfig,
    ) -> Result<(), CheckpointError> {
        let Some(baseline) = &config.baseline else {
            return Ok(());
        };
        if config.store_in_workspace.unwrap_or(false)
            && baseline.mode == wf_types::config::file_checkpoint::BaselineMode::Borrow
        {
            tracing::warn!(
                "checkpoint store lives inside the workspace but baseline mode is borrow: \
                 the delivered directory still depends on the external repository; \
                 use copy mode for a self-contained delivery"
            );
        }
        let failure_behavior = baseline
            .failure_behavior
            .unwrap_or(wf_types::config::file_checkpoint::FailureBehavior::Error);
        let git = self.git_ref()?;
        let request = crate::git_store::BaselineRequest {
            mode: match baseline.mode {
                wf_types::config::file_checkpoint::BaselineMode::Borrow => {
                    crate::git_store::BaselineMode::Borrow
                }
                wf_types::config::file_checkpoint::BaselineMode::Copy => {
                    crate::git_store::BaselineMode::Copy
                }
            },
            branch: baseline.branch.clone(),
            commit_prefix: baseline.commit_prefix.clone(),
            behind: baseline.behind,
        };
        match git.ensure_baseline(&request) {
            Ok(crate::git_store::BaselineSeedOutcome::Seeded(anchor)) => {
                tracing::info!(
                    anchor = %anchor.id,
                    spec = %anchor.source_spec,
                    "checkpoint baseline seeded"
                );
            }
            Ok(crate::git_store::BaselineSeedOutcome::AlreadySeeded(_)) => {}
            Ok(crate::git_store::BaselineSeedOutcome::Skipped(reason)) => {
                tracing::debug!(reason = %reason, "checkpoint baseline seeding skipped");
            }
            Err(e) => {
                Self::apply_baseline_failure_policy(
                    failure_behavior,
                    format!("checkpoint baseline unavailable: {e}"),
                )?;
            }
        }
        match git.validate_baseline() {
            Ok(crate::git_store::BaselineStatus::Degraded { reason, .. }) => {
                Self::apply_baseline_failure_policy(
                    failure_behavior,
                    format!("checkpoint baseline degraded: {reason}"),
                )?;
            }
            Ok(_) => {}
            Err(e) => {
                Self::apply_baseline_failure_policy(
                    failure_behavior,
                    format!("checkpoint baseline validation failed: {e}"),
                )?;
            }
        }
        Ok(())
    }

    /// Current baseline health, if a workspace git store is bound.
    pub fn baseline_status(&self) -> Result<crate::git_store::BaselineStatus, CheckpointError> {
        let git = self.git_ref()?;
        git.validate_baseline().map_err(crate::file::git_write::map_git_error)
    }

    fn apply_baseline_failure_policy(
        failure_behavior: wf_types::config::file_checkpoint::FailureBehavior,
        context: String,
    ) -> Result<(), CheckpointError> {
        match failure_behavior {
            wf_types::config::file_checkpoint::FailureBehavior::Error => {
                Err(CheckpointError::Validation { reason: context })
            }
            wf_types::config::file_checkpoint::FailureBehavior::Warn => {
                tracing::warn!("{context}");
                Ok(())
            }
            wf_types::config::file_checkpoint::FailureBehavior::Ignore => Ok(()),
        }
    }
}

/// Storage config used when the user configures none: the SQLite metadata
/// database lives beside the bare repository — in the global checkpoint
/// store dir for the workspace, or in the legacy in-workspace layout when
/// `in_workspace` is set. Without a workspace root there is nothing to key
/// the store on, so the in-memory fallback applies.
fn default_storage_config(
    workspace_root: &Option<String>,
    in_workspace: bool,
) -> Result<wf_types::config::file_checkpoint::FileCheckpointStorageConfig, CheckpointError> {
    let Some(root) = workspace_root else {
        return Ok(
            wf_types::config::file_checkpoint::FileCheckpointStorageConfig {
                storage_type: wf_types::config::file_checkpoint::FileCheckpointStorageType::Sqlite,
                db_path: None,
            },
        );
    };
    let store_dir = if in_workspace {
        Path::new(root).join(crate::git_store::CHECKPOINT_GIT_DIR_NAME)
    } else {
        crate::store_path::global_store_dir(Path::new(root))
    };
    std::fs::create_dir_all(&store_dir).map_err(CheckpointError::Io)?;
    Ok(
        wf_types::config::file_checkpoint::FileCheckpointStorageConfig {
            storage_type: wf_types::config::file_checkpoint::FileCheckpointStorageType::Sqlite,
            db_path: Some(store_dir.join("meta.db").to_string_lossy().into_owned()),
        },
    )
}
