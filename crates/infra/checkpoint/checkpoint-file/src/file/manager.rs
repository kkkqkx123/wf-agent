mod capture;
mod lifecycle;
mod queries;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_b;
mod types;

use std::path::PathBuf;
use std::sync::Arc;

use dashmap::DashMap;

use crate::event::CheckpointEventBus;
use crate::manager_store::{ManagerPolicy, ManagerStore};
use checkpoint_base::actor::cache::ActorCache;
use checkpoint_base::clock::CheckpointClock;

pub use wf_types::config::file_checkpoint::ApprovalPolicy;

pub use types::{
    FileCheckpoint, FileCheckpointMetadata, FileCheckpointOptions, FileContentEntry,
    FileProjection, FileState, WorkspaceRestoreResult,
};

/// File checkpoint engine rebuilt on top of the Git object store
/// (authoritative model). The manager drives ref operations directly:
/// actor edit refs hold per-actor file edits (`apply_agent_edit`),
/// review/feature/main refs carry the submit/merge DAG, and restore
/// expands target trees. `FileCheckpoint` / `FileState` are projections
/// over the commit graph.
pub struct FileCheckpointManager {
    pub(crate) store: ManagerStore,
    pub(crate) policy: ManagerPolicy,
    /// Time source for checkpoint creation timestamps. Tests inject a
    /// manual clock and advance it explicitly instead of sleeping.
    pub(crate) clock: CheckpointClock,
    /// Change-event feed: every recorded agent/manual edit publishes a
    /// `CheckpointEvent::FileChanged`. Absent when the manager is used
    /// without an event layer.
    pub(crate) event_bus: Option<CheckpointEventBus>,
    /// Workspace root the manager is bound to (from `FileCheckpointConfig`).
    /// Scoped captures (script diff, manual watcher) restrict their scope to
    /// this root; `None` disables them.
    pub(crate) workspace_root: Option<PathBuf>,
    /// Global default execution scope (from `FileCheckpointConfig`).
    /// Per-execution scopes inherit from it when they leave fields missing.
    pub(crate) default_scope: Option<wf_types::config::file_scope::ExecutionFileScope>,
    /// Entity id -> resolved `ActorId` (sub-execution isolation). Built at
    /// first actor resolution: a child execution whose parent is known in
    /// the index gets `parent.child(execution_id)`, so nested executions
    /// live in their own hierarchical partition.
    pub(crate) actor_index: ActorCache,
    /// Shared scoped-shell sampling state (foreground scopes + background
    /// sessions). Owned here so every `CheckpointSession` clone routes to
    /// the same registry instead of isolated per-handle maps.
    pub(crate) session_scopes: Arc<crate::scope::SessionScopeRegistry>,
    /// Staged multi-file operations awaiting their single atomic commit,
    /// keyed by session id string. Grouping is a commit trailer, never a
    /// persistence row. In-memory only: a restart drops staged groups while
    /// committed session trailers survive in the commit graph.
    pub(crate) pending_batches: Arc<DashMap<String, crate::file::git_write::PendingEditBatch>>,
    /// Undone edit-ref heads per actor, for redo after a local undo.
    /// Memory-cached with a SQLite mirror so redo survives a restart while
    /// committed history stays reachable from the refs.
    pub(crate) redo_stacks: Arc<crate::file::redo_store::RedoStackStore>,
    /// Unified checkpoint metrics collector. Clones share the same slot so
    /// a collector attached after construction still observes every handle.
    /// Absent collectors add zero overhead.
    checkpoint_metrics: Arc<std::sync::Mutex<Option<Arc<wf_metrics::CheckpointMetricsCollector>>>>,
}

impl Clone for FileCheckpointManager {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            policy: self.policy.clone(),
            clock: self.clock.clone(),
            event_bus: self.event_bus.clone(),
            workspace_root: self.workspace_root.clone(),
            default_scope: self.default_scope.clone(),
            actor_index: self.actor_index.clone(),
            session_scopes: self.session_scopes.clone(),
            pending_batches: self.pending_batches.clone(),
            redo_stacks: self.redo_stacks.clone(),
            checkpoint_metrics: self.checkpoint_metrics.clone(),
        }
    }
}

impl FileCheckpointManager {
    /// Global default execution scope inherited by executions without an
    /// explicit override.
    pub fn default_scope(&self) -> Option<&wf_types::config::file_scope::ExecutionFileScope> {
        self.default_scope.as_ref()
    }

    /// Whether this manager's checkpoint store is self-contained. Borrowed
    /// baselines depend on an external repository and are never fully
    /// self-contained.
    pub fn self_contained(&self) -> bool {
        !matches!(
            self.policy.baseline_mode,
            Some(wf_types::config::file_checkpoint::BaselineMode::Borrow)
        )
    }

    /// Human-readable reason when the store is not self-contained.
    pub fn self_contained_blocker(&self) -> Option<String> {
        if self.self_contained() {
            return None;
        }
        Some(
            "baseline mode is borrow; checkpoint objects depend on the external repository"
                .to_string(),
        )
    }

    /// Attach the unified checkpoint metrics collector. Creation, restore
    /// and cleanup paths record into it; absent collectors add zero
    /// overhead. Shared across clones, so late attachment still observes
    /// every handle.
    pub fn set_checkpoint_metrics(&self, metrics: Arc<wf_metrics::CheckpointMetricsCollector>) {
        *wf_common::lock::lock_ok(self.checkpoint_metrics.lock()) = Some(metrics);
    }

    /// Attached metrics collector for observability of best-effort paths.
    pub fn checkpoint_metrics_for_observability(
        &self,
    ) -> Option<Arc<wf_metrics::CheckpointMetricsCollector>> {
        wf_common::lock::lock_ok(self.checkpoint_metrics.lock()).clone()
    }

    pub(crate) fn checkpoint_metrics(&self) -> Option<Arc<wf_metrics::CheckpointMetricsCollector>> {
        self.checkpoint_metrics_for_observability()
    }
}
