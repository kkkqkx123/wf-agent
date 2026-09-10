//! Generic tool side-effect observation.
//!
//! Business-free observation capability carried by [`ToolExecutionContext`].
//! The tool layer reports *what happened* (precise file changes, scoped
//! execution boundaries, background session lifecycle); an upper layer that
//! already owns both the tool registry and the file-checkpoint manager
//! implements [`ToolSideEffectObserver`] and injects it into the context.
//! This keeps the `wf-tools -> wf-checkpoint` dependency direction intact:
//! this module never mentions checkpoint, actor or layertwine types.

use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Precise file operation reported after a successful disk write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreciseFileOp {
    /// File was created or overwritten (callers distinguish via prior
    /// existence when needed; observers treat both as upsert).
    Created,
    Modified,
    Deleted,
    /// `path` in the enclosing change is the new location; `from` is the old.
    Renamed {
        from: PathBuf,
    },
}

/// One successful file mutation with an absolute path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreciseFileChange {
    /// Absolute path of the affected file (new path for renames).
    pub path: PathBuf,
    pub op: PreciseFileOp,
    /// Tool execution that produced the change (for diagnostics).
    pub execution_id: String,
}

impl PreciseFileChange {
    pub fn new(path: PathBuf, op: PreciseFileOp, execution_id: &str) -> Self {
        Self {
            path,
            op,
            execution_id: execution_id.to_string(),
        }
    }
}

/// Outcome of a scoped (directory-diff) execution such as `execute_command`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeOutcome {
    /// Tool execution that produced the scope.
    pub execution_id: String,
    /// Whether the command itself succeeded. Scoped sampling must still run
    /// when this is false as long as the process has terminated: a failed
    /// command may already have written files.
    pub success: bool,
    /// Whether the process was confirmed terminated. When false the sample
    /// is incomplete and must be marked as such, never as a complete result.
    pub terminated: bool,
    /// Human-readable detail (exit code, timeout, startup failure, ...).
    pub detail: Option<String>,
}

/// Background session lifecycle boundary. `scope_dir` is the resolved
/// session cwd when known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionBoundary {
    pub execution_id: String,
    pub session_id: String,
    pub scope_dir: Option<PathBuf>,
}

/// Business-free observer implemented by an upper layer (agent / workflow /
/// API) that owns the file-checkpoint manager. All methods are synchronous
/// so filesystem handlers, async handlers and stateful handlers share one
/// context. Implementations must emit structured logs/events on failure and
/// never silently drop observations. Default methods are no-ops so observers
/// only implement the boundaries they care about.
pub trait ToolSideEffectObserver: Send + Sync {
    /// Precise single-file change after a successful disk operation.
    fn notify_precise(&self, _change: PreciseFileChange) {}

    /// A scoped execution is about to start in `scope_dir` (absolute,
    /// already resolved from `parameters.cwd` or the tool default).
    fn notify_scope_begin(&self, _execution_id: &str, _scope_dir: &Path) {}

    /// A scoped execution finished (or failed to start). `terminated=false`
    /// means the sampling is incomplete and must not be presented as complete.
    fn notify_scope_end(&self, _scope_dir: &Path, _outcome: ScopeOutcome) {}

    /// A background session was created (long-running command may still be
    /// running; this is NOT a completion signal).
    fn notify_session_started(&self, _boundary: SessionBoundary) {}

    /// One command inside a background session finished.
    fn notify_session_command_finished(&self, _boundary: SessionBoundary) {}

    /// A background session terminated / was released.
    fn notify_session_finished(&self, _boundary: SessionBoundary) {}
}

/// Clonable shared handle carried by the execution context. `None` means
/// plain tool behavior with no observation.
#[derive(Clone, Default)]
pub struct ToolSideEffectObserverHandle {
    inner: Option<Arc<dyn ToolSideEffectObserver>>,
}

impl ToolSideEffectObserverHandle {
    pub fn none() -> Self {
        Self { inner: None }
    }

    pub fn new(observer: Arc<dyn ToolSideEffectObserver>) -> Self {
        Self {
            inner: Some(observer),
        }
    }

