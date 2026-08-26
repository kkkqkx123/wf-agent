use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use dashmap::DashMap;
use layertwine::core::file_node::FileNode;
use layertwine::core::snapshot::{Snapshot, SnapshotContent};
use layertwine::layered::StateMachine;
use layertwine::storage::repository::{MetadataStore, SnapshotStore};
use layertwine::storage::sqlite::SqliteStorage;
pub use wf_types::config::file_checkpoint::ApprovalPolicy;
use wf_types::config::file_checkpoint::{ConflictBehavior, FailureBehavior};

use crate::actor_id::ActorId;
use crate::diff::DiffEngine;
use crate::error::CheckpointError;
use crate::event::CheckpointEventBus;
use crate::file_util::{map_layertwine_error, sha256_hex};
use crate::layertwine::LayertwineGitAdapter;
use crate::provenance::{DeltaSummary, FileDiffView, PartitionView, WorkspaceFile};
use crate::recent_agent_writes::RecentAgentWrites;
use crate::scan::{ScanConfig, WorkspaceScanner};
use crate::watcher::{FileChangeKind, FileChangeRecord};

fn is_false(v: &bool) -> bool {
    !*v
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct FileState {
    pub path: String,
    pub hash: String,
    pub size: u64,
    pub last_modified: i64,
    /// Deletion projection marker: the actor partition
    /// stores an empty content for the path; `deleted = true` excludes the
    /// path from workspace restores so the file is removed from the
    /// workspace. Skipped when `false` to keep the historical JSON shape.
    #[serde(default, skip_serializing_if = "is_false")]
    pub deleted: bool,
}

fn default_checkpoint_type() -> String {
    "full".to_string()
}

/// Lightweight projection of a layertwine `Checkpoint` + `Partition`.
///
/// The authoritative model lives in layertwine (content-addressed, INSERT-
/// ONLY snapshots and partitions); this struct is the read model exposed to
/// coordinators / API consumers, keeping the historical field shape so
/// downstream code stays unchanged. Every checkpoint is a "full" projection
/// of the actor partition's latest per-file state — incremental storage is a
/// layertwine-internal concern (partition history).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct FileCheckpoint {
    /// layertwine checkpoint id (Blake3 hex).
    pub id: String,
    /// Checkpoint creation time (Unix milliseconds).
    pub timestamp: i64,
    pub full_hash: String,
    pub files: Vec<FileState>,
    /// Always `"full"` in the projection (layertwine owns incrementality).
    #[serde(default = "default_checkpoint_type")]
    pub checkpoint_type: String,
    /// Always `None` in the projection (no hand-written delta chains).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_checkpoint_id: Option<String>,
    /// Directories that contained no files at snapshot time; recreated on
    /// workspace restore. Kept in the projection index (not in layertwine).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub empty_dirs: Option<Vec<String>>,
}

/// Metadata for indexing and querying file checkpoints.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct FileCheckpointMetadata {
    pub id: String,
    pub entity_id: String,
    pub timestamp: i64,
    pub checkpoint_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_checkpoint_id: Option<String>,
    pub file_count: u64,
    pub full_hash: String,
    pub total_size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_fields: Option<HashMap<String, serde_json::Value>>,
}

impl From<&FileCheckpoint> for FileCheckpointMetadata {
    fn from(checkpoint: &FileCheckpoint) -> Self {
        Self {
            id: checkpoint.id.clone(),
            entity_id: String::new(),
            timestamp: checkpoint.timestamp,
            checkpoint_type: checkpoint.checkpoint_type.clone(),
            base_checkpoint_id: checkpoint.base_checkpoint_id.clone(),
            file_count: checkpoint.files.len() as u64,
            full_hash: checkpoint.full_hash.clone(),
            total_size: checkpoint.files.iter().map(|f| f.size).sum(),
            tags: None,
            custom_fields: None,
        }
    }
}

/// A file's path and full content, used by content-level file checkpointing.
#[derive(Debug, Clone)]
pub struct FileContentEntry {
    pub path: String,
    pub content: Vec<u8>,
}

impl FileContentEntry {
    pub fn new(path: impl Into<String>, content: Vec<u8>) -> Self {
        Self {
            path: path.into(),
            content,
        }
    }
}

/// Content-level storage for file rollback: bytes are stored
/// content-addressed (path + content hash), so unchanged content is never
/// duplicated across checkpoints and old bytes remain retrievable until
/// explicitly removed.
pub trait FileContentStore: Send + Sync {
    /// Persist one file's content. Returns the content hash (SHA-256 hex)
    /// which callers record in the checkpoint's [`FileState`].
    fn save_content(&self, path: &str, content: &[u8]) -> Result<String, CheckpointError>;

    /// Load a file's content by its recorded hash. `None` when no such
    /// content was stored (or it was removed).
    fn load_content(&self, path: &str, hash: &str) -> Result<Option<Vec<u8>>, CheckpointError>;

    /// Drop stored content for a path/hash pair. Content stores are
    /// immutable by design, so the default implementation is a no-op;
    /// explicit garbage collection removes unreferenced bytes.
    fn remove_content(&self, path: &str, hash: &str) -> Result<(), CheckpointError> {
        let _ = (path, hash);
        Ok(())
    }
}

/// Layertwine-backed [`FileContentStore`]: a thin wrapper over
/// `SnapshotContent::FileContent` snapshots stored in layertwine's Sqlite
/// storage (content-addressed, INSERT-ONLY). No `wf-file-content:` metadata
/// index is maintained — lookups scan the content-addressed snapshots by
/// path and match the recorded SHA-256 hash against the stored bytes.
pub struct LayertwineFileContentStore {
    storage: Arc<SqliteStorage>,
}

impl LayertwineFileContentStore {
    pub fn new_in_memory() -> Result<Self, CheckpointError> {
        let storage = Arc::new(SqliteStorage::new_full_in_memory().map_err(map_layertwine_error)?);
        Ok(Self { storage })
    }

    pub fn new(path: &Path) -> Result<Self, CheckpointError> {
        let storage = Arc::new(SqliteStorage::new_full(path).map_err(map_layertwine_error)?);
        Ok(Self { storage })
    }

    /// Share the underlying Sqlite connection (for test diagnostics).
    pub fn share(&self) -> Self {
        Self {
            storage: self.storage.clone(),
        }
    }
}

impl FileContentStore for LayertwineFileContentStore {
    fn save_content(&self, path: &str, content: &[u8]) -> Result<String, CheckpointError> {
        let hash = sha256_hex(content);
        let file_node = FileNode::new(PathBuf::from(path), content);
        let snapshot = Snapshot::new_with_content(
            file_node,
            SnapshotContent::FileContent(content.to_vec()),
            format!("file://{}", path),
            "file".to_string(),
            vec![],
            vec![],
        );
        self.storage
            .store_snapshot(&snapshot, content)
            .map_err(map_layertwine_error)?;
        Ok(hash)
    }

