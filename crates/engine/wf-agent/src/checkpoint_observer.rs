//! Actor-aware file-checkpoint adapter for agent tool calls.
//!
//! Implements the business-free [`wf_tools::ToolSideEffectObserver`] with a
//! [`wf_checkpoint::FileCheckpointManager`]: precise file-tool events are
//! applied into the agent actor partition, and shell scopes are diffed only
//! inside the workspace intersection. The tool layer never sees checkpoint
//! types; this upper crate owns both sides and injects the adapter into
//! every [`wf_tools::executor::trait_def::ToolExecutionContext`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use dashmap::DashMap;
use wf_checkpoint::{ActorId, FileCheckpointManager, PreciseFileEvent, PreciseFileEventKind};
use wf_tools::{
    PreciseFileChange, PreciseFileOp, ScopeOutcome, SessionBoundary, ToolSideEffectObserver,
};
use wf_types::config::file_checkpoint::FailureBehavior;

/// Resolve the shell scope intersection: the requested directory intersected
/// with the workspace root. Returns `None` for out-of-workspace execution
/// (explicit "no synchronous capture range", never a whole-workspace scan).
/// When the requested scope encloses the workspace, the scope is narrowed to
/// the workspace root.
pub fn resolve_shell_scope(workspace_root: &Path, scope_dir: &Path) -> Option<PathBuf> {
    let ws = wf_checkpoint::normalize_absolute_path(workspace_root);
    let scope = wf_checkpoint::normalize_absolute_path(scope_dir);
    if scope.starts_with(&ws) {
        Some(scope)
    } else if ws.starts_with(&scope) {
        Some(ws)
    } else {
        None
    }
}

/// Actor-aware observer injected into agent tool contexts.
pub struct AgentCheckpointObserver {
    manager: FileCheckpointManager,
    actor: ActorId,
    workspace_root: Option<PathBuf>,
    behavior: FailureBehavior,
    entity_id: String,
    /// `execution_id|scope` -> before hashes for foreground scoped runs.
    scoped_before: Arc<DashMap<String, HashMap<PathBuf, String>>>,
    /// session_id -> before hashes for background sessions.
    session_before: Arc<DashMap<String, HashMap<PathBuf, String>>>,
    /// session_id -> resolved scope dir.
    session_scope: Arc<DashMap<String, PathBuf>>,
}

impl AgentCheckpointObserver {
    /// Build the observer for an agent execution. The actor is resolved with
    /// the existing entity hierarchy so nested executions stay isolated.
    pub fn new(
        manager: FileCheckpointManager,
        entity_id: &str,
        parent_execution_id: Option<&str>,
    ) -> Self {
        let actor = manager.resolve_actor(entity_id, parent_execution_id);
        let workspace_root = manager.workspace_root().map(|p| p.to_path_buf());
        let behavior = manager.failure_behavior();
        Self {
            manager,
            actor,
            workspace_root,
            behavior,
            entity_id: entity_id.to_string(),
            scoped_before: Arc::new(DashMap::new()),
            session_before: Arc::new(DashMap::new()),
            session_scope: Arc::new(DashMap::new()),
        }
    }

    /// The resolved actor partition (for diagnostics / tests).
    pub fn actor(&self) -> &ActorId {
        &self.actor
    }

    fn collector_for(
        &self,
        scope: &Path,
    ) -> Option<wf_checkpoint::script_capture::WorkspaceChangeCollector> {
        let root = self.workspace_root.as_ref()?;
        let scanner = wf_checkpoint::WorkspaceScanner::new(self.manager.scan_config().clone());
        let scope_str = scope.to_string_lossy().to_string();
        let collector = wf_checkpoint::script_capture::WorkspaceChangeCollector::new(
            root,
            &[scope_str],
            scanner,
        );
        if collector.has_scope() {
            Some(collector)
        } else {
            None
        }
    }

    fn capture_scope(&self, scope: &Path) -> Option<HashMap<PathBuf, String>> {
        let collector = self.collector_for(scope)?;
        match collector.capture() {
            Ok(map) => Some(map),
            Err(err) => {
                tracing::warn!(
                    entity = %self.entity_id,
                    scope = %scope.display(),
                    error = %err,
                    "scoped capture (before/after) failed; sampling marked incomplete"
                );
                None
            }
        }
    }

