use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use dashmap::DashMap;
use layertwine::checkpoint::types::{Checkpoint, CheckpointMetadata};
use layertwine::core::delta::Delta;
use layertwine::core::file_node::FileNode;
use layertwine::core::partition::Partition;
use layertwine::core::snapshot::{Snapshot, SnapshotContent};
use layertwine::core::types::{AgentInstanceId, CheckpointId, LineDiff, SnapshotId, SourceType};
use layertwine::layered::agent;
use layertwine::layered::{MergeResult, StateMachine};
use layertwine::storage::repository::{
    CheckpointPersist, DeltaStore, FileNodeStore, PartitionStore, SnapshotStore,
};
use layertwine::storage::sqlite::SqliteStorage;
use sha2::{Digest, Sha256};
use wf_types::config::file_checkpoint::{ApprovalPolicy, ConflictBehavior, FailureBehavior};

use crate::actor_id::{ActorId, ActorKind};
use crate::approval::{inject_conflict_markers, to_conflict_views, MergeOutcome, PendingApproval};
use crate::diff::DiffEngine;
use crate::error::CheckpointError;
use crate::event::CheckpointEventBus;
use crate::provenance::{DeltaSummary, FileDiffView, PartitionView, WorkspaceFile};
use crate::recent_agent_writes::RecentAgentWrites;
use crate::scan::{is_hardcoded_ignored, ScanConfig, WorkspaceScanner};
use crate::script_capture::{CollectedChange, CollectedChangeKind, WorkspaceChangeCollector};
use crate::watcher::{FileChangeKind, FileChangeRecord};

/// SHA-256 hex digest of a byte slice. Replaces the earlier
/// `DefaultHasher` (SipHash) based fingerprint, which is not stable across
/// Rust versions and therefore could not serve as a durable content
/// identifier.
pub fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect()
}

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
    /// Always `None` in the projection (changes live in layertwine deltas).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changes: Option<FileCheckpointDelta>,
    /// Directories that contained no files at snapshot time; recreated on
    /// workspace restore. Kept in the projection index (not in layertwine).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub empty_dirs: Option<Vec<String>>,
}

/// Kept for serialization compatibility with the historical checkpoint
/// shape; no longer produced by the manager (layertwine deltas replace it).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct FileCheckpointDelta {
    pub added: Vec<FileState>,
    pub modified: Vec<FileState>,
    pub deleted: Vec<String>,
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
/// `SnapshotContent::FileContent` snapshots stored in layertwine's SQLite
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

    /// Share the underlying SQLite connection (for test diagnostics).
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
    /// Kept for configuration compatibility. layertwine partitions retain
    /// full history (INSERT-ONLY), so no chain-length forcing applies.
    pub max_delta_chain_length: u32,
    /// Per-file error handling during scan/restore.
    pub failure_behavior: FailureBehavior,
    /// Additional ignore patterns applied while scanning the workspace.
    pub custom_ignore_patterns: Vec<String>,
}