    fn load_content(&self, path: &str, hash: &str) -> Result<Option<Vec<u8>>, CheckpointError> {
        let snapshots = self
            .storage
            .find_snapshots_by_file(path)
            .map_err(map_layertwine_error)?;
        for snapshot in snapshots {
            if let Some(content) = snapshot.content {
                let bytes = content.to_bytes();
                if sha256_hex(&bytes) == hash {
                    return Ok(Some(bytes));
                }
            }
        }
        Ok(None)
    }

    fn remove_content(&self, _path: &str, _hash: &str) -> Result<(), CheckpointError> {
        // INSERT-ONLY storage: physical removal is a separate GC concern.
        Ok(())
    }
}

/// Options controlling checkpoint decisions and per-file error tolerance.
#[derive(Debug, Clone)]
pub struct FileCheckpointOptions {
    /// Per-file error handling during scan/restore.
    pub failure_behavior: FailureBehavior,
    /// Additional ignore patterns applied while scanning the workspace.
    pub custom_ignore_patterns: Vec<String>,
}

impl Default for FileCheckpointOptions {
    fn default() -> Self {
        Self {
            failure_behavior: FailureBehavior::Warn,
            custom_ignore_patterns: Vec::new(),
        }
    }
}

/// Result of a workspace-aligned restore.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WorkspaceRestoreResult {
    /// Files written back to disk.
    pub restored: usize,
    /// Extra files deleted from the workspace.
    pub deleted: usize,
    /// Files already matching the target state (skipped).
    pub skipped: usize,
}

/// File checkpoint engine rebuilt on top of layertwine's layered state
/// machine (authoritative model). The manager drives
/// `layertwine::layered::StateMachine<SqliteStorage>`: agent partitions hold
/// per-actor file edits (`apply_agent_edit`), checkpoint creation snapshots
/// the partition state into layertwine `Checkpoint`s, and restore goes
/// through `transition::reconstruct_text`. `FileCheckpoint` / `FileState`
/// are projections over the layertwine model.
pub struct FileCheckpointManager {
    pub(crate) storage: Option<Arc<SqliteStorage>>,
    pub(crate) state_machine: Option<StateMachine<SqliteStorage>>,
    pub(crate) branch_adapter: Arc<LayertwineGitAdapter>,
    /// Actor id -> latest checkpoint id (projection index; cheap in-memory
    /// mirror, the DB remains authoritative).
    pub(crate) latest_checkpoints: Arc<DashMap<String, String>>,
    /// Checkpoint id -> empty directories recorded at snapshot time
    /// (projection-only, not stored in layertwine).
    pub(crate) empty_dirs: Arc<DashMap<String, Vec<String>>>,
    /// Path -> content hash registry of recent agent writes (manual watcher
    /// uses it to distinguish agent self-writes from human edits).
    pub(crate) recent_agent_writes: Arc<RecentAgentWrites>,
    /// Actor id -> file paths deleted by the actor (write-side cache only).
    /// The authoritative deletion set is derived from the snapshot chain's
    /// explicit deletion markers (`checkpoint_deleted_paths`); this map is
    /// kept for cheap lookups and cleared/updated on writes.
    pub(crate) deleted_files: Arc<DashMap<String, HashSet<String>>>,
    /// Change-event feed: every recorded agent/manual edit publishes a
    /// `CheckpointEvent::FileChanged`. Absent when the manager is used
    /// without an event layer.
    pub(crate) event_bus: Option<CheckpointEventBus>,
    /// Workspace root the manager is bound to (from `FileCheckpointConfig`).
    /// Scoped captures (script diff, manual watcher) restrict their scope to
    /// this root; `None` disables them.
    pub(crate) workspace_root: Option<PathBuf>,
    /// Workspace scan rules (ignore patterns + per-file failure behavior).
    pub(crate) scan_config: ScanConfig,
    /// Layered approval policy applied when an execution ends
    /// (`on_agent_complete`); flows the actor partition through the approval
    /// layer before merging into a feature.
    pub(crate) approval_policy: ApprovalPolicy,
    /// Three-way merge conflict strategy applied by `approve_changes` /
    /// `merge_entity_changes` (marker / fail / approval).
    pub(crate) conflict_behavior: ConflictBehavior,
    /// Physical GC auto-run interval in seconds; `None` = never run
    /// automatically (from `FileCheckpointConfig.gc_interval_secs`).
    pub(crate) gc_interval_secs: Option<u64>,
    /// GC retention policy (from `FileCheckpointConfig.gc_retention`);
    /// `None` = default (only the built-in protected set).
    pub(crate) gc_retention: Option<layertwine::git_sync::GcRetention>,
    /// Entity id -> resolved `ActorId` (sub-execution isolation). Built at
    /// first actor resolution: a child execution whose parent is known in
    /// the index gets `parent.child(execution_id)`, so nested executions
    /// live in their own hierarchical partition.
    pub(crate) actor_index: Arc<DashMap<String, ActorId>>,
}

impl Clone for FileCheckpointManager {
    fn clone(&self) -> Self {
        Self {
            storage: self.storage.clone(),
            state_machine: self
                .storage
                .as_ref()
                .map(|storage| StateMachine::new(storage.clone())),
            branch_adapter: Arc::clone(&self.branch_adapter),
            latest_checkpoints: self.latest_checkpoints.clone(),
            empty_dirs: self.empty_dirs.clone(),
            recent_agent_writes: self.recent_agent_writes.clone(),
            deleted_files: self.deleted_files.clone(),
            event_bus: self.event_bus.clone(),
            workspace_root: self.workspace_root.clone(),
            scan_config: self.scan_config.clone(),
            approval_policy: self.approval_policy,
            conflict_behavior: self.conflict_behavior,
            gc_interval_secs: self.gc_interval_secs,
            gc_retention: self.gc_retention,
            actor_index: self.actor_index.clone(),
        }
    }
}

impl FileCheckpointManager {
    pub fn new() -> Self {
        let branch_adapter = Arc::new(
            LayertwineGitAdapter::new_in_memory().expect("in-memory adapter should not fail"),
        );
        Self {
            storage: None,
            state_machine: None,
            branch_adapter,
            latest_checkpoints: Arc::new(DashMap::new()),
            empty_dirs: Arc::new(DashMap::new()),
            recent_agent_writes: Arc::new(RecentAgentWrites::new()),
            deleted_files: Arc::new(DashMap::new()),
            event_bus: None,
            workspace_root: None,
            scan_config: ScanConfig::default(),
            approval_policy: ApprovalPolicy::default(),
            conflict_behavior: ConflictBehavior::default(),
            gc_interval_secs: None,
            gc_retention: None,
            actor_index: Arc::new(DashMap::new()),
        }
    }