    fn apply_scoped_diff(
        &self,
        scope: &Path,
        before: &HashMap<PathBuf, String>,
        execution_id: &str,
    ) {
        let Some(root) = self.workspace_root.as_ref() else {
            tracing::debug!(
                entity = %self.entity_id,
                execution = %execution_id,
                "shell scope outside workspace checkpoint range; no synchronous capture"
            );
            return;
        };
        let collector = match self.collector_for(scope) {
            Some(c) => c,
            None => {
                tracing::debug!(
                    entity = %self.entity_id,
                    execution = %execution_id,
                    scope = %scope.display(),
                    "empty shell scope; no synchronous capture"
                );
                return;
            }
        };
        let after = match collector.capture() {
            Ok(map) => map,
            Err(err) => {
                tracing::warn!(
                    entity = %self.entity_id,
                    execution = %execution_id,
                    scope = %scope.display(),
                    stage = "after",
                    error = %err,
                    "scoped after-capture failed"
                );
                return;
            }
        };
        let changes = wf_checkpoint::script_capture::WorkspaceChangeCollector::diff(before, &after);
        if changes.is_empty() {
            return;
        }
        match self
            .manager
            .apply_workspace_changes(&self.actor, root, &changes, self.behavior)
        {
            Ok(applied) => {
                tracing::debug!(
                    entity = %self.entity_id,
                    execution = %execution_id,
                    scope = %scope.display(),
                    applied,
                    total = changes.len(),
                    "scoped shell diff applied into agent partition"
                );
            }
            Err(err) => {
                tracing::warn!(
                    entity = %self.entity_id,
                    execution = %execution_id,
                    scope = %scope.display(),
                    error = %err,
                    "scoped shell diff apply failed"
                );
            }
        }
        // Resolve in-flight leases with final content so later watcher
        // events match by hash instead of being deferred forever.
        for change in &changes {
            let path = &change.path;
            match std::fs::read(path) {
                Ok(content) => {
                    let hash = wf_checkpoint::sha256_hex(&content);
                    self.manager
                        .recent_agent_writes()
                        .resolve_inflight(path.clone(), hash, false);
                }
                Err(_) => {
                    self.manager.recent_agent_writes().resolve_inflight(
                        path.clone(),
                        String::new(),
                        true,
                    );
                }
            }
        }
    }

    fn scoped_key(execution_id: &str, scope: &Path) -> String {
        format!("{}|{}", execution_id, scope.display())
    }
}

impl ToolSideEffectObserver for AgentCheckpointObserver {
    fn notify_precise(&self, change: PreciseFileChange) {
        let Some(root) = self.workspace_root.clone() else {
            tracing::debug!(
                entity = %self.entity_id,
                execution = %change.execution_id,
                path = %change.path.display(),
                "precise file event outside workspace checkpoint range (no root)"
            );
            return;
        };
        let kind = match &change.op {
            PreciseFileOp::Created => PreciseFileEventKind::Created,
            PreciseFileOp::Modified => PreciseFileEventKind::Modified,
            PreciseFileOp::Deleted => PreciseFileEventKind::Deleted,
            PreciseFileOp::Renamed { from } => PreciseFileEventKind::Renamed { from: from.clone() },
        };
        let event = PreciseFileEvent::new(change.path.clone(), kind);
        match self.manager.apply_precise_file_events(
            &self.actor,
            &root,
            std::slice::from_ref(&event),
            self.behavior,
        ) {
            Ok(stats) => {
                if !stats.out_of_scope.is_empty() {
                    tracing::debug!(
                        entity = %self.entity_id,
                        execution = %change.execution_id,
                        path = %change.path.display(),
                        "precise file event outside workspace; recorded as out-of-scope execution result"
                    );
                }
                if !stats.failed.is_empty() {
                    tracing::warn!(
                        entity = %self.entity_id,
                        execution = %change.execution_id,
                        path = %change.path.display(),
                        "precise file event apply failed"
                    );
                }
            }
            Err(err) => {
                tracing::warn!(
                    entity = %self.entity_id,
                    execution = %change.execution_id,
                    path = %change.path.display(),
                    error = %err,
                    "precise file event apply failed"
                );
            }
        }
    }