    pub fn is_some(&self) -> bool {
        self.inner.is_some()
    }

    pub fn notify_precise(&self, change: PreciseFileChange) {
        if let Some(inner) = self.inner.as_ref() {
            inner.notify_precise(change);
        }
    }

    pub fn notify_scope_begin(&self, execution_id: &str, scope_dir: &Path) {
        if let Some(inner) = self.inner.as_ref() {
            inner.notify_scope_begin(execution_id, scope_dir);
        }
    }

    pub fn notify_scope_end(&self, scope_dir: &Path, outcome: ScopeOutcome) {
        if let Some(inner) = self.inner.as_ref() {
            inner.notify_scope_end(scope_dir, outcome);
        }
    }

    pub fn notify_session_started(&self, boundary: SessionBoundary) {
        if let Some(inner) = self.inner.as_ref() {
            inner.notify_session_started(boundary);
        }
    }

    pub fn notify_session_command_finished(&self, boundary: SessionBoundary) {
        if let Some(inner) = self.inner.as_ref() {
            inner.notify_session_command_finished(boundary);
        }
    }

    pub fn notify_session_finished(&self, boundary: SessionBoundary) {
        if let Some(inner) = self.inner.as_ref() {
            inner.notify_session_finished(boundary);
        }
    }
}

impl std::fmt::Debug for ToolSideEffectObserverHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolSideEffectObserverHandle")
            .field("has_observer", &self.inner.is_some())
            .finish()
    }
}

/// Lexically normalize an absolute path (no filesystem access) so observer
/// reports and checkpoint keys share one spelling.
pub fn normalize_observer_path(path: &Path) -> PathBuf {
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
    if out.as_os_str().is_empty() {
        return PathBuf::from("/");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingObserver {
        precise: AtomicUsize,
        begins: AtomicUsize,
        ends: AtomicUsize,
    }

    impl ToolSideEffectObserver for CountingObserver {
        fn notify_precise(&self, _change: PreciseFileChange) {
            self.precise.fetch_add(1, Ordering::SeqCst);
        }

        fn notify_scope_begin(&self, _execution_id: &str, _scope_dir: &Path) {
            self.begins.fetch_add(1, Ordering::SeqCst);
        }

        fn notify_scope_end(&self, _scope_dir: &Path, _outcome: ScopeOutcome) {
            self.ends.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn none_handle_is_noop() {
        let handle = ToolSideEffectObserverHandle::none();
        assert!(!handle.is_some());
        handle.notify_precise(PreciseFileChange::new(
            PathBuf::from("/tmp/a.txt"),
            PreciseFileOp::Created,
            "exec-1",
        ));
        handle.notify_scope_begin("exec-1", Path::new("/tmp"));
        handle.notify_scope_end(
            Path::new("/tmp"),
            ScopeOutcome {
                execution_id: "exec-1".to_string(),
                success: true,
                terminated: true,
                detail: None,
            },
        );
    }

    #[test]
    fn handle_forwards_to_observer() {
        let observer = Arc::new(CountingObserver {
            precise: AtomicUsize::new(0),
            begins: AtomicUsize::new(0),
            ends: AtomicUsize::new(0),
        });
        let handle = ToolSideEffectObserverHandle::new(observer.clone());
        assert!(handle.is_some());
        handle.notify_precise(PreciseFileChange::new(
            PathBuf::from("/tmp/a.txt"),
            PreciseFileOp::Modified,
            "exec-1",
        ));
        handle.notify_scope_begin("exec-1", Path::new("/tmp"));
        handle.notify_scope_end(
            Path::new("/tmp"),
            ScopeOutcome {
                execution_id: "exec-1".to_string(),
                success: false,
                terminated: true,
                detail: Some("exit 1".to_string()),
            },
        );
        assert_eq!(observer.precise.load(Ordering::SeqCst), 1);
        assert_eq!(observer.begins.load(Ordering::SeqCst), 1);
        assert_eq!(observer.ends.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn observer_paths_normalize() {
        assert_eq!(
            normalize_observer_path(Path::new("/a/b/../c")),
            PathBuf::from("/a/c")
        );
    }
}