    /// Attach a layertwine Sqlite backend; this is the production entry
    /// point (the storage is shared with the surrounding runtime).
    pub fn with_sqlite(storage: Arc<SqliteStorage>) -> Self {
        let state_machine = StateMachine::new(storage.clone());
        let branch_adapter = Arc::new(LayertwineGitAdapter::from_shared(storage.clone()));
        Self {
            storage: Some(storage),
            state_machine: Some(state_machine),
            branch_adapter,
            latest_checkpoints: Arc::new(DashMap::new()),
            empty_dirs: Arc::new(DashMap::new()),
            recent_agent_writes: Arc::new(RecentAgentWrites::new()),
            deleted_files: Arc::new(DashMap::new()),
            event_bus: None,
            workspace_root: None,
            scan_config: ScanConfig::default(),
            approval_policy: ApprovalPolicy::default(),
            conflict_behavior: ConflictBehavior::default(),
            gc_interval_secs: None,
            gc_retention: None,
            actor_index: Arc::new(DashMap::new()),
        }
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
    /// bootstrap entry point: `wf-runtime` builds the manager without
    /// depending on layertwine directly).
    pub fn open(
        config: &wf_types::config::file_checkpoint::FileCheckpointStorageConfig,
    ) -> Result<Self, CheckpointError> {
        let storage = match &config.db_path {
            Some(path) => SqliteStorage::new_full(Path::new(path)),
            None => SqliteStorage::new_full_in_memory(),
        }
        .map_err(map_layertwine_error)?;
        Ok(Self::with_sqlite(Arc::new(storage)))
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
        manager.scan_config = ScanConfig {
            custom_ignore_patterns: config.custom_ignore_patterns.clone().unwrap_or_default(),
            failure_behavior: config.failure_behavior,
        };
        manager.approval_policy = config.approval_policy;
        manager.conflict_behavior = config.conflict_behavior;
        manager.gc_interval_secs = config.gc_interval_secs;
        manager.gc_retention = config
            .gc_retention
            .map(|r| layertwine::git_sync::GcRetention {
                keep_recent_heads: r.keep_recent_heads,
            });
        manager.check_workspace_root_binding(config)?;
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
        const WS_ROOT_KEY: &str = "wf-checkpoint:workspace-root";
        let (Some(storage_cfg), Some(root)) = (&config.storage, &config.workspace_root) else {
            return Ok(());
        };
        if storage_cfg.db_path.is_none() {
            return Ok(());
        }
        let normalized = crate::file_util::normalize_workspace_key(Path::new(root));
        let storage = self.storage_ref()?;
        match storage.load_metadata(WS_ROOT_KEY).map_err(map_layertwine_error)? {
            Some(existing) if existing != normalized => Err(CheckpointError::Validation {
                reason: format!(
                    "db_path is bound to workspace root '{existing}', cannot open with '{normalized}'"
                ),
            }),
            Some(_) => Ok(()),
            None => storage
                .store_metadata(WS_ROOT_KEY, &normalized)
                .map_err(map_layertwine_error),
        }
    }

    /// In-memory backend for tests and tooling.
    pub fn new_in_memory() -> Result<Self, CheckpointError> {
        let storage = Arc::new(SqliteStorage::new_full_in_memory().map_err(map_layertwine_error)?);
        let branch_adapter = Arc::new(LayertwineGitAdapter::from_shared(storage.clone()));
        Ok(Self {
            storage: Some(storage),
            state_machine: None,
            branch_adapter,
            latest_checkpoints: Arc::new(DashMap::new()),
            empty_dirs: Arc::new(DashMap::new()),
            recent_agent_writes: Arc::new(RecentAgentWrites::new()),
            deleted_files: Arc::new(DashMap::new()),
            event_bus: None,
            workspace_root: None,
            scan_config: ScanConfig::default(),
            approval_policy: ApprovalPolicy::default(),
            conflict_behavior: ConflictBehavior::default(),
            gc_interval_secs: None,
            gc_retention: None,
            actor_index: Arc::new(DashMap::new()),
        })
    }

    pub fn state_machine(&self) -> Option<&StateMachine<SqliteStorage>> {
        self.state_machine.as_ref()
    }

    pub fn storage(&self) -> Option<&Arc<SqliteStorage>> {
        self.storage.as_ref()
    }

    pub(crate) fn storage_ref(&self) -> Result<&SqliteStorage, CheckpointError> {
        self.storage.as_deref().ok_or_else(|| {
            CheckpointError::Coordinator("no file checkpoint storage configured".to_string())
        })
    }

    // ── workspace checkpointing ─────────────────────────────────────

    /// Scan the workspace and create a content-level checkpoint from it
    /// (the default full-scan path): files are hashed and stored as agent
    /// edits, and empty directories are recorded for later restore.
    pub fn create_workspace_checkpoint(
        &self,
        entity_id: &str,
        base_dir: &Path,
        opts: &FileCheckpointOptions,
    ) -> Result<FileCheckpoint, CheckpointError> {
        let scanner = WorkspaceScanner::new(ScanConfig {
            custom_ignore_patterns: opts.custom_ignore_patterns.clone(),
            failure_behavior: opts.failure_behavior,
        });
        let scan = scanner.scan(base_dir)?;
        let mut entries = Vec::with_capacity(scan.files.len());
        for state in &scan.files {
            let path = base_dir.join(&state.path);
            match std::fs::read(&path) {
                Ok(content) => entries.push(FileContentEntry::new(state.path.clone(), content)),
                Err(err) => match opts.failure_behavior {
                    FailureBehavior::Error => {
                        return Err(CheckpointError::Io(std::io::Error::other(format!(
                            "failed to read '{}': {err}",
                            state.path
                        ))));
                    }
                    FailureBehavior::Warn => {
                        tracing::warn!("failed to read '{}': {err}", state.path);
                    }
                    FailureBehavior::Ignore => {}
                },
            }
        }
        let mut checkpoint = self.create_checkpoint_with_content(entity_id, &entries)?;
        checkpoint.empty_dirs = Some(scan.empty_dirs.clone());
        self.empty_dirs
            .insert(checkpoint.id.clone(), scan.empty_dirs.clone());
        Ok(checkpoint)
    }

    /// Create a checkpoint from a set of watcher-recorded changes: only the
    /// changed files are re-read (O(N) → O(K)), then applied as agent edits
    /// on the actor partition. Unchanged files keep their recorded state.
    /// Deletions are recorded as empty content. The caller should call
    /// `FileWatcher::reset()` afterwards.
    pub fn create_incremental_checkpoint(
        &self,
        entity_id: &str,
        base_dir: &Path,
        changes: &[FileChangeRecord],
        opts: &FileCheckpointOptions,
    ) -> Result<FileCheckpoint, CheckpointError> {
        let mut entries = Vec::new();
        for change in changes {
            let Ok(relative) = change.path.strip_prefix(base_dir) else {
                continue;
            };
            let relative = relative.to_string_lossy().replace('\\', "/");
            match change.kind {
                FileChangeKind::Unlink => {
                    entries.push(FileContentEntry::new(relative, Vec::new()));
                }
                FileChangeKind::Add | FileChangeKind::Change => {
                    match std::fs::read(base_dir.join(&relative)) {
                        Ok(content) => {
                            entries.push(FileContentEntry::new(relative, content));
                        }
                        Err(err) => match opts.failure_behavior {
                            FailureBehavior::Error => {
                                return Err(CheckpointError::Io(std::io::Error::other(format!(
                                    "failed to read changed file '{relative}': {err}"
                                ))));
                            }
                            FailureBehavior::Warn => {
                                tracing::warn!("failed to read changed file '{relative}': {err}");
                            }
                            FailureBehavior::Ignore => {}
                        },
                    }
                }
            }
        }
        self.create_checkpoint_with_content(entity_id, &entries)
    }
    // ── utilities ───────────────────────────────────────────────────

    pub fn compute_file_hash(data: &[u8]) -> String {
        sha256_hex(data)
    }

    pub fn unified_diff(
        previous_content: &str,
        current_content: &str,
        context_lines: usize,
    ) -> String {
        DiffEngine::new()
            .with_context_lines(context_lines)
            .unified_diff(previous_content, current_content, None, None)
    }

    // ── provenance queries ──────────────────────────────────────────

    /// All partitions of the file-checkpoint store (actor partitions,
    /// approval, integrated features, staged).
    pub fn list_partitions(&self) -> Result<Vec<PartitionView>, CheckpointError> {
        crate::provenance::list_partitions(self.storage_ref()?)
    }

    /// Changes recorded in an actor partition, in chronological order.
    pub fn list_changes_by_actor(
        &self,
        actor: &str,
        path_filter: Option<&str>,
        time_range: Option<(i64, i64)>,
    ) -> Result<Vec<DeltaSummary>, CheckpointError> {
        crate::provenance::list_changes_by_actor(
            self.storage_ref()?,
            actor,
            path_filter,
            time_range,
        )
    }

    /// Changes touching a path across every partition. `time_range`
    /// (inclusive `(start, end)` timestamps) narrows the window.
    pub fn list_changes_by_path(
        &self,
        path: &str,
        time_range: Option<(i64, i64)>,
    ) -> Result<Vec<DeltaSummary>, CheckpointError> {
        crate::provenance::list_changes_by_path(self.storage_ref()?, path, time_range)
    }

    /// Reconstructed file set of an actor partition (current state).
    pub fn get_actor_workspace(&self, actor: &str) -> Result<Vec<WorkspaceFile>, CheckpointError> {
        crate::provenance::get_actor_workspace(self.storage_ref()?, actor)
    }

    /// Per-file diff between two actor workspaces.
    pub fn diff_actors(
        &self,
        actor_a: &str,
        actor_b: &str,
    ) -> Result<Vec<FileDiffView>, CheckpointError> {
        crate::provenance::diff_actors(self.storage_ref()?, actor_a, actor_b)
    }

    /// Per-file diff between an actor workspace and the staged partition.
    pub fn diff_against_staged(&self, actor: &str) -> Result<Vec<FileDiffView>, CheckpointError> {
        crate::provenance::diff_against_staged(
            self.storage_ref()?,
            actor,
            self.workspace_key().as_deref(),
        )
    }

    /// Files with unresolved merge conflicts across the staged and feature
    /// partitions, with re-derived conflict regions (see
    /// [`crate::provenance::list_conflicts`]).
    pub fn list_conflicts(&self) -> Result<Vec<crate::provenance::ConflictFile>, CheckpointError> {
        crate::provenance::list_conflicts(self.storage_ref()?, self.workspace_key().as_deref())
    }
}

impl Default for FileCheckpointManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use layertwine::storage::repository::{CheckpointPersist, PartitionStore};