    fn notify_scope_begin(&self, execution_id: &str, scope_dir: &Path) {
        let Some(root) = self.workspace_root.as_ref() else {
            tracing::debug!(
                entity = %self.entity_id,
                execution = %execution_id,
                scope = %scope_dir.display(),
                "foreground shell outside workspace checkpoint range"
            );
            return;
        };
        let Some(scope) = resolve_shell_scope(root, scope_dir) else {
            tracing::debug!(
                entity = %self.entity_id,
                execution = %execution_id,
                scope = %scope_dir.display(),
                "foreground shell scope has empty workspace intersection; no synchronous capture"
            );
            return;
        };
        // In-flight lease for existing files in scope so watcher events
        // during the command defer to this sampler.
        if let Some(before) = self.capture_scope(&scope) {
            for path in before.keys() {
                self.manager
                    .recent_agent_writes()
                    .acquire_inflight(path.clone());
            }
            self.scoped_before
                .insert(Self::scoped_key(execution_id, &scope), before);
        } else {
            self.scoped_before
                .insert(Self::scoped_key(execution_id, &scope), HashMap::new());
        }
    }

    fn notify_scope_end(&self, scope_dir: &Path, outcome: ScopeOutcome) {
        let Some(root) = self.workspace_root.as_ref() else {
            return;
        };
        let Some(scope) = resolve_shell_scope(root, scope_dir) else {
            tracing::debug!(
                entity = %self.entity_id,
                execution = %outcome.execution_id,
                scope = %scope_dir.display(),
                terminated = outcome.terminated,
                "foreground shell out-of-scope end; no capture"
            );
            return;
        };
        if !outcome.terminated {
            tracing::warn!(
                entity = %self.entity_id,
                execution = %outcome.execution_id,
                scope = %scope.display(),
                detail = ?outcome.detail,
                "shell process may still be alive; sampling marked incomplete, not complete"
            );
            // Keep the before snapshot so a later confirmed end can still
            // diff; release nothing yet.
            return;
        }
        let key = Self::scoped_key(&outcome.execution_id, &scope);
        if let Some((_, before)) = self.scoped_before.remove(&key) {
            // Even failed commands apply already-written files.
            self.apply_scoped_diff(&scope, &before, &outcome.execution_id);
            // Release leases for scope files that did not change.
            for path in before.keys() {
                if !self.manager.recent_agent_writes().is_inflight(path) {
                    continue;
                }
                // Still leased (unchanged): resolve with the before hash.
                if let Some(hash) = before.get(path) {
                    self.manager.recent_agent_writes().resolve_inflight(
                        path.clone(),
                        hash.clone(),
                        false,
                    );
                }
            }
        } else {
            tracing::debug!(
                entity = %self.entity_id,
                execution = %outcome.execution_id,
                scope = %scope.display(),
                "no matching scope-begin snapshot; sampling skipped"
            );
        }
    }

    fn notify_session_started(&self, boundary: SessionBoundary) {
        // Session creation is NOT a completion signal: establish the scope
        // baseline and mark it as still possibly writing.
        let Some(root) = self.workspace_root.as_ref() else {
            return;
        };
        let Some(scope_dir) = boundary.scope_dir.as_ref() else {
            tracing::debug!(
                entity = %self.entity_id,
                session = %boundary.session_id,
                "background session without cwd; no scope baseline"
            );
            return;
        };
        let Some(scope) = resolve_shell_scope(root, scope_dir) else {
            tracing::debug!(
                entity = %self.entity_id,
                session = %boundary.session_id,
                scope = %scope_dir.display(),
                "background session outside workspace; no scope baseline"
            );
            return;
        };
        if let Some(before) = self.capture_scope(&scope) {
            // Keep the first baseline when one already exists: a duplicate
            // start (e.g. session reuse reported twice) must not clobber the
            // earlier snapshot, otherwise writes between the two captures
            // would lose attribution.
            if self.session_before.contains_key(&boundary.session_id) {
                return;
            }
            for path in before.keys() {
                self.manager
                    .recent_agent_writes()
                    .acquire_inflight(path.clone());
            }
            self.session_before
                .insert(boundary.session_id.clone(), before);
            self.session_scope
                .insert(boundary.session_id.clone(), scope);
        }
    }

