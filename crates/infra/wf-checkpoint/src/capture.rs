//! Scope/session sampling logic migrated out of
//! `wf-agent::checkpoint_observer::AgentCheckpointObserver`.
//!
//! Originally the observer trait impl did two jobs: translate
//! `PreciseFileChange` → `PreciseFileEvent` for `apply_precise_file_events`,
//! and manage per-execution `HashMap<PathBuf, String>` before/after snapshots
//! for foreground scoped runs and background sessions. The second job is the
//! scope of this module: both foreground `execute_command` and background
//! `execute_in_session` need the same workspace-intersection resolution,
//! before-snapshot capture, after-snapshot diffing, and `recent_agent_writes`
//! lease bookkeeping.
//!
//! `ScopeCapture` bundles those helpers behind a single struct so
//! `CheckpointSession` (and any future non-session caller) can drive scope
//! sampling without reaching into `script_capture`, `scan`, or
//! `recent_agent_writes` individually.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use dashmap::DashMap;

use crate::ActorId;
use crate::FileCheckpointManager;
use crate::WorkspaceChangeCollector;
use crate::WorkspaceScanner;

/// Resolve the shell scope intersection: the requested directory intersected
/// with the workspace root. Returns `None` for out-of-workspace execution
/// (explicit "no synchronous capture range", never a whole-workspace scan).
/// When the requested scope encloses the workspace, the scope is narrowed to
/// the workspace root.
pub fn resolve_shell_scope(workspace_root: &Path, scope_dir: &Path) -> Option<PathBuf> {
    let ws = crate::watcher::normalize_absolute_path(workspace_root);
    let scope = crate::watcher::normalize_absolute_path(scope_dir);
    if scope.starts_with(&ws) {
        Some(scope)
    } else if ws.starts_with(&scope) {
        Some(ws)
    } else {
        None
    }
}

/// Per-execution state for scoped shell sampling (foreground runs).
#[derive(Clone)]
pub struct ScopeCapture {
    manager: FileCheckpointManager,
    actor: ActorId,
    entity_id: String,
    /// `execution_id|scope` -> before hashes for foreground scoped runs.
    scoped_before: Arc<DashMap<String, HashMap<PathBuf, String>>>,
    /// session_id -> before hashes for background sessions.
    session_before: Arc<DashMap<String, HashMap<PathBuf, String>>>,
    /// session_id -> resolved scope dir.
    session_scope: Arc<DashMap<String, PathBuf>>,
}

impl ScopeCapture {
    pub fn new(
        manager: FileCheckpointManager,
        actor: ActorId,
        entity_id: &str,
    ) -> Result<Self, crate::error::CheckpointError> {
        Ok(Self {
            manager,
            actor,
            entity_id: entity_id.to_string(),
            scoped_before: Arc::new(DashMap::new()),
            session_before: Arc::new(DashMap::new()),
            session_scope: Arc::new(DashMap::new()),
        })
    }

    pub fn actor(&self) -> &ActorId {
        &self.actor
    }

    pub fn entity_id(&self) -> &str {
        &self.entity_id
    }

    pub fn manager(&self) -> &FileCheckpointManager {
        &self.manager
    }

    // ---- foreground scope helpers ----

    pub fn begin_scope(&self, execution_id: &str, scope_dir: &Path) -> Option<PathBuf> {
        let root = self.manager.workspace_root()?;
        let scope = resolve_shell_scope(root, scope_dir)?;
        if let Some(before) = self.capture_scope(&scope) {
            for path in before.keys() {
                self.manager.recent_agent_writes().acquire_inflight(path.clone());
            }
            self.scoped_before
                .insert(key(execution_id, &scope), before);
        } else {
            self.scoped_before
                .insert(key(execution_id, &scope), HashMap::new());
        }
        Some(scope)
    }

    pub fn end_scope(
        &self,
        execution_id: &str,
        scope_dir: &Path,
        terminated: bool,
    ) -> Option<()> {
        let root = self.manager.workspace_root()?;
        let scope = resolve_shell_scope(root, scope_dir)?;
        if !terminated {
            return None;
        }
        let scoped_key = key(execution_id, &scope);
        let before = self.scoped_before.remove(&scoped_key).map(|(_, v)| v)?;
        self.apply_scoped_diff(&scope, &before, execution_id);
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
        Some(())
    }

    // ---- background session helpers ----

    pub fn begin_session(&self, session_id: &str, scope_dir: &Path) -> Option<PathBuf> {
        let root = self.manager.workspace_root()?;
        let scope = resolve_shell_scope(root, scope_dir)?;
        if let Some(before) = self.capture_scope(&scope) {
            if self.session_before.contains_key(session_id) {
                return Some(scope);
            }
            for path in before.keys() {
                self.manager.recent_agent_writes().acquire_inflight(path.clone());
            }
            self.session_before
                .insert(session_id.to_string(), before);
            self.session_scope
                .insert(session_id.to_string(), scope.clone());
        }
        Some(scope)
    }

    pub fn session_command_finished(&self, session_id: &str, execution_id: &str) {
        let (Some(before), Some(scope)) = (
            self.session_before.get(session_id).map(|e| e.clone()),
            self.session_scope.get(session_id).map(|e| e.clone()),
        ) else {
            return;
        };
        self.apply_scoped_diff(&scope, &before, execution_id);
        if let Some(next) = self.capture_scope(&scope) {
            for path in next.keys() {
                if !before.contains_key(path) {
                    self.manager.recent_agent_writes().acquire_inflight(path.clone());
                }
            }
            self.session_before.insert(session_id.to_string(), next);
        }
    }

    pub fn end_session(&self, session_id: &str, execution_id: &str) {
        let before = self.session_before.remove(session_id).map(|(_, v)| v);
        let scope = self.session_scope.remove(session_id).map(|(_, v)| v);
        if let (Some(before), Some(scope)) = (before, scope) {
            self.apply_scoped_diff(&scope, &before, execution_id);
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

    // ---- internals ----

    fn collector_for(&self, scope: &Path) -> Option<WorkspaceChangeCollector> {
        let root = self.manager.workspace_root()?;
        let scanner = WorkspaceScanner::new(self.manager.scan_config().clone());
        let scope_str = scope.to_string_lossy().to_string();
        let collector = WorkspaceChangeCollector::new(root, &[scope_str], scanner);
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
        let Some(root) = self.manager.workspace_root() else {
            return;
        };
        let collector = match self.collector_for(scope) {
            Some(c) => c,
            None => return,
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
        let changes = WorkspaceChangeCollector::diff(before, &after);
        if changes.is_empty() {
            return;
        }
        match self.manager.apply_workspace_changes(
            &self.actor,
            root,
            &changes,
            self.manager.failure_behavior(),
        ) {
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
        for change in &changes {
            let path = &change.path;
            match std::fs::read(path) {
                Ok(content) => {
                    let hash = crate::sha256_hex(&content);
                    self.manager.recent_agent_writes().resolve_inflight(
                        path.clone(),
                        hash,
                        false,
                    );
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
}

fn key(execution_id: &str, scope: &Path) -> String {
    format!("{}|{}", execution_id, scope.display())
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
        assert_eq!(
            resolve_shell_scope(ws, Path::new("/")),
            Some(PathBuf::from("/ws"))
        );
        assert_eq!(resolve_shell_scope(ws, Path::new("/tmp")), None);
    }
}