    fn manager() -> FileCheckpointManager {
        FileCheckpointManager::new_in_memory().unwrap()
    }

    fn entry(path: &str, content: &[u8]) -> FileContentEntry {
        FileContentEntry::new(path, content.to_vec())
    }

    fn state_map(states: &[FileState]) -> HashMap<&str, &FileState> {
        states.iter().map(|f| (f.path.as_str(), f)).collect()
    }

    #[test]
    fn compute_file_hash_produces_consistent_output() {
        let hash1 = FileCheckpointManager::compute_file_hash(b"hello world");
        let hash2 = FileCheckpointManager::compute_file_hash(b"hello world");
        assert_eq!(hash1, hash2);
        assert_ne!(
            hash1,
            FileCheckpointManager::compute_file_hash(b"different")
        );
        // SHA-256 hex digest: 64 chars, stable across runs/versions.
        assert_eq!(hash1.len(), 64);
    }

    #[test]
    fn unified_diff_shows_changes() {
        let prev = "line1\nline2\nline3\n";
        let curr = "line1\nline2_modified\nline3\n";

        let diff = FileCheckpointManager::unified_diff(prev, curr, 1);
        assert!(diff.contains("line2"));
        assert!(diff.contains("-line2"));
        assert!(diff.contains("+line2_modified"));
    }

    #[test]
    fn create_and_restore_checkpoint_roundtrip() {
        let manager = manager();
        let cp = manager
            .create_checkpoint(
                "exec-1",
                &[entry("a.txt", b"hello a"), entry("b.txt", b"hello b")],
            )
            .unwrap();
        assert_eq!(cp.files.len(), 2);
        assert_eq!(cp.checkpoint_type, "full");
        assert!(cp.base_checkpoint_id.is_none());

        let restored = manager.restore_checkpoint("exec-1", &cp.id).unwrap();
        let map = state_map(&restored);
        assert_eq!(map["a.txt"].hash, sha256_hex(b"hello a"));
        assert_eq!(map["b.txt"].hash, sha256_hex(b"hello b"));
    }