impl Default for FileCheckpointOptions {
    fn default() -> Self {
        Self {
            max_delta_chain_length: 20,
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

/// Path of the synthetic initial snapshot seeding an actor partition. The
/// seed holds an empty baseline so the first `apply_agent_edit` produces a
/// full-file delta; it is never part of the projected file set (empty path).
const SEED_PATH: &str = ".wf-checkpoint-seed";

/// File checkpoint engine rebuilt on top of layertwine's layered state
/// machine (authoritative model). The manager drives
/// `layertwine::layered::StateMachine<SqliteStorage>`: agent partitions hold
/// per-actor file edits (`apply_agent_edit`), checkpoint creation snapshots
/// the partition state into layertwine `Checkpoint`s, and restore goes
/// through `transition::reconstruct_text`. `FileCheckpoint` / `FileState`
/// are projections over the layertwine model.
pub struct FileCheckpointManager {
    storage: Option<Arc<SqliteStorage>>,
    state_machine: Option<StateMachine<SqliteStorage>>,
    /// Actor id -> latest checkpoint id (projection index; cheap in-memory
    /// mirror, the DB remains authoritative).
    latest_checkpoints: Arc<DashMap<String, String>>,
    /// Checkpoint id -> empty directories recorded at snapshot time
    /// (projection-only, not stored in layertwine).
    empty_dirs: Arc<DashMap<String, Vec<String>>>,
    /// Path -> content hash registry of recent agent writes (manual watcher
    /// uses it to distinguish agent self-writes from human edits).
    recent_agent_writes: Arc<RecentAgentWrites>,
    /// Actor id -> file paths deleted by the actor (deletion projection
    /// marker). Restores exclude these paths so the files are removed from the
    /// workspace.
    deleted_files: Arc<DashMap<String, HashSet<String>>>,
    /// Change-event feed: every recorded agent/manual edit publishes a
    /// `CheckpointEvent::FileChanged`. Absent when the manager is used
    /// without an event layer.
    event_bus: Option<CheckpointEventBus>,
    /// Workspace root the manager is bound to (from `FileCheckpointConfig`).
    /// Scoped captures (script diff, manual watcher) restrict their scope to
    /// this root; `None` disables them.
    workspace_root: Option<PathBuf>,
    /// Workspace scan rules (ignore patterns + per-file failure behavior).
    scan_config: ScanConfig,
    /// Layered approval policy applied when an execution ends
    /// (`on_agent_complete`); flows the actor partition through the approval
    /// layer before merging into a feature.
    approval_policy: ApprovalPolicy,
    /// Three-way merge conflict strategy applied by `approve_changes` /
    /// `merge_entity_changes` (marker / fail / approval).
    conflict_behavior: ConflictBehavior,
    /// Entity id -> resolved `ActorId` (sub-execution isolation). Built at
    /// first actor resolution: a child execution whose parent is known in
    /// the index gets `parent.child(execution_id)`, so nested executions
    /// live in their own hierarchical partition.
    actor_index: Arc<DashMap<String, ActorId>>,
}

impl Clone for FileCheckpointManager {
    fn clone(&self) -> Self {
        Self {
            storage: self.storage.clone(),
            state_machine: self
                .storage
                .as_ref()
                .map(|storage| StateMachine::new(storage.clone())),
            latest_checkpoints: self.latest_checkpoints.clone(),
            empty_dirs: self.empty_dirs.clone(),
            recent_agent_writes: self.recent_agent_writes.clone(),
            deleted_files: self.deleted_files.clone(),
            event_bus: self.event_bus.clone(),
            workspace_root: self.workspace_root.clone(),
            scan_config: self.scan_config.clone(),
            approval_policy: self.approval_policy,
            conflict_behavior: self.conflict_behavior,
            actor_index: self.actor_index.clone(),
        }
    }
}

impl FileCheckpointManager {
    pub fn new() -> Self {
        Self {
            storage: None,
            state_machine: None,
            latest_checkpoints: Arc::new(DashMap::new()),
            empty_dirs: Arc::new(DashMap::new()),
            recent_agent_writes: Arc::new(RecentAgentWrites::new()),
            deleted_files: Arc::new(DashMap::new()),
            event_bus: None,
            workspace_root: None,
            scan_config: ScanConfig::default(),
            approval_policy: ApprovalPolicy::default(),
            conflict_behavior: ConflictBehavior::default(),
            actor_index: Arc::new(DashMap::new()),
        }
    }

    /// Attach a layertwine SQLite backend; this is the production entry
    /// point (the storage is shared with the surrounding runtime).
    pub fn with_sqlite(storage: Arc<SqliteStorage>) -> Self {
        let state_machine = StateMachine::new(storage.clone());
        Self {
            storage: Some(storage),
            state_machine: Some(state_machine),
            latest_checkpoints: Arc::new(DashMap::new()),
            empty_dirs: Arc::new(DashMap::new()),
            recent_agent_writes: Arc::new(RecentAgentWrites::new()),
            deleted_files: Arc::new(DashMap::new()),
            event_bus: None,
            workspace_root: None,
            scan_config: ScanConfig::default(),
            approval_policy: ApprovalPolicy::default(),
            conflict_behavior: ConflictBehavior::default(),
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

    /// Resolve the actor partition for a child execution: full `ActorId`
    /// strings parse as-is; otherwise a child actor is derived from the
    /// given parent (`parent.child(execution_id)`) so nested executions are
    /// isolated in their own partition.
    pub fn actor_id_for_child(
        &self,
        entity_id: &str,
        parent: Option<&ActorId>,
    ) -> Result<ActorId, CheckpointError> {
        if let Some(actor) = self.actor_index.get(entity_id) {
            return Ok(actor.clone());
        }
        if let Ok(actor) = ActorId::parse(entity_id) {
            self.actor_index
                .insert(entity_id.to_string(), actor.clone());
            return Ok(actor);
        }
        let child_id = wf_types::Id::from(entity_id.to_string());
        let actor = match parent {
            Some(parent) => parent
                .child(&child_id)
                .map_err(|e| CheckpointError::Validation {
                    reason: format!("invalid child actor for '{entity_id}': {e}"),
                }),
            None => ActorId::new(ActorKind::Agent, &[child_id]).map_err(|e| {
                CheckpointError::Validation {
                    reason: format!("invalid actor for '{entity_id}': {e}"),
                }
            }),
        }?;
        self.actor_index
            .insert(entity_id.to_string(), actor.clone());
        Ok(actor)
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
        Ok(manager)
    }

    /// In-memory backend for tests and tooling.
    pub fn new_in_memory() -> Result<Self, CheckpointError> {
        let storage = Arc::new(SqliteStorage::new_full_in_memory().map_err(map_layertwine_error)?);
        Ok(Self::with_sqlite(storage))
    }

    pub fn state_machine(&self) -> Option<&StateMachine<SqliteStorage>> {
        self.state_machine.as_ref()
    }

    pub fn storage(&self) -> Option<&Arc<SqliteStorage>> {
        self.storage.as_ref()
    }

    fn storage_ref(&self) -> Result<&SqliteStorage, CheckpointError> {
        self.storage.as_deref().ok_or_else(|| {
            CheckpointError::Coordinator("no file checkpoint storage configured".to_string())
        })
    }

    /// Resolve the actor for an execution entity id. Full `ActorId` strings
    /// (e.g. `agent:{loop_id}` / `wf:{workflow_id}/child:{subgraph_id}`)
    /// parse as-is; bare execution ids map to a root agent actor. A
    /// previously resolved hierarchical actor (via [`Self::resolve_actor`])
    /// takes precedence so later calls keep the same partition.
    pub fn actor_id_for(&self, entity_id: &str) -> ActorId {
        if let Some(actor) = self.actor_index.get(entity_id) {
            return actor.clone();
        }
        let actor = match ActorId::parse(entity_id) {
            Ok(actor) => actor,
            Err(_) => root_actor(wf_types::Id::from(entity_id.to_string())),
        };
        self.actor_index
            .insert(entity_id.to_string(), actor.clone());
        actor
    }

    /// Resolve the actor for an execution with an optional immediate parent
    /// (sub-execution isolation). A child execution whose parent is
    /// already in the actor index is encoded as `parent.child(execution_id)`
    /// (`{kind}:{parent}/child:{child}`), keeping nested executions in their
    /// own hierarchical partition. Falls back to a root actor when the
    /// parent is unknown.
    pub fn resolve_actor(&self, entity_id: &str, parent_execution_id: Option<&str>) -> ActorId {
        if let Some(actor) = self.actor_index.get(entity_id) {
            return actor.clone();
        }
        if let Ok(actor) = ActorId::parse(entity_id) {
            self.actor_index
                .insert(entity_id.to_string(), actor.clone());
            return actor;
        }
        let child_id = wf_types::Id::from(entity_id.to_string());
        let actor = match parent_execution_id {
            Some(parent) if parent != entity_id => match self.actor_index.get(parent) {
                Some(parent_actor) => parent_actor
                    .child(&child_id)
                    .unwrap_or_else(|_| root_actor(child_id.clone())),
                None => root_actor(child_id.clone()),
            },
            _ => root_actor(child_id.clone()),
        };
        self.actor_index
            .insert(entity_id.to_string(), actor.clone());
        actor
    }

    /// The resolved actor of an entity, if it was resolved earlier.
    pub fn resolved_actor(&self, entity_id: &str) -> Option<ActorId> {
        self.actor_index.get(entity_id).map(|a| a.clone())
    }

    // ── actor partition primitives ──────────────────────────────────

    /// Ensure the actor's agent partition exists, seeding it with an empty
    /// baseline snapshot on first use.
    pub fn ensure_agent_partition(&self, actor: &ActorId) -> Result<(), CheckpointError> {
        let storage = self.storage_ref()?;
        let agent_id = actor.to_agent_instance_id();
        let pid = agent::agent_partition_id(&agent_id);
        if storage.get_partition(&pid).is_ok() {
            return Ok(());
        }
        let initial = seed_initial_snapshot(storage, &agent_id)?;
        agent::ensure_agent_partition(storage, &agent_id, initial).map_err(map_layertwine_error)?;
        Ok(())
    }

    /// Record one file edit for an actor: text goes through layertwine's
    /// line-diff `apply_agent_edit`; binary content is snapshotted verbatim
    /// via `SnapshotContent::FileContent` (no line diff). Returns the new
    /// snapshot id (hex).
    pub fn apply_agent_edit(
        &self,
        actor: &ActorId,
        path: &str,
        content: &[u8],
    ) -> Result<String, CheckpointError> {
        let storage = self.storage_ref()?;
        let agent_id = actor.to_agent_instance_id();
        self.ensure_agent_partition(actor)?;
        let snapshot_id = if let Ok(text) = std::str::from_utf8(content) {
            agent::apply_agent_edit(storage, &agent_id, path, text).map_err(map_layertwine_error)?
        } else {
            let file_node = FileNode::new(PathBuf::from(path), content);
            let snapshot = Snapshot::new_with_content(
                file_node,
                SnapshotContent::FileContent(content.to_vec()),
                format!("file://{}", path),
                format!("agent/{}", agent_id),
                vec![],
                vec![],
            );
            storage
                .store_snapshot(&snapshot, content)
                .map_err(map_layertwine_error)?;
            let pid = agent::agent_partition_id(&agent_id);
            storage
                .update_pointer(&pid, &snapshot.id)
                .map_err(map_layertwine_error)?;
            snapshot.id
        };
        // Clear any earlier deletion marker for this path (the file exists
        // again); register the write for the manual watcher. The registry is
        // keyed by both the (workspace-relative) edit path and the absolute
        // path — watcher events always carry absolute paths.
        if !content.is_empty() {
            if let Some(mut deleted) = self.deleted_files.get_mut(actor.as_str()) {
                deleted.remove(path);
            }
        }
        let write_hash = sha256_hex(content);
        self.recent_agent_writes
            .register(PathBuf::from(path), write_hash.clone());
        if let Some(root) = &self.workspace_root {
            self.recent_agent_writes
                .register(root.join(path), write_hash.clone());
        }
        if let Some(ref bus) = self.event_bus {
            bus.publish(CheckpointEventBus::file_changed_with_summary(
                snapshot_id.to_hex(),
                path,
                actor.as_str(),
                Some(DeltaSummary {
                    file: path.to_string(),
                    source: actor.as_str().to_string(),
                    timestamp: wf_common::now(),
                    snapshot_id: snapshot_id.to_hex(),
                    hash: write_hash.clone(),
                }),
            ));
        }
        Ok(snapshot_id.to_hex())
    }

    /// Record a manual (human/IDE) edit into the global manual partition,
    /// bypassing any actor. Text content goes through layertwine's line-diff
    /// `apply_manual_edit`; binary content is snapshotted verbatim via
    /// `SnapshotContent::FileContent`. Returns the new snapshot id (hex).
    pub fn apply_manual_edit(&self, path: &str, content: &[u8]) -> Result<String, CheckpointError> {
        let storage = self.storage_ref()?;
        let manual_pid = layertwine::layered::manual::manual_partition_id();
        if storage.get_partition(&manual_pid).is_err() {
            let seed = seed_initial_snapshot(storage, &AgentInstanceId("manual".into()))?;
            layertwine::layered::manual::ensure_manual_partition(storage, seed)
                .map_err(map_layertwine_error)?;
        }
        let snapshot_id = if let Ok(text) = std::str::from_utf8(content) {
            layertwine::layered::manual::apply_manual_edit(storage, path, text)
                .map_err(map_layertwine_error)?
        } else {
            let file_node = FileNode::new(PathBuf::from(path), content);
            let snapshot = Snapshot::new_with_content(
                file_node,
                SnapshotContent::FileContent(content.to_vec()),
                format!("file://{}", path),
                "manual".to_string(),
                vec![],
                vec![],
            );
            storage
                .store_snapshot(&snapshot, content)
                .map_err(map_layertwine_error)?;
            storage
                .update_pointer(
                    &layertwine::layered::manual::manual_partition_id(),
                    &snapshot.id,
                )
                .map_err(map_layertwine_error)?;
            snapshot.id
        };
        if let Some(ref bus) = self.event_bus {
            let write_hash = sha256_hex(content);
            bus.publish(CheckpointEventBus::file_changed_with_summary(
                snapshot_id.to_hex(),
                path,
                "manual",
                Some(DeltaSummary {
                    file: path.to_string(),
                    source: "manual".to_string(),
                    timestamp: wf_common::now(),
                    snapshot_id: snapshot_id.to_hex(),
                    hash: write_hash,
                }),
            ));
        }
        Ok(snapshot_id.to_hex())
    }

    /// Apply a set of collected workspace changes (script capture) as agent
    /// edits on the actor partition. Add/Modify changes read the file
    /// content from disk; Delete changes apply empty content and register the
    /// deletion projection marker. Per-file failures follow `behavior`.
    /// Returns the number of successfully applied changes.
    pub fn apply_workspace_changes(
        &self,
        actor: &ActorId,
        base_dir: &Path,
        changes: &[CollectedChange],
        behavior: FailureBehavior,
    ) -> Result<usize, CheckpointError> {
        let mut applied = 0;
        for change in changes {
            let Ok(relative) = change.path.strip_prefix(base_dir) else {
                continue;
            };
            let relative = relative.to_string_lossy().replace('\\', "/");
            match change.kind {
                CollectedChangeKind::Delete => {
                    self.deleted_files
                        .entry(actor.as_str().to_string())
                        .or_default()
                        .insert(relative.clone());
                    match self.apply_agent_edit(actor, &relative, b"") {
                        Ok(_) => applied += 1,
                        Err(err) => match behavior {
                            FailureBehavior::Error => return Err(err),
                            FailureBehavior::Warn => {
                                tracing::warn!("failed to apply delete of '{relative}': {err}")
                            }
                            FailureBehavior::Ignore => {}
                        },
                    }
                }
                CollectedChangeKind::Add | CollectedChangeKind::Modify => {
                    let content = match std::fs::read(&change.path) {
                        Ok(content) => content,
                        Err(err) => match behavior {
                            FailureBehavior::Error => {
                                return Err(CheckpointError::Io(std::io::Error::other(format!(
                                    "failed to read changed file '{relative}': {err}"
                                ))));
                            }
                            FailureBehavior::Warn => {
                                tracing::warn!("failed to read changed file '{relative}': {err}");
                                continue;
                            }
                            FailureBehavior::Ignore => continue,
                        },
                    };
                    match self.apply_agent_edit(actor, &relative, &content) {
                        Ok(_) => applied += 1,
                        Err(err) => match behavior {
                            FailureBehavior::Error => return Err(err),
                            FailureBehavior::Warn => {
                                tracing::warn!("failed to apply edit of '{relative}': {err}")
                            }
                            FailureBehavior::Ignore => {}
                        },
                    }
                }
            }
        }
        Ok(applied)
    }

    /// Discard an execution's file changes: revert the actor partition
    /// pointer to its parent (best-effort), delete the actor partition
    /// entirely (partition + history rows), and drop the in-memory
    /// projection index. Snapshots/deltas remain as immutable history
    /// (INSERT-ONLY, GC'd separately). No-op when the actor has no
    /// partition.
    pub fn discard_execution(&self, entity_id: &str) -> Result<(), CheckpointError> {
        let storage = self.storage_ref()?;
        let actor = self.actor_id_for(entity_id);
        let agent_id = actor.to_agent_instance_id();
        let pid = agent::agent_partition_id(&agent_id);
        if storage.get_partition(&pid).is_err() {
            return Ok(());
        }
        // Best-effort pointer revert; there is nothing to revert when the
        // partition has no parent (the seed snapshot), and the partition is
        // deleted right after anyway.
        let _ = agent::discard_agent_edit(storage, &agent_id);
        storage
            .delete_partition(&pid)
            .map_err(map_layertwine_error)?;
        self.latest_checkpoints.remove(actor.as_str());
        self.deleted_files.remove(actor.as_str());
        Ok(())
    }

    /// Shared registry of recent agent writes (the manual watcher reads it).
    pub fn recent_agent_writes(&self) -> &Arc<RecentAgentWrites> {
        &self.recent_agent_writes
    }

    /// File paths currently marked deleted for the actor (keyed by the
    /// actor id string, e.g. `checkpoint.metadata.author`).
    pub fn deleted_files(&self, author: &str) -> HashSet<String> {
        self.deleted_files
            .get(author)
            .map(|set| set.clone())
            .unwrap_or_default()
    }

    // ── workspace context (script diff / manual watcher scope) ──────

    /// The workspace root the manager is bound to, when configured.
    pub fn workspace_root(&self) -> Option<&Path> {
        self.workspace_root.as_deref()
    }

    /// The workspace scan rules (ignore patterns + per-file failure
    /// behavior) derived from the file-checkpoint config.
    pub fn scan_config(&self) -> &ScanConfig {
        &self.scan_config
    }

    /// Per-file failure behavior of workspace operations (scan/capture/
    /// restore), from `FileCheckpointConfig.failure_behavior`.
    pub fn failure_behavior(&self) -> FailureBehavior {
        self.scan_config.failure_behavior
    }

    /// Build a scoped change collector over the workspace root for the
    /// given `allowed_write` prefixes (from `PathPolicy.allowed_write`).
    /// `None` when no workspace root is configured or the scope is empty
    /// (no capture happens).
    pub fn collector_for(&self, allowed_write: &[String]) -> Option<WorkspaceChangeCollector> {
        let base = self.workspace_root.as_ref()?;
        let scanner = WorkspaceScanner::new(self.scan_config.clone());
        let collector = WorkspaceChangeCollector::new(base, allowed_write, scanner);
        if collector.has_scope() {
            Some(collector)
        } else {
            None
        }
    }

    /// Route watcher events into the manual partition, skipping agent
    /// self-writes: Add/Change records are hashed and compared against the
    /// recent-agent-writes registry (deterministic primary criterion) plus
    /// the 100ms post-write grace window (belt-and-braces); matching records
    /// are agent's own writes already recorded via `apply_agent_edit`.
    /// Unlink records map to the manual deletion semantics (empty content).
    /// Returns the number of applied manual edits.
    pub fn process_manual_changes(
        &self,
        records: &[FileChangeRecord],
    ) -> Result<usize, CheckpointError> {
        let Some(base) = self.workspace_root.as_ref() else {
            return Ok(0);
        };
        let mut applied = 0;
        for record in records {
            let Ok(relative) = record.path.strip_prefix(base) else {
                continue;
            };
            let relative = relative.to_string_lossy().replace('\\', "/");
            match record.kind {
                FileChangeKind::Unlink => {
                    if self.recent_agent_writes.is_recent_write(&record.path) {
                        continue;
                    }
                    self.apply_manual_edit(&relative, b"")?;
                    applied += 1;
                }
                FileChangeKind::Add | FileChangeKind::Change => {
                    if self.recent_agent_writes.is_recent_write(&record.path) {
                        continue;
                    }
                    let Ok(content) = std::fs::read(&record.path) else {
                        // File was removed between the event and processing.
                        continue;
                    };
                    let hash = sha256_hex(&content);
                    if self.recent_agent_writes.is_agent_write(&record.path, &hash) {
                        continue;
                    }
                    self.apply_manual_edit(&relative, &content)?;
                    applied += 1;
                }
            }
        }
        Ok(applied)
    }

    fn checkpoint_states(
        storage: &SqliteStorage,
        checkpoint: &Checkpoint,
    ) -> Result<Vec<(String, Vec<u8>, i64)>, CheckpointError> {
        let mut states = Vec::with_capacity(checkpoint.baseline_snapshots.len());
        for snapshot_id in &checkpoint.baseline_snapshots {
            let snapshot = storage
                .get_snapshot(snapshot_id)
                .map_err(map_layertwine_error)?;
            let path = snapshot_file_path(storage, &snapshot)?;
            if path == SEED_PATH {
                continue;
            }
            let bytes = snapshot_content_bytes(storage, &snapshot)?;
            states.push((path, bytes, snapshot.created_at));
        }
        Ok(states)
    }

    // ── checkpoint creation ─────────────────────────────────────────

    /// Create a file checkpoint for an entity: apply each entry as an agent
    /// edit on the actor partition, snapshot the partition state into a
    /// layertwine `Checkpoint` (`metadata.author = ActorId`, parent = the
    /// actor's previous checkpoint, forming a linear commit chain) and
    /// return the projection.
    pub fn create_checkpoint(
        &self,
        entity_id: &str,
        entries: &[FileContentEntry],
    ) -> Result<FileCheckpoint, CheckpointError> {
        let storage = self.storage_ref()?;
        let actor = self.actor_id_for(entity_id);
        let agent_id = actor.to_agent_instance_id();
        self.ensure_agent_partition(&actor)?;
        for entry in entries {
            self.apply_agent_edit(&actor, &entry.path, &entry.content)?;
        }
        let partition = storage
            .get_partition(&agent::agent_partition_id(&agent_id))
            .map_err(map_layertwine_error)?;
        let baseline_snapshots = partition_latest_snapshot_ids(storage, &partition)?;
        let parents = self
            .latest_checkpoint_id(storage, &actor)?
            .into_iter()
            .filter_map(|id| CheckpointId::from_hex(&id))
            .collect();
        let checkpoint = Checkpoint::new(
            baseline_snapshots,
            parents,
            CheckpointMetadata::new(actor.as_str(), "file checkpoint"),
        );
        storage
            .store_checkpoint(&checkpoint)
            .map_err(map_layertwine_error)?;
        self.latest_checkpoints
            .insert(actor.as_str().to_string(), checkpoint.id.to_hex());
        self.project(storage, &checkpoint)
    }

    /// Content-level checkpoint: alias of [`FileCheckpointManager::create_checkpoint`].
    pub fn create_checkpoint_with_content(
        &self,
        entity_id: &str,
        entries: &[FileContentEntry],
    ) -> Result<FileCheckpoint, CheckpointError> {
        self.create_checkpoint(entity_id, entries)
    }

    /// Create a file checkpoint for an entity from the actor partition's
    /// current state (the deferred snapshot path used by async
    /// persistence). Returns `None` when the entity has no file history yet.
    pub fn create_latest_file_checkpoint(
        &self,
        entity_id: &str,
    ) -> Result<Option<FileCheckpoint>, CheckpointError> {
        let storage = self.storage_ref()?;
        let actor = self.actor_id_for(entity_id);
        let agent_id = actor.to_agent_instance_id();
        let partition = match storage.get_partition(&agent::agent_partition_id(&agent_id)) {
            Ok(p) => p,
            Err(_) => return Ok(None),
        };
        if partition.history.len() <= 1 {
            return Ok(None);
        }
        let baseline_snapshots = partition_latest_snapshot_ids(storage, &partition)?;
        let parents = self
            .latest_checkpoint_id(storage, &actor)?
            .into_iter()
            .filter_map(|id| CheckpointId::from_hex(&id))
            .collect();
        let checkpoint = Checkpoint::new(
            baseline_snapshots,
            parents,
            CheckpointMetadata::new(actor.as_str(), "file checkpoint"),
        );
        storage
            .store_checkpoint(&checkpoint)
            .map_err(map_layertwine_error)?;
        self.latest_checkpoints
            .insert(actor.as_str().to_string(), checkpoint.id.to_hex());
        Ok(Some(self.project(storage, &checkpoint)?))
    }

    // ── restore ─────────────────────────────────────────────────────

    fn load_checkpoint(
        &self,
        storage: &SqliteStorage,
        checkpoint_id: &str,
    ) -> Result<Checkpoint, CheckpointError> {
        let id =
            CheckpointId::from_hex(checkpoint_id).ok_or_else(|| CheckpointError::Validation {
                reason: format!("invalid checkpoint id '{}'", checkpoint_id),
            })?;
        let exists = storage
            .checkpoint_exists(&id)
            .map_err(map_layertwine_error)?;
        if !exists {
            return Err(CheckpointError::NotFound {
                id: checkpoint_id.to_string(),
            });
        }
        storage.get_checkpoint(&id).map_err(map_layertwine_error)
    }

    /// Projection of a layertwine checkpoint with the actor's deletion
    /// markers applied.
    fn project(
        &self,
        storage: &SqliteStorage,
        checkpoint: &Checkpoint,
    ) -> Result<FileCheckpoint, CheckpointError> {
        let deleted = self.deleted_files(&checkpoint.metadata.author);
        projection(storage, checkpoint, &deleted)
    }

    /// Resolve the file set at a layertwine checkpoint (projection).
    pub fn restore_checkpoint(
        &self,
        _entity_id: &str,
        checkpoint_id: &str,
    ) -> Result<Vec<FileState>, CheckpointError> {
        let storage = self.storage_ref()?;
        let checkpoint = self.load_checkpoint(storage, checkpoint_id)?;
        let states = Self::checkpoint_states(storage, &checkpoint)?;
        let deleted = self.deleted_files(&checkpoint.metadata.author);
        Ok(states
            .into_iter()
            .map(|(path, content, ts)| FileState {
                deleted: deleted.contains(&path),
                path,
                hash: sha256_hex(&content),
                size: content.len() as u64,
                last_modified: ts,
            })
            .collect())
    }

    fn latest_checkpoint_id(
        &self,
        storage: &SqliteStorage,
        actor: &ActorId,
    ) -> Result<Option<String>, CheckpointError> {
        let actor_str = actor.as_str().to_string();
        if let Some(id) = self.latest_checkpoints.get(&actor_str) {
            return Ok(Some(id.clone()));
        }
        // Cross-process fallback: scan stored checkpoints by author.
        let checkpoints = storage.list_checkpoints().map_err(map_layertwine_error)?;
        let latest = checkpoints
            .iter()
            .filter(|c| c.metadata.author == actor_str)
            .max_by_key(|c| c.created_at);
        Ok(latest.map(|c| c.id.to_hex()))
    }

    /// Restore the latest file checkpoint for an entity, if any.
    pub fn restore_latest(
        &self,
        entity_id: &str,
    ) -> Result<Option<Vec<FileState>>, CheckpointError> {
        let storage = self.storage_ref()?;
        let actor = self.actor_id_for(entity_id);
        match self.latest_checkpoint_id(storage, &actor)? {
            Some(id) => Ok(Some(self.restore_checkpoint(entity_id, &id)?)),
            None => Ok(None),
        }
    }

    /// Content-level rollback: write the files of `checkpoint_id` back to
    /// disk. Relative paths are resolved under `base_dir`; paths escaping
    /// `base_dir` are rejected (rollback must never write outside the
    /// working tree). Returns the list of written paths.
    pub fn restore_content(
        &self,
        checkpoint_id: &str,
        base_dir: &Path,
    ) -> Result<Vec<String>, CheckpointError> {
        let storage = self.storage_ref()?;
        let checkpoint = self.load_checkpoint(storage, checkpoint_id)?;
        let states = Self::checkpoint_states(storage, &checkpoint)?;
        let deleted = self.deleted_files(&checkpoint.metadata.author);
        let mut written = Vec::with_capacity(states.len());
        for (path, content, _) in states {
            if deleted.contains(&path) {
                continue;
            }
            let target = resolve_restore_target(base_dir, &path)?;
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&target, &content)?;
            written.push(target.to_string_lossy().into_owned());
        }
        Ok(written)
    }

    /// Content-level rollback to the latest checkpoint of an entity, if any.
    pub fn restore_latest_content(
        &self,
        entity_id: &str,
        base_dir: &Path,
    ) -> Result<Option<Vec<String>>, CheckpointError> {
        let storage = self.storage_ref()?;
        let actor = self.actor_id_for(entity_id);
        match self.latest_checkpoint_id(storage, &actor)? {
            Some(id) => Ok(Some(self.restore_content(&id, base_dir)?)),
            None => Ok(None),
        }
    }

    /// Workspace-aligned restore: write back the checkpoint's files, delete
    /// extra files not part of the target set (hardcoded-ignored paths like
    /// `.git` / `node_modules` are protected), and recreate empty
    /// directories. Per-file failures follow `failure_behavior`.
    pub fn restore_workspace(
        &self,
        _entity_id: &str,
        checkpoint_id: &str,
        base_dir: &Path,
        opts: &FileCheckpointOptions,
    ) -> Result<WorkspaceRestoreResult, CheckpointError> {
        let storage = self.storage_ref()?;
        let checkpoint = self.load_checkpoint(storage, checkpoint_id)?;
        let states = Self::checkpoint_states(storage, &checkpoint)?;
        let deleted = self.deleted_files(&checkpoint.metadata.author);
        let target_map: HashMap<String, FileState> = states
            .iter()
            .filter(|(path, _, _)| !deleted.contains(path))
            .map(|(path, content, ts)| {
                (
                    path.clone(),
                    FileState {
                        path: path.clone(),
                        hash: sha256_hex(content),
                        size: content.len() as u64,
                        last_modified: *ts,
                        deleted: false,
                    },
                )
            })
            .collect();

        let scanner = WorkspaceScanner::new(ScanConfig {
            custom_ignore_patterns: opts.custom_ignore_patterns.clone(),
            failure_behavior: opts.failure_behavior,
        });
        let current = scanner.scan(base_dir)?;
        let current_map: HashMap<String, FileState> = current
            .files
            .iter()
            .map(|f| (f.path.clone(), f.clone()))
            .collect();

        let mut result = WorkspaceRestoreResult::default();

        // Restore target files: skip identical content, write the rest.
        for state in target_map.values() {
            let target = resolve_restore_target(base_dir, &state.path)?;
            let current_hash = current_map.get(&state.path).map(|f| f.hash.as_str());
            if current_hash == Some(state.hash.as_str()) {
                result.skipped += 1;
                continue;
            }
            let content = states
                .iter()
                .find(|(path, _, _)| path == &state.path)
                .map(|(_, content, _)| content)
                .ok_or_else(|| CheckpointError::NotFound {
                    id: format!("file content for {}", state.path),
                })?;
            match write_file_with_dirs(&target, content) {
                Ok(()) => result.restored += 1,
                Err(err) => handle_restore_failure(opts.failure_behavior, &state.path, &err)?,
            }
        }

        // Delete extra files not in the target set, protecting hardcoded
        // ignored paths.
        let mut extras: Vec<String> = current
            .files
            .iter()
            .map(|f| f.path.clone())
            .filter(|path| !target_map.contains_key(path) && !is_hardcoded_ignored(path))
            .collect();
        extras.sort();
        for path in extras {
            let target = resolve_restore_target(base_dir, &path)?;
            match std::fs::remove_file(&target) {
                Ok(()) => result.deleted += 1,
                Err(err) => handle_restore_failure(opts.failure_behavior, &path, &err)?,
            }
        }

        // Recreate empty directories recorded at snapshot time.
        if let Some(empty_dirs) = self.empty_dirs.get(checkpoint_id) {
            for empty_dir in empty_dirs.iter() {
                let dir = base_dir.join(empty_dir);
                match std::fs::create_dir_all(&dir) {
                    Ok(()) => {}
                    Err(err) => {
                        handle_restore_failure(opts.failure_behavior, empty_dir, &err)?;
                    }
                }
            }
        }

        Ok(result)
    }

    /// Workspace-aligned restore to the latest checkpoint of an entity, if
    /// any.
    pub fn restore_latest_workspace(
        &self,
        entity_id: &str,
        base_dir: &Path,
        opts: &FileCheckpointOptions,
    ) -> Result<Option<WorkspaceRestoreResult>, CheckpointError> {
        let storage = self.storage_ref()?;
        let actor = self.actor_id_for(entity_id);
        match self.latest_checkpoint_id(storage, &actor)? {
            Some(id) => Ok(Some(
                self.restore_workspace(entity_id, &id, base_dir, opts)?,
            )),
            None => Ok(None),
        }
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

    // ── layered merge wrappers ────────────────────────────────────────

    fn ensure_approval_ready(
        &self,
        storage: &SqliteStorage,
        agent_id: &AgentInstanceId,
    ) -> Result<(), CheckpointError> {
        let pid = agent::agent_partition_id(agent_id);
        let partition = storage.get_partition(&pid).map_err(map_layertwine_error)?;
        let baseline =
            partition
                .history
                .first()
                .copied()
                .ok_or_else(|| CheckpointError::Corrupted {
                    id: pid.to_string(),
                    reason: "agent partition has empty history".to_string(),
                })?;
        layertwine::layered::approval::ensure_approval_agent_partition(storage, agent_id, baseline)
            .map_err(map_layertwine_error)?;
        Ok(())
    }

    fn ensure_staged_ready(&self, storage: &SqliteStorage) -> Result<(), CheckpointError> {
        let staged_pid = layertwine::layered::staged::staged_partition_id();
        if storage.get_partition(&staged_pid).is_ok() {
            return Ok(());
        }
        let seed = seed_initial_snapshot(storage, &AgentInstanceId("staged".into()))?;
        layertwine::layered::staged::ensure_staged_partition(storage, seed)
            .map_err(map_layertwine_error)?;
        Ok(())
    }

    /// Move the actor's changes into the approval layer (three-way merge
    /// against the approval baseline). Returns the approval snapshot id.
    pub fn move_agent_to_approval(&self, entity_id: &str) -> Result<String, CheckpointError> {
        let storage = self.storage_ref()?;
        let actor = self.actor_id_for(entity_id);
        let agent_id = actor.to_agent_instance_id();
        self.ensure_agent_partition(&actor)?;
        self.ensure_approval_ready(storage, &agent_id)?;
        let snapshot_id =
            agent::move_agent_to_approval(storage, &agent_id).map_err(map_layertwine_error)?;
        Ok(snapshot_id.to_hex())
    }

    /// Merge the actor's approved changes into a feature (integrated)
    /// partition via three-way merge.
    pub fn merge_agent_to_feature(
        &self,
        entity_id: &str,
        feature_name: &str,
    ) -> Result<MergeResult, CheckpointError> {
        let storage = self.storage_ref()?;
        let actor = self.actor_id_for(entity_id);
        let agent_id = actor.to_agent_instance_id();
        self.ensure_agent_partition(&actor)?;
        self.ensure_approval_ready(storage, &agent_id)?;
        layertwine::layered::integrated::merge_agent_to_feature(storage, &agent_id, feature_name)
            .map_err(map_layertwine_error)
    }

    /// Merge all given features into the staged partition (three-way merge
    /// per feature, sequential accumulation).
    pub fn merge_features_to_staged(
        &self,
        feature_names: &[&str],
    ) -> Result<MergeResult, CheckpointError> {
        let storage = self.storage_ref()?;
        self.ensure_staged_ready(storage)?;
        let names: Vec<String> = feature_names.iter().map(|s| s.to_string()).collect();
        layertwine::layered::staged::merge_features_to_staged(storage, &names)
            .map_err(map_layertwine_error)
    }

    // ── approval layer (list / approve / reject) ─────────────────────

    /// All pending approvals: actor partitions at the approval layer with
    /// more than one history entry (submitted but neither merged nor
    /// rejected). Persisted in SQLite, so pending approvals survive across
    /// executions ("review after the run ends").
    pub fn list_pending_approvals(&self) -> Result<Vec<PendingApproval>, CheckpointError> {
        let storage = self.storage_ref()?;
        let pending = layertwine::layered::approval::list_pending_approvals(storage)
            .map_err(map_layertwine_error)?;
        let mut views = Vec::with_capacity(pending.len());
        for partition in pending {
            let actor = match &partition.partition_type {
                layertwine::core::types::PartitionType::Approval(id) => id.0.clone(),
                other => other.name(),
            };
            let last_id =
                partition
                    .history
                    .last()
                    .copied()
                    .ok_or_else(|| CheckpointError::Corrupted {
                        id: partition.id.to_string(),
                        reason: "pending approval partition has empty history".to_string(),
                    })?;
            let last_snapshot = storage
                .get_snapshot(&last_id)
                .map_err(map_layertwine_error)?;
            let mut changes = Vec::new();
            for snapshot_id in &partition.history {
                let snapshot = storage
                    .get_snapshot(snapshot_id)
                    .map_err(map_layertwine_error)?;
                if let Some(summary) =
                    crate::provenance::DeltaSummary::from_snapshot(storage, &snapshot)?
                {
                    changes.push(summary);
                }
            }
            views.push(PendingApproval {
                actor,
                snapshot_id: last_id.to_hex(),
                submitted_at: last_snapshot.created_at,
                changes,
            });
        }
        views.sort_by_key(|a| a.submitted_at);
        Ok(views)
    }

    /// Reject a pending approval: roll the actor's approval partition back to
    /// its baseline. Returns the baseline snapshot id (hex).
    pub fn reject_changes(&self, entity_id: &str) -> Result<String, CheckpointError> {
        let storage = self.storage_ref()?;
        let actor = self.actor_id_for(entity_id);
        let agent_id = actor.to_agent_instance_id();
        let baseline = layertwine::layered::approval::reject_approval(storage, &agent_id)
            .map_err(map_layertwine_error)?;
        Ok(baseline.to_hex())
    }

    /// Approve an actor's pending changes: merge them into the named feature
    /// and apply the configured conflict behavior.
    ///
    /// - `ConflictBehavior::Marker` (default): merged text keeps conflict
    ///   markers embedded in the affected files (written to `workspace_root`
    ///   when provided) and the outcome reports them; execution continues.
    /// - `ConflictBehavior::Fail`: any conflict aborts with an error.
    /// - `ConflictBehavior::Approval`: conflicts stay pending in the
    ///   approval layer instead of being merged.
    pub fn approve_changes(
        &self,
        entity_id: &str,
        feature_name: &str,
        conflict_behavior: ConflictBehavior,
        workspace_root: Option<&Path>,
    ) -> Result<MergeOutcome, CheckpointError> {
        let storage = self.storage_ref()?;
        let actor = self.actor_id_for(entity_id);
        let agent_id = actor.to_agent_instance_id();
        self.ensure_agent_partition(&actor)?;
        self.ensure_approval_ready(storage, &agent_id)?;

        let submitted = self.move_agent_to_approval(entity_id)?;
        if conflict_behavior == ConflictBehavior::Approval {
            // Route conflicted changes to the approval layer: keep the
            // submission pending and let the host resolve it.
            return Ok(MergeOutcome {
                merged: false,
                snapshot_id: submitted,
                conflicts: vec![],
                conflict_files: vec![],
                message: "changes remain pending in the approval layer".to_string(),
            });
        }

        let merged = self.merge_agent_to_feature(entity_id, feature_name)?;
        let merged_snapshot = storage
            .get_snapshot(&merged.snapshot_id)
            .map_err(map_layertwine_error)?;
        let file = crate::provenance::snapshot_file_path(storage, &merged_snapshot)?;
        let conflicts = to_conflict_views(&file, &merged.conflicts);
        let mut conflict_files: Vec<String> = conflicts
            .iter()
            .map(|c| c.file.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        conflict_files.sort();

        if !conflicts.is_empty() && conflict_behavior == ConflictBehavior::Fail {
            return Err(CheckpointError::MergeConflict {
                actor: actor.as_str().to_string(),
                files: conflict_files,
            });
        }

        if !conflicts.is_empty() && conflict_behavior == ConflictBehavior::Marker {
            if let Some(root) = workspace_root {
                let merged_text =
                    layertwine::layered::transition::reconstruct_text(storage, &merged_snapshot)
                        .map_err(map_layertwine_error)?;
                let marked = inject_conflict_markers(&merged_text, &merged.conflicts);
                let target = resolve_restore_target(root, &file)?;
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&target, marked)?;
            }
            if let Some(ref bus) = self.event_bus {
                bus.publish(CheckpointEventBus::merge_conflicted(
                    merged.snapshot_id.to_hex(),
                    conflict_files.join(", "),
                    Some(actor.as_str().to_string()),
                ));
            }
        }

        Ok(MergeOutcome {
            merged: true,
            snapshot_id: merged.snapshot_id.to_hex(),
            conflicts,
            conflict_files,
            message: if merged.has_conflicts() {
                "merged with conflicts".to_string()
            } else {
                "merged cleanly".to_string()
            },
        })
    }

    /// Full merge entry point for an actor: move to approval, then merge
    /// into the named feature (the `ApprovalPolicy::auto` path).
    pub fn merge_entity_changes(
        &self,
        entity_id: &str,
        feature_name: &str,
    ) -> Result<MergeResult, CheckpointError> {
        self.move_agent_to_approval(entity_id)?;
        self.merge_agent_to_feature(entity_id, feature_name)
    }

    // ── end-of-execution approval policy ─────────────────────────────

    /// The configured layered approval policy.
    pub fn approval_policy(&self) -> ApprovalPolicy {
        self.approval_policy
    }

    /// The configured merge conflict behavior.
    pub fn conflict_behavior(&self) -> ConflictBehavior {
        self.conflict_behavior
    }

    /// Default feature name merged into under `ApprovalPolicy::auto` (a
    /// per-execution feature keeps actors isolated while still landing the
    /// changes into the integrated layer).
    pub fn default_feature_name(entity_id: &str) -> String {
        format!("exec-{}", entity_id)
    }

    /// Agent loop end hook: applies the configured approval policy.
    ///
    /// - `None`: no-op (today's behavior; the actor partition keeps its
    ///   history and stays queryable).
    /// - `Auto`: move the actor to the approval layer and immediately merge
    ///   into the default feature partition.
    /// - `Llm` / `Manual`: move the actor to the approval layer and leave the
    ///   changes pending (`approve_changes` tool / host API resolve them,
    ///   possibly across executions).
    ///
    /// Best-effort by design: the file checkpoint layer must never break an
    /// execution that finished successfully, so failures are reported but
    /// not propagated.
    pub fn on_agent_complete(
        &self,
        entity_id: &str,
    ) -> Result<Option<MergeResult>, CheckpointError> {
        match self.approval_policy {
            ApprovalPolicy::None => Ok(None),
            ApprovalPolicy::Auto => {
                let feature = Self::default_feature_name(entity_id);
                let merged = self.merge_entity_changes(entity_id, &feature)?;
                Ok(Some(merged))
            }
            ApprovalPolicy::Llm | ApprovalPolicy::Manual => {
                self.move_agent_to_approval(entity_id)?;
                Ok(None)
            }
        }
    }

    /// Approve a pending actor's changes using the configured conflict
    /// behavior (thin wrapper over [`Self::approve_changes`]).
    pub fn approve_pending(
        &self,
        entity_id: &str,
        feature_name: &str,
    ) -> Result<MergeOutcome, CheckpointError> {
        self.approve_changes(
            entity_id,
            feature_name,
            self.conflict_behavior,
            self.workspace_root.as_deref(),
        )
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
        crate::provenance::diff_against_staged(self.storage_ref()?, actor)
    }
}

impl Default for FileCheckpointManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Map a layertwine error into the unified `CheckpointError` (no redundant
/// adapter error type). `NotFound` maps to `CheckpointError::NotFound`;
/// storage errors become `CheckpointError::Internal`.
pub(crate) fn map_layertwine_error<E: Into<layertwine::LayertwineError>>(e: E) -> CheckpointError {
    match e.into() {
        layertwine::LayertwineError::NotFound(id) => CheckpointError::NotFound { id },
        layertwine::LayertwineError::Storage(err) => match err {
            layertwine::StorageError::NotFound(id) => CheckpointError::NotFound { id },
            other => CheckpointError::Internal(format!("layertwine: {other}")),
        },
        other => CheckpointError::Internal(format!("layertwine: {other}")),
    }
}

/// The file path a snapshot applies to. Snapshot structs carry the *parent*
/// file node (chain head); the actual per-file path lives in the last delta
/// of the chain. Content snapshots (`new_with_content`, binary files) carry
/// their own file node.
fn snapshot_file_path(
    storage: &SqliteStorage,
    snapshot: &Snapshot,
) -> Result<String, CheckpointError> {
    if let Some(delta_id) = snapshot.deltas.last() {
        let delta = storage.get_delta(delta_id).map_err(map_layertwine_error)?;
        Ok(delta.file.path_str().to_string())
    } else {
        Ok(snapshot.file.path_str().to_string())
    }
}

/// Reconstruct the byte content of a snapshot: verbatim content when
/// present (binary), otherwise the line-diff reconstruction.
fn snapshot_content_bytes(
    storage: &SqliteStorage,
    snapshot: &Snapshot,
) -> Result<Vec<u8>, CheckpointError> {
    if let Some(content) = &snapshot.content {
        return Ok(content.to_bytes());
    }
    Ok(
        layertwine::layered::transition::reconstruct_text(storage, snapshot)
            .map_err(map_layertwine_error)?
            .into_bytes(),
    )
}

/// Seed a fresh initial snapshot for a partition: an empty-baseline file
/// node with an empty diff delta. Subsequent `apply_agent_edit` calls diff
/// against it, producing full-file deltas for the first edit.
fn seed_initial_snapshot(
    storage: &SqliteStorage,
    agent_id: &AgentInstanceId,
) -> Result<SnapshotId, CheckpointError> {
    let file_node = FileNode::new(PathBuf::from(SEED_PATH), b"");
    storage
        .store_file_node(&file_node, b"")
        .map_err(map_layertwine_error)?;
    let delta = Delta::new(
        file_node.clone(),
        LineDiff::new(vec![]),
        SourceType::Agent(agent_id.clone()),
    );
    storage.store_delta(&delta).map_err(map_layertwine_error)?;
    let snapshot = Snapshot::new_initial(file_node, delta.id);
    storage
        .store_snapshot(&snapshot, b"")
        .map_err(map_layertwine_error)?;
    Ok(snapshot.id)
}

/// Latest snapshot id per file path in the partition history (history
/// order, last occurrence wins). The partition's seed snapshot and any
/// later snapshots that still apply to the seed path are excluded.
fn partition_latest_snapshot_ids(
    storage: &SqliteStorage,
    partition: &Partition,
) -> Result<Vec<SnapshotId>, CheckpointError> {
    let mut last_per_path: HashMap<String, SnapshotId> = HashMap::new();
    for snapshot_id in &partition.history {
        let snapshot = storage
            .get_snapshot(snapshot_id)
            .map_err(map_layertwine_error)?;
        let path = snapshot_file_path(storage, &snapshot)?;
        last_per_path.insert(path, *snapshot_id);
    }
    let mut seen = std::collections::HashSet::new();
    let mut ids = Vec::new();
    for snapshot_id in &partition.history {
        let snapshot = storage
            .get_snapshot(snapshot_id)
            .map_err(map_layertwine_error)?;
        let path = snapshot_file_path(storage, &snapshot)?;
        if last_per_path.get(&path) == Some(snapshot_id) && path != SEED_PATH && seen.insert(path) {
            ids.push(*snapshot_id);
        }
    }
    Ok(ids)
}

/// Build the projection of a layertwine checkpoint.
fn projection(
    storage: &SqliteStorage,
    checkpoint: &Checkpoint,
    deleted: &HashSet<String>,
) -> Result<FileCheckpoint, CheckpointError> {
    let states = FileCheckpointManager::checkpoint_states(storage, checkpoint)?;
    let mut files = Vec::with_capacity(states.len());
    for (path, content, ts) in states {
        files.push(FileState {
            deleted: deleted.contains(&path),
            path,
            hash: sha256_hex(&content),
            size: content.len() as u64,
            last_modified: ts,
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let full_hash = compute_full_hash(&files);
    Ok(FileCheckpoint {
        id: checkpoint.id.to_hex(),
        timestamp: checkpoint.created_at,
        full_hash,
        files,
        checkpoint_type: "full".to_string(),
        base_checkpoint_id: None,
        changes: None,
        empty_dirs: None,
    })
}

/// SHA-256 of the sorted `path=hash;` pairs (stable workspace fingerprint).
fn compute_full_hash(files: &[FileState]) -> String {
    let mut parts: Vec<&FileState> = files.iter().collect();
    parts.sort_by(|a, b| a.path.cmp(&b.path));
    let mut hasher = Sha256::new();
    for f in parts {
        hasher.update(f.path.as_bytes());
        hasher.update(b"=");
        hasher.update(f.hash.as_bytes());
        hasher.update(b";");
    }
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>()
}

fn write_file_with_dirs(target: &Path, content: &[u8]) -> Result<(), std::io::Error> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(target, content)
}

fn handle_restore_failure(
    behavior: FailureBehavior,
    path: &str,
    err: &std::io::Error,
) -> Result<(), CheckpointError> {
    match behavior {
        FailureBehavior::Error => Err(CheckpointError::Io(std::io::Error::other(format!(
            "failed to restore '{path}': {err}"
        )))),
        FailureBehavior::Warn => {
            tracing::warn!("failed to restore '{path}': {err}");
            Ok(())
        }
        FailureBehavior::Ignore => Ok(()),
    }
}

/// Resolve the on-disk target for a restored file state. Absolute paths are
/// used as-is (they were recorded from the original location); relative
/// paths are joined onto `base_dir` and must stay inside it.
fn resolve_restore_target(base_dir: &Path, path: &str) -> Result<PathBuf, CheckpointError> {
    let candidate = PathBuf::from(path);
    if candidate.is_absolute() {
        return Ok(candidate);
    }
    let joined = base_dir.join(&candidate);
    let normalized = joined
        .canonicalize()
        .unwrap_or_else(|_| normalize_lexically(&joined));
    let base = base_dir
        .canonicalize()
        .unwrap_or_else(|_| normalize_lexically(base_dir));
    if !normalized.starts_with(&base) {
        return Err(CheckpointError::Validation {
            reason: format!(
                "file checkpoint path '{}' escapes base directory '{}'",
                path,
                base_dir.display()
            ),
        });
    }
    Ok(joined)
}

/// Lexical normalization without touching the filesystem (used when the
/// target does not exist yet, so `canonicalize` cannot resolve it).
fn normalize_lexically(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Fallback root actor for a bare execution id (agent kind).
fn root_actor(execution_id: wf_types::Id) -> ActorId {
    ActorId::new(ActorKind::Agent, &[execution_id]).unwrap_or_else(|_| {
        ActorId::new(ActorKind::Agent, &[wf_types::Id::from("unknown")]).unwrap()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(!result.has_conflicts());
        assert!(!result.snapshot_id.to_hex().is_empty());

        let staged = manager.merge_features_to_staged(&["feature-1"]).unwrap();
        assert!(!staged.has_conflicts());
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
}