    fn notify_session_command_finished(&self, boundary: SessionBoundary) {
        let session_id = boundary.session_id.clone();
        let (Some(before), Some(scope)) = (
            self.session_before.get(&session_id).map(|e| e.clone()),
            self.session_scope.get(&session_id).map(|e| e.clone()),
        ) else {
            return;
        };
        self.apply_scoped_diff(&scope, &before, &boundary.execution_id);
        // Refresh the baseline for the next command in the same session.
        if let Some(next) = self.capture_scope(&scope) {
            for path in next.keys() {
                if !before.contains_key(path) {
                    self.manager
                        .recent_agent_writes()
                        .acquire_inflight(path.clone());
                }
            }
            self.session_before.insert(session_id, next);
        }
    }

    fn notify_session_finished(&self, boundary: SessionBoundary) {
        let session_id = boundary.session_id.clone();
        let before = self.session_before.remove(&session_id).map(|(_, v)| v);
        let scope = self.session_scope.remove(&session_id).map(|(_, v)| v);
        if let (Some(before), Some(scope)) = (before, scope) {
            self.apply_scoped_diff(&scope, &before, &boundary.execution_id);
            for path in before.keys() {
                if self.manager.recent_agent_writes().is_inflight(path) {
                    if let Some(hash) = before.get(path) {
                        self.manager.recent_agent_writes().resolve_inflight(
                            path.clone(),
                            hash.clone(),
                            false,
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_scope_intersection_rules() {
        let ws = Path::new("/ws");
        assert_eq!(
            resolve_shell_scope(ws, Path::new("/ws/sub")),
            Some(PathBuf::from("/ws/sub"))
        );
        // Scope enclosing the workspace narrows to the workspace.
        assert_eq!(
            resolve_shell_scope(ws, Path::new("/")),
            Some(PathBuf::from("/ws"))
        );
        // Disjoint scopes have no capture range.
        assert_eq!(resolve_shell_scope(ws, Path::new("/tmp")), None);
    }

    #[test]
    fn precise_events_land_in_agent_partition() {
        let dir = tempfile::tempdir().unwrap();
        let mut manager = FileCheckpointManager::new_in_memory().unwrap();
        manager.set_workspace_root(Some(dir.path().to_path_buf()));
        let observer = AgentCheckpointObserver::new(manager.clone(), "agent-1", None);

        let path = dir.path().join("a.txt");
        std::fs::write(&path, b"hello").unwrap();
        observer.notify_precise(PreciseFileChange::new(
            path.clone(),
            PreciseFileOp::Created,
            "exec-1",
        ));

        let actor = manager.actor_id_for("agent-1");
        let ws = manager.get_actor_workspace(actor.as_str()).unwrap();
        assert!(ws.iter().any(|f| f.path == "a.txt"));
        // Watcher must not re-record the agent write as manual.
        let hash = wf_checkpoint::sha256_hex(b"hello");
        assert!(manager.recent_agent_writes().is_agent_write(&path, &hash));
    }

    #[test]
    fn out_of_scope_shell_has_no_capture() {
        let dir = tempfile::tempdir().unwrap();
        let mut manager = FileCheckpointManager::new_in_memory().unwrap();
        manager.set_workspace_root(Some(dir.path().to_path_buf()));
        let observer = AgentCheckpointObserver::new(manager.clone(), "agent-1", None);
        observer.notify_scope_begin("exec-1", Path::new("/tmp"));
        observer.notify_scope_end(
            Path::new("/tmp"),
            ScopeOutcome {
                execution_id: "exec-1".to_string(),
                success: true,
                terminated: true,
                detail: None,
            },
        );
        assert!(observer.scoped_before.is_empty());
    }
}