    #[test]
    fn restore_latest_returns_most_recent() {
        let manager = manager();
        manager
            .create_checkpoint("exec-1", &[entry("a.txt", b"v1")])
            .unwrap();
        manager
            .create_checkpoint("exec-1", &[entry("a.txt", b"v2")])
            .unwrap();

        let latest = manager.restore_latest("exec-1").unwrap().unwrap();
        let map = state_map(&latest);
        assert_eq!(map["a.txt"].hash, sha256_hex(b"v2"));
        assert_eq!(map["a.txt"].size, 2);
    }

    #[test]
    fn restore_latest_none_without_checkpoints() {
        let manager = manager();
        assert!(manager.restore_latest("exec-1").unwrap().is_none());
    }

    #[test]
    fn create_latest_file_checkpoint_snapshots_partition_state() {
        let manager = manager();
        manager
            .create_checkpoint("exec-1", &[entry("a.txt", b"v1")])
            .unwrap();

        let latest = manager.create_latest_file_checkpoint("exec-1").unwrap();
        assert!(latest.is_some());
        let cp = latest.unwrap();
        let map = state_map(&cp.files);
        assert_eq!(map["a.txt"].hash, sha256_hex(b"v1"));

        // No history yet → None.
        assert!(manager
            .create_latest_file_checkpoint("never-touched")
            .unwrap()
            .is_none());
    }

    #[test]
    fn unchanged_files_keep_prior_state_on_next_checkpoint() {
        let manager = manager();
        let cp1 = manager
            .create_checkpoint(
                "exec-1",
                &[entry("a.txt", b"stable"), entry("b.txt", b"changing")],
            )
            .unwrap();
        assert_eq!(cp1.files.len(), 2);

        // Only b.txt is applied again; a.txt keeps its recorded state.
        let cp2 = manager
            .create_checkpoint("exec-1", &[entry("b.txt", b"changed")])
            .unwrap();
        let map = state_map(&cp2.files);
        assert_eq!(map.len(), 2);
        assert_eq!(map["a.txt"].hash, sha256_hex(b"stable"));
        assert_eq!(map["b.txt"].hash, sha256_hex(b"changed"));
    }

    #[test]
    fn binary_files_are_snapshotted_without_diffing() {
        let manager = manager();
        let bytes: Vec<u8> = (0u8..=255u8).collect();
        let cp = manager
            .create_checkpoint("exec-1", &[entry("bin.dat", &bytes)])
            .unwrap();
        let map = state_map(&cp.files);
        assert_eq!(map["bin.dat"].hash, sha256_hex(&bytes));
        assert_eq!(map["bin.dat"].size, 256);
    }

    #[test]
    fn create_and_restore_checkpoint_with_content() {
        let manager = manager();
        let entries = vec![entry("a.txt", b"hello a"), entry("b.txt", b"hello b")];
        let cp = manager
            .create_checkpoint_with_content("exec-1", &entries)
            .unwrap();
        assert_eq!(cp.files.len(), 2);
        assert_eq!(
            cp.files[0].hash,
            sha256_hex(b"hello a"),
            "content hash recorded in FileState"
        );

        // Rollback into a temp dir restores the original bytes.
        let dir = tempfile::tempdir().unwrap();
        let written = manager.restore_content(&cp.id, dir.path()).unwrap();
        assert_eq!(written.len(), 2);
        assert_eq!(std::fs::read(dir.path().join("a.txt")).unwrap(), b"hello a");
        assert_eq!(std::fs::read(dir.path().join("b.txt")).unwrap(), b"hello b");
    }

    #[test]
    fn restore_content_rejects_paths_escaping_base_dir() {
        let manager = manager();
        let cp = manager
            .create_checkpoint("exec-1", &[entry("../escape.txt", b"bad")])
            .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let err = manager.restore_content(&cp.id, dir.path()).unwrap_err();
        assert!(matches!(err, CheckpointError::Validation { .. }));
    }

    #[test]
    fn restore_latest_content_roundtrip() {
        let manager = manager();
        manager
            .create_checkpoint("exec-1", &[entry("v1.txt", b"version one")])
            .unwrap();
        manager
            .create_checkpoint("exec-1", &[entry("v1.txt", b"version two")])
            .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let written = manager
            .restore_latest_content("exec-1", dir.path())
            .unwrap()
            .unwrap();
        assert_eq!(written.len(), 1);
        assert_eq!(
            std::fs::read(dir.path().join("v1.txt")).unwrap(),
            b"version two",
            "latest content wins"
        );
    }

    #[test]
    fn restore_missing_checkpoint_is_not_found() {
        let manager = manager();
        let err = manager
            .restore_checkpoint("exec-1", "ff".repeat(32).as_str())
            .unwrap_err();
        assert!(matches!(err, CheckpointError::NotFound { .. }));
    }

    #[test]
    fn content_store_deduplicates_by_hash() {
        let store = LayertwineFileContentStore::new_in_memory().unwrap();
        let h1 = store.save_content("a.txt", b"same bytes").unwrap();
        let h2 = store.save_content("a.txt", b"same bytes").unwrap();
        assert_eq!(h1, h2);
        assert_eq!(
            store.load_content("a.txt", &h1).unwrap().unwrap(),
            b"same bytes"
        );
        assert!(store.load_content("missing.txt", &h1).unwrap().is_none());
        assert!(store
            .load_content("a.txt", &"ff".repeat(32))
            .unwrap()
            .is_none());
    }

    #[test]
    fn create_incremental_checkpoint_from_watcher_changes() {
        let manager = manager();
        let opts = FileCheckpointOptions::default();
        let dir = tempfile::tempdir().unwrap();

        std::fs::write(dir.path().join("a.txt"), b"v1").unwrap();
        manager
            .create_checkpoint("exec-1", &[entry("a.txt", b"v1")])
            .unwrap();

        std::fs::write(dir.path().join("a.txt"), b"v2").unwrap();
        std::fs::write(dir.path().join("b.txt"), b"new file").unwrap();
        let changes = vec![
            FileChangeRecord {
                path: dir.path().join("a.txt"),
                kind: FileChangeKind::Change,
                timestamp: 1,
            },
            FileChangeRecord {
                path: dir.path().join("b.txt"),
                kind: FileChangeKind::Add,
                timestamp: 2,
            },
        ];
        let cp2 = manager
            .create_incremental_checkpoint("exec-1", dir.path(), &changes, &opts)
            .unwrap();

        let restored = manager.restore_checkpoint("exec-1", &cp2.id).unwrap();
        let map = state_map(&restored);
        assert_eq!(map.len(), 2);
        assert_eq!(map["a.txt"].hash, sha256_hex(b"v2"));
        assert_eq!(map["b.txt"].hash, sha256_hex(b"new file"));
    }

    #[test]
    fn restore_workspace_aligns_files_dirs_and_protects_ignored() {
        let manager = manager();
        let opts = FileCheckpointOptions::default();
        let dir = tempfile::tempdir().unwrap();

        std::fs::write(dir.path().join("a.txt"), b"v1").unwrap();
        std::fs::create_dir_all(dir.path().join("emptydir")).unwrap();
        let cp = manager
            .create_workspace_checkpoint("exec-1", dir.path(), &opts)
            .unwrap();
        assert_eq!(
            cp.empty_dirs.as_deref(),
            Some(vec!["emptydir".to_string()].as_slice())
        );

        // Mutate the workspace: modify a tracked file, add extras (one
        // protected by hardcoded ignores).
        std::fs::write(dir.path().join("a.txt"), b"v2").unwrap();
        std::fs::write(dir.path().join("extra.txt"), b"extra").unwrap();
        std::fs::create_dir_all(dir.path().join("node_modules")).unwrap();
        std::fs::write(dir.path().join("node_modules/lib.js"), b"lib").unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join(".git/config"), b"git").unwrap();
        std::fs::remove_dir(dir.path().join("emptydir")).unwrap();

        let result = manager
            .restore_workspace("exec-1", &cp.id, dir.path(), &opts)
            .unwrap();
        assert_eq!(result.restored, 1, "a.txt written back");
        assert_eq!(result.deleted, 1, "extra.txt removed");
        assert_eq!(result.skipped, 0);

        assert_eq!(std::fs::read(dir.path().join("a.txt")).unwrap(), b"v1");
        assert!(!dir.path().join("extra.txt").exists());
        assert!(
            dir.path().join("node_modules/lib.js").exists(),
            "ignored files are never deleted"
        );
        assert!(dir.path().join(".git/config").exists());
        assert!(
            dir.path().join("emptydir").is_dir(),
            "empty directory recreated"
        );

        // Second restore: everything already matches.
        let result2 = manager
            .restore_workspace("exec-1", &cp.id, dir.path(), &opts)
            .unwrap();
        assert_eq!(result2.skipped, 1);
        assert_eq!(result2.restored, 0);
        assert_eq!(result2.deleted, 0);
    }

    #[test]
    fn restore_workspace_via_latest_resolves_latest_state() {
        let manager = manager();
        let opts = FileCheckpointOptions::default();
        let dir = tempfile::tempdir().unwrap();

        std::fs::write(dir.path().join("a.txt"), b"v1").unwrap();
        manager
            .create_workspace_checkpoint("exec-1", dir.path(), &opts)
            .unwrap();

        std::fs::write(dir.path().join("a.txt"), b"v2").unwrap();
        std::fs::write(dir.path().join("b.txt"), b"new").unwrap();
        manager
            .create_workspace_checkpoint("exec-1", dir.path(), &opts)
            .unwrap();

        std::fs::write(dir.path().join("a.txt"), b"v3").unwrap();
        std::fs::write(dir.path().join("b.txt"), b"newer").unwrap();
        manager
            .create_workspace_checkpoint("exec-1", dir.path(), &opts)
            .unwrap();

        std::fs::write(dir.path().join("a.txt"), b"mutated").unwrap();
        std::fs::write(dir.path().join("b.txt"), b"mutated too").unwrap();
        std::fs::write(dir.path().join("stray.txt"), b"stray").unwrap();

        let result = manager
            .restore_latest_workspace("exec-1", dir.path(), &opts)
            .unwrap()
            .unwrap();
        assert_eq!(result.restored, 2, "a.txt and b.txt written back");
        assert_eq!(result.deleted, 1, "stray.txt removed");
        assert_eq!(result.skipped, 0);
        assert_eq!(std::fs::read(dir.path().join("a.txt")).unwrap(), b"v3");
        assert_eq!(std::fs::read(dir.path().join("b.txt")).unwrap(), b"newer");
        assert!(!dir.path().join("stray.txt").exists());
    }

    #[test]
    fn actor_id_for_parses_full_actor_or_defaults_to_agent() {
        let manager = manager();
        let actor = manager.actor_id_for("agent:loop-1");
        assert_eq!(actor.as_str(), "agent:loop-1");
        let actor = manager.actor_id_for("wf:exec-1/child:sub-1");
        assert_eq!(actor.as_str(), "wf:exec-1/child:sub-1");
        let actor = manager.actor_id_for("bare-exec-id");
        assert_eq!(actor.as_str(), "agent:bare-exec-id");
    }

    #[test]
    fn apply_agent_edit_records_edits_per_actor() {
        let manager = manager();
        let actor = manager.actor_id_for("exec-1");
        manager.apply_agent_edit(&actor, "a.txt", b"first").unwrap();
        let snap2 = manager
            .apply_agent_edit(&actor, "a.txt", b"second")
            .unwrap();
        assert!(!snap2.is_empty());

        let latest = manager
            .create_latest_file_checkpoint("exec-1")
            .unwrap()
            .unwrap();
        let map = state_map(&latest.files);
        assert_eq!(map["a.txt"].hash, sha256_hex(b"second"));
    }

    #[test]
    fn merge_entity_changes_moves_agent_to_feature() {
        let manager = manager();
        manager
            .create_checkpoint("exec-1", &[entry("a.txt", b"base\n")])
            .unwrap();
        manager
            .create_checkpoint("exec-1", &[entry("a.txt", b"base\nagent change\n")])
            .unwrap();

        let result = manager.merge_entity_changes("exec-1", "feature-1").unwrap();
        assert!(!result.merge_result.has_conflicts());
        assert!(!result.merge_result.snapshot_id.to_hex().is_empty());
        assert!(!result.checkpoint_id.is_empty());

        let staged = manager.merge_features_to_staged(&["feature-1"]).unwrap();
        assert!(!staged.merge_result.has_conflicts());
        assert!(!staged.checkpoint_id.is_empty());
    }

    /// Reads the reconstructed text of every file in an integrated (feature)
    /// partition, in history order (last occurrence per path wins).
    fn feature_texts(storage: &SqliteStorage, feature_name: &str) -> HashMap<String, String> {
        let pid = layertwine::layered::integrated::integrated_partition_id(feature_name);
        let partition = storage.get_partition(&pid).unwrap();
        let mut texts: HashMap<String, String> = HashMap::new();
        for snapshot_id in &partition.history {
            let snapshot = storage.get_snapshot(snapshot_id).unwrap();
            let path = crate::provenance::snapshot_file_path(storage, &snapshot).unwrap();
            let text = layertwine::layered::transition::reconstruct_text(storage, &snapshot)
                .unwrap()
                .unwrap_or_default();
            texts.insert(path, text);
        }
        texts
    }

    #[test]
    fn approve_changes_file_level_selects_paths() {
        let manager = manager();
        // Three files submitted; the approval layer only advances the last
        // edited file, the others are read back from the agent partition.
        for (path, content) in [
            ("a.txt", b"base-a".as_slice()),
            ("b.txt", b"base-b".as_slice()),
            ("c.txt", b"base-c".as_slice()),
        ] {
            manager
                .create_checkpoint("exec-1", &[entry(path, content)])
                .unwrap();
        }
        for (path, content) in [
            ("a.txt", b"new-a".as_slice()),
            ("b.txt", b"new-b".as_slice()),
            ("c.txt", b"new-c".as_slice()),
        ] {
            manager
                .create_checkpoint("exec-1", &[entry(path, content)])
                .unwrap();
        }
        manager.move_agent_to_approval("exec-1").unwrap();

        // Approve only a.txt and b.txt.
        let outcome = manager
            .approve_changes(
                "exec-1",
                "feature-1",
                Some(vec!["a.txt".to_string(), "b.txt".to_string()]),
                ConflictBehavior::Marker,
                None,
            )
            .unwrap();
        assert!(outcome.merged, "file-level approval should merge");

        // Feature now holds the two approved files; c.txt stays unapproved.
        let storage = manager.storage().unwrap();
        let texts = feature_texts(storage, "feature-1");
        assert_eq!(texts.get("a.txt").map(String::as_str), Some("new-a"));
        assert_eq!(texts.get("b.txt").map(String::as_str), Some("new-b"));
        assert!(!texts.contains_key("c.txt"), "c.txt must stay pending");

        // The approval layer still reports the submission as pending.
        let pending = manager.list_pending_approvals().unwrap();
        assert_eq!(pending.len(), 1, "remaining files stay pending");
    }

    #[test]
    fn approve_changes_full_batch_matches_legacy_behavior() {
        let manager = manager();
        manager
            .create_checkpoint("exec-1", &[entry("a.txt", b"base\n")])
            .unwrap();
        manager
            .create_checkpoint("exec-1", &[entry("a.txt", b"base\nagent\n")])
            .unwrap();

        // `paths = None` behaves exactly like the pre-F3 full-batch approve.
        let outcome = manager
            .approve_changes("exec-1", "feature-1", None, ConflictBehavior::Marker, None)
            .unwrap();
        assert!(outcome.merged);
        assert!(!outcome.has_conflicts());
        let pending = manager.list_pending_approvals().unwrap();
        assert!(
            pending.is_empty(),
            "full-batch approve clears the submission"
        );
    }

    #[test]
    fn conflict_flow_lists_and_resolves() {
        let manager = manager();
        // Agent 1 edits the first line and merges cleanly.
        manager
            .create_checkpoint("exec-1", &[entry("a.txt", b"base\nline2\n")])
            .unwrap();
        manager
            .create_checkpoint("exec-1", &[entry("a.txt", b"one\nline2\n")])
            .unwrap();
        let r1 = manager.merge_entity_changes("exec-1", "feature-1").unwrap();
        assert!(!r1.merge_result.has_conflicts());

        // Agent 2 edits the same line differently → conflict.
        manager
            .create_checkpoint("exec-2", &[entry("a.txt", b"base\nline2\n")])
            .unwrap();
        manager
            .create_checkpoint("exec-2", &[entry("a.txt", b"two\nline2\n")])
            .unwrap();
        let outcome = manager
            .approve_changes("exec-2", "feature-1", None, ConflictBehavior::Marker, None)
            .unwrap();
        assert!(outcome.has_conflicts(), "same-line edits must conflict");
        assert!(outcome.conflict_files.contains(&"a.txt".to_string()));

        // list_conflicts reports the conflicted path.
        let conflicts = manager.list_conflicts().unwrap();
        assert!(
            conflicts.iter().any(|c| c.path == "a.txt"),
            "list_conflicts must report a.txt"
        );

        // Resolve with the authoritative content; the marker is cleared.
        let remaining = manager
            .resolve_conflicts(
                "exec-2",
                "feature-1",
                &[("a.txt".to_string(), b"resolved\nline2\n".to_vec())],
            )
            .unwrap();
        assert_eq!(remaining, 0, "resolution must clear all conflicts");
        let conflicts = manager.list_conflicts().unwrap();
        assert!(
            conflicts.is_empty(),
            "no conflicts may remain after resolution"
        );

        // A subsequent full merge succeeds cleanly.
        let remerged = manager.merge_entity_changes("exec-2", "feature-1").unwrap();
        assert!(
            !remerged.merge_result.has_conflicts(),
            "re-merge after resolution must succeed"
        );
    }

    #[test]
    fn merge_branch_changes_joins_features_and_cleans_pointers() {
        let manager = manager();
        // Branch 1 edits a.txt.
        manager
            .create_checkpoint("exec-1", &[entry("a.txt", b"base-a")])
            .unwrap();
        manager
            .create_checkpoint("exec-1", &[entry("a.txt", b"branch-a")])
            .unwrap();
        manager.merge_entity_changes("exec-1", "branch-1").unwrap();
        // Branch 2 edits b.txt.
        manager
            .create_checkpoint("exec-2", &[entry("b.txt", b"base-b")])
            .unwrap();
        manager
            .create_checkpoint("exec-2", &[entry("b.txt", b"branch-b")])
            .unwrap();
        manager.merge_entity_changes("exec-2", "branch-2").unwrap();

        let storage = manager.storage().unwrap();
        // Create branch head pointers so the join has something to clean up.
        let head = storage
            .get_partition(&layertwine::layered::integrated::integrated_partition_id(
                "branch-1",
            ))
            .unwrap()
            .current_snapshot;
        storage
            .store_branch(&layertwine::checkpoint::branch::Branch::new(
                "branch-1", head,
            ))
            .unwrap();
        let head2 = storage
            .get_partition(&layertwine::layered::integrated::integrated_partition_id(
                "branch-2",
            ))
            .unwrap()
            .current_snapshot;
        storage
            .store_branch(&layertwine::checkpoint::branch::Branch::new(
                "branch-2", head2,
            ))
            .unwrap();

        // Join: merge both features into staged, then delete the pointers.
        let joined = manager
            .merge_branch_changes(&["branch-1", "branch-2"])
            .unwrap();
        assert!(
            !joined.merge_result.has_conflicts(),
            "different-file branches must join cleanly"
        );

        // Both changes are present in the staged workspace.
        let staged = crate::provenance::get_staged_workspace(storage, None).unwrap();
        let map: HashMap<&str, &WorkspaceFile> =
            staged.iter().map(|f| (f.path.as_str(), f)).collect();
        assert_eq!(map["a.txt"].content, b"branch-a");
        assert_eq!(map["b.txt"].content, b"branch-b");

        // Branch head pointers are removed; the DAG data stays intact.
        let branches = storage.list_branches().unwrap();
        assert!(
            !branches
                .iter()
                .any(|b| b.name == "branch-1" || b.name == "branch-2"),
            "join must delete the branch head pointers"
        );
    }

    #[test]
    fn layered_partitions_isolate_actors() {
        let manager = manager();
        manager
            .create_checkpoint("agent:exec-1", &[entry("a.txt", b"actor one")])
            .unwrap();
        manager
            .create_checkpoint("agent:exec-2", &[entry("a.txt", b"actor two")])
            .unwrap();

        let one = manager.restore_latest("agent:exec-1").unwrap().unwrap();
        let two = manager.restore_latest("agent:exec-2").unwrap().unwrap();
        assert_eq!(one[0].hash, sha256_hex(b"actor one"));
        assert_eq!(two[0].hash, sha256_hex(b"actor two"));
    }

    /// Two managers over one shared DB with different workspace roots must
    /// not cross-write: manual edits land in workspace-scoped partitions.
    #[test]
    fn multi_workspace_manual_partitions_do_not_cross_write() {
        let storage = Arc::new(
            layertwine::storage::SqliteStorage::new_full_in_memory()
                .map_err(map_layertwine_error)
                .unwrap(),
        );
        let mut m_a = FileCheckpointManager::with_sqlite(storage.clone());
        m_a.workspace_root = Some(PathBuf::from("/ws/a"));
        let mut m_b = FileCheckpointManager::with_sqlite(storage.clone());
        m_b.workspace_root = Some(PathBuf::from("/ws/b"));

        // Same file edited in both workspaces with different content.
        m_a.apply_manual_edit("a.txt", b"from workspace a").unwrap();
        m_b.apply_manual_edit("a.txt", b"from workspace b").unwrap();

        let pid_a = layertwine::layered::manual::manual_partition_id_for("/ws/a");
        let pid_b = layertwine::layered::manual::manual_partition_id_for("/ws/b");
        assert_ne!(pid_a, pid_b, "workspace-scoped manual partitions differ");

        let part_a = storage.get_partition(&pid_a).unwrap();
        let part_b = storage.get_partition(&pid_b).unwrap();
        assert_ne!(
            part_a.current_snapshot, part_b.current_snapshot,
            "edits must not bleed across workspaces"
        );

        // Each workspace's manual partition carries only its own edit.
        let text_a = layertwine::layered::transition::reconstruct_text(
            &*storage,
            &storage.get_snapshot(&part_a.current_snapshot).unwrap(),
        )
        .unwrap()
        .unwrap_or_default();
        let text_b = layertwine::layered::transition::reconstruct_text(
            &*storage,
            &storage.get_snapshot(&part_b.current_snapshot).unwrap(),
        )
        .unwrap()
        .unwrap_or_default();
        assert_eq!(text_a, "from workspace a");
        assert_eq!(text_b, "from workspace b");
    }

    /// Without a workspace root the manager keeps the legacy fixed
    /// partition ids (behavior identical to the pre-F5 single-workspace
    /// mode).
    #[test]
    fn no_workspace_root_keeps_legacy_partition_ids() {
        let manager = manager();
        assert_eq!(manager.workspace_key(), None);

        let legacy_manual = layertwine::layered::manual::manual_partition_id();
        let legacy_staged = layertwine::layered::staged::staged_partition_id();

        manager.apply_manual_edit("a.txt", b"legacy").unwrap();
        let storage = manager.storage().unwrap();
        assert!(
            storage.get_partition(&legacy_manual).is_ok(),
            "no workspace root must use the legacy manual partition id"
        );

        // The derived ids are unrelated to the legacy ids.
        assert_ne!(
            layertwine::layered::manual::manual_partition_id_for("/ws/x"),
            legacy_manual
        );
        assert_ne!(
            layertwine::layered::staged::staged_partition_id_for("/ws/x"),
            legacy_staged
        );
    }

    /// Opening a persistent DB binds it to the workspace root recorded in
    /// metadata; a different root on the same DB is rejected.
    #[test]
    fn open_from_config_binds_workspace_root_to_db() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("cp.db");
        let config_for = |root: &str| wf_types::config::file_checkpoint::FileCheckpointConfig {
            storage: Some(wf_types::config::file_checkpoint::FileCheckpointStorageConfig {
                storage_type: wf_types::config::file_checkpoint::FileCheckpointStorageType::Sqlite,
                db_path: Some(db_path.to_string_lossy().into_owned()),
            }),
            workspace_root: Some(root.to_string()),
            ..Default::default()
        };

        // First open records the binding.
        FileCheckpointManager::open_from_config(&config_for("/ws/a")).unwrap();
        // Reopening with the same root is fine.
        FileCheckpointManager::open_from_config(&config_for("/ws/a")).unwrap();
        // A different root on the same DB is rejected.
        let err = match FileCheckpointManager::open_from_config(&config_for("/ws/b")) {
            Ok(_) => panic!("different workspace root on a bound DB must fail"),
            Err(e) => e,
        };
        assert!(
            matches!(err, CheckpointError::Validation { .. }),
            "different workspace root on a bound DB must fail: {err:?}"
        );
        // No workspace root is not bound (legacy mode still opens).
        let legacy = wf_types::config::file_checkpoint::FileCheckpointConfig {
            storage: Some(wf_types::config::file_checkpoint::FileCheckpointStorageConfig {
                storage_type: wf_types::config::file_checkpoint::FileCheckpointStorageType::Sqlite,
                db_path: Some(db_path.to_string_lossy().into_owned()),
            }),
            ..Default::default()
        };
        FileCheckpointManager::open_from_config(&legacy).unwrap();
    }

    #[test]
    fn no_file_checkpoint_delta_remnants() {
        // Incremental storage is owned by layertwine partition history; the
        // hand-written delta projection must stay deleted. Needles are
        // assembled from fragments so this test's own source cannot match.
        let delta_needle = ["FileCheckpoint", "Delta"].concat();
        let file_src = include_str!("file.rs");
        assert!(
            !file_src.contains(&delta_needle),
            "delta projection struct must not be reintroduced in file.rs"
        );
        let lib_src = include_str!("lib.rs");
        assert!(
            !lib_src.contains(&delta_needle),
            "delta projection struct must not be re-exported from lib.rs"
        );
    }

    #[test]
    fn no_max_delta_chain_in_file_options() {
        let option_needle = ["max_", "delta_chain_", "length"].concat();
        let file_src = include_str!("file.rs");
        assert!(
            !file_src.contains(&option_needle),
            "checkpoint options must stay free of the dead chain-length knob"
        );
    }
}
