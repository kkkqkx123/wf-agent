use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::file::util::sha256_hex;
use crate::scan::WorkspaceScanner;
use checkpoint_base::error::CheckpointError;

/// Kind of a collected workspace change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectedChangeKind {
    /// File created between the two capture points.
    Add,
    /// File modified between the two capture points.
    Modify,
    /// File removed between the two capture points.
    Delete,
}

/// One workspace change detected by hashing files before and after a script execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectedChange {
    /// Absolute path of the changed file.
    pub path: PathBuf,
    pub kind: CollectedChangeKind,
}

impl CollectedChange {
    pub fn new(path: PathBuf, kind: CollectedChangeKind) -> Self {
        Self { path, kind }
    }
}

/// Captures file-state changes within a workspace-scoped write scope.
///
/// The scope is the `PathPolicy.allowed_write` prefix set intersected with
/// the workspace root: only files under an allowed-write prefix *inside* the
/// workspace are tracked, so scripts that write outside the workspace (e.g.
/// `/tmp`) are not re-hashed and large workspaces are not fully rescanned.
/// Ignore rules (hardcoded + custom) are applied on top.
///
/// Contract:
/// - the collector only accepts an already resolved and validated
///   workspace root plus scope prefixes; scope resolution itself is the
///   caller's responsibility;
/// - an empty scope is the explicit "no synchronous capture range" result,
///   never a signal to scan the whole workspace;
/// - symbolic links never enter the store as links: an in-workspace target
///   contributes its content under the link path, an out-of-workspace
///   target rejects the whole subtree it would escape through;
/// - capture-time file read failures surface as path-qualified IO errors so
///   the caller can apply its `FailureBehavior` with execution id, scope and
///   stage in the log.
pub struct WorkspaceChangeCollector {
    base_dir: PathBuf,
    canonical_base: PathBuf,
    scope: Vec<PathBuf>,
    scanner: WorkspaceScanner,
}

impl WorkspaceChangeCollector {
    /// Build the collector. `allowed_write` prefixes are absolute paths, or
    /// relative paths resolved against `base_dir`; prefixes outside the
    /// workspace are excluded from the scope.
    pub fn new(base_dir: &Path, allowed_write: &[String], scanner: WorkspaceScanner) -> Self {
        let normalized_base = crate::watcher::normalize_absolute_path(base_dir);
        let canonical_base = base_dir
            .canonicalize()
            .unwrap_or_else(|_| normalized_base.clone());
        let mut scope = Vec::new();
        for prefix in allowed_write {
            let candidate = if Path::new(prefix).is_absolute() {
                crate::watcher::normalize_absolute_path(Path::new(prefix))
            } else {
                crate::watcher::normalize_absolute_path(&base_dir.join(prefix))
            };
            if candidate.starts_with(&normalized_base) {
                scope.push(candidate);
            }
        }
        scope.sort();
        scope.dedup();
        Self {
            base_dir: normalized_base,
            canonical_base,
            scope,
            scanner,
        }
    }

    /// Whether the collector has any in-workspace scope (empty allowed-write
    /// prefix set yields an empty scope and therefore no capture).
    pub fn has_scope(&self) -> bool {
        !self.scope.is_empty()
    }

    /// The resolved scope prefixes (absolute, in-workspace).
    pub fn scope(&self) -> &[PathBuf] {
        &self.scope
    }

    /// Hash every file currently inside the scope (absolute path -> sha256).
    /// The result is a deterministic "before" snapshot for
    /// [`WorkspaceChangeCollector::diff`].
    pub fn capture(&self) -> Result<HashMap<PathBuf, String>, CheckpointError> {
        let mut hashes = HashMap::new();
        let mut followed: HashSet<PathBuf> = HashSet::new();
        for prefix in &self.scope {
            self.collect_dir(prefix, prefix, &mut followed, &mut hashes)?;
        }
        Ok(hashes)
    }

    /// Resolve a symlink to its canonical target when the target stays
    /// inside the workspace. Out-of-workspace or unresolvable targets are
    /// rejected (warned, skipped) so links can never smuggle outside files
    /// into the capture. `followed` guards against link cycles.
    fn resolve_link(&self, link: &Path, followed: &mut HashSet<PathBuf>) -> Option<PathBuf> {
        match link.canonicalize() {
            Ok(target) if target.starts_with(&self.canonical_base) => {
                // Only directories join the cycle guard: two links to one
                // file are a diamond, not a cycle. Directory cycles must
                // pass through a dir, so guarding dirs alone suffices.
                if target.is_dir() && !followed.insert(target.clone()) {
                    tracing::warn!(
                        link = %link.display(),
                        "symlink cycle detected; skipping"
                    );
                    return None;
                }
                Some(target)
            }
            _ => {
                tracing::warn!(
                    link = %link.display(),
                    "symlink escapes workspace or is unresolvable; skipping"
                );
                None
            }
        }
    }

    fn record_file(
        &self,
        walk_path: &Path,
        recorded_path: &Path,
        out: &mut HashMap<PathBuf, String>,
    ) -> Result<(), CheckpointError> {
        let relative = crate::file::util::normalize_posix_separators(
            &recorded_path
                .strip_prefix(&self.base_dir)
                .unwrap_or(recorded_path)
                .to_string_lossy(),
        );
        if self.scanner.is_ignored(&relative) {
            return Ok(());
        }
        let content = std::fs::read(walk_path).map_err(|e| {
            CheckpointError::Io(std::io::Error::other(format!(
                "failed to read scoped file '{}': {e}",
                walk_path.display()
            )))
        })?;
        out.insert(recorded_path.to_path_buf(), sha256_hex(&content));
        Ok(())
    }

    /// `walk` is the filesystem path traversed (canonical after following an
    /// in-workspace link); `recorded` is the key stored in the snapshot
    /// (always the lexical in-workspace path, so before/after maps agree).
    fn collect_dir(
        &self,
        walk: &Path,
        recorded: &Path,
        followed: &mut HashSet<PathBuf>,
        out: &mut HashMap<PathBuf, String>,
    ) -> Result<(), CheckpointError> {
        if walk.is_file() {
            if std::fs::symlink_metadata(walk).is_ok_and(|m| m.file_type().is_symlink())
                && self.resolve_link(walk, followed).is_none()
            {
                return Ok(());
            }
            self.record_file(walk, recorded, out)?;
            return Ok(());
        }
        if !walk.is_dir() {
            return Ok(());
        }
        // A symlinked scope or directory is traversed at its canonical
        // target while keys stay lexical; escapes were rejected above.
        let physical;
        let walk = if std::fs::symlink_metadata(walk).is_ok_and(|m| m.file_type().is_symlink()) {
            match self.resolve_link(walk, followed) {
                Some(target) => {
                    physical = target;
                    &physical
                }
                None => return Ok(()),
            }
        } else {
            walk
        };
        // A single directory scan: a missing scope directory is "no files",
        // not an error; per-file read failures below are path-qualified.
        let entries = std::fs::read_dir(walk).map_err(|e| {
            CheckpointError::Io(std::io::Error::other(format!(
                "failed to scan scope '{}': {e}",
                walk.display()
            )))
        })?;
        for entry in entries {
            let entry = entry.map_err(|e| {
                CheckpointError::Io(std::io::Error::other(format!(
                    "failed to read scope entry in '{}': {e}",
                    walk.display()
                )))
            })?;
            let path = entry.path();
            let recorded_child = recorded.join(entry.file_name());
            let file_type = entry.file_type().map_err(|e| {
                CheckpointError::Io(std::io::Error::other(format!(
                    "failed to stat '{}': {e}",
                    path.display()
                )))
            })?;
            if file_type.is_symlink() {
                // Links never enter the store: in-workspace targets
                // contribute content under the link path, escapes are cut.
                let Some(target) = self.resolve_link(&path, followed) else {
                    continue;
                };
                if target.is_file() {
                    self.record_file(&path, &recorded_child, out)?;
                } else if target.is_dir() {
                    self.collect_dir(&target, &recorded_child, followed, out)?;
                }
            } else if file_type.is_dir() {
                self.collect_dir(&path, &recorded_child, followed, out)?;
            } else if file_type.is_file() {
                self.record_file(&path, &recorded_child, out)?;
            }
        }
        Ok(())
    }

    /// List the changes between a "before" and an "after" hash snapshot:
    /// added / modified / deleted files, sorted by path.
    pub fn diff(
        before: &HashMap<PathBuf, String>,
        after: &HashMap<PathBuf, String>,
    ) -> Vec<CollectedChange> {
        let mut changes = Vec::new();
        for (path, hash) in after {
            match before.get(path) {
                None => changes.push(CollectedChange::new(path.clone(), CollectedChangeKind::Add)),
                Some(prev) if prev != hash => changes.push(CollectedChange::new(
                    path.clone(),
                    CollectedChangeKind::Modify,
                )),
                _ => {}
            }
        }
        for path in before.keys() {
            if !after.contains_key(path) {
                changes.push(CollectedChange::new(
                    path.clone(),
                    CollectedChangeKind::Delete,
                ));
            }
        }
        changes.sort_by(|a, b| a.path.cmp(&b.path));
        changes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::ScanConfig;

    fn collector(root: &Path, prefixes: &[&str]) -> WorkspaceChangeCollector {
        let prefixes: Vec<String> = prefixes.iter().map(|s| s.to_string()).collect();
        WorkspaceChangeCollector::new(
            root,
            &prefixes,
            WorkspaceScanner::new(ScanConfig::default()),
        )
    }

    fn hashes(root: &Path) -> HashMap<PathBuf, String> {
        let c = collector(root, &["."]);
        c.capture().unwrap()
    }

    #[test]
    fn capture_hashes_only_files_in_scope() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), b"a").unwrap();
        std::fs::create_dir_all(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub/b.txt"), b"b").unwrap();

        let c = collector(dir.path(), &["."]);
        let map = c.capture().unwrap();
        assert_eq!(map.len(), 2);
        assert_eq!(
            map.get(&dir.path().join("a.txt")).unwrap(),
            &sha256_hex(b"a")
        );
        assert_eq!(
            map.get(&dir.path().join("sub/b.txt")).unwrap(),
            &sha256_hex(b"b")
        );
    }

    #[test]
    fn out_of_workspace_prefixes_are_excluded() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), b"a").unwrap();
        std::fs::write(outside.path().join("x.txt"), b"x").unwrap();

        let c = collector(dir.path(), &[outside.path().to_str().unwrap(), "."]);
        assert_eq!(c.scope().len(), 1);
        let map = c.capture().unwrap();
        assert_eq!(map.len(), 1);
        assert!(map.contains_key(&dir.path().join("a.txt")));
    }

    #[test]
    fn empty_scope_has_no_capture() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), b"a").unwrap();
        let c = collector(dir.path(), &["/tmp"]);
        assert!(!c.has_scope());
        assert!(c.capture().unwrap().is_empty());
    }

    #[test]
    fn ignored_files_are_not_captured() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), b"a").unwrap();
        std::fs::create_dir_all(dir.path().join("node_modules")).unwrap();
        std::fs::write(dir.path().join("node_modules/lib.js"), b"lib").unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join(".git/config"), b"git").unwrap();

        let c = collector(dir.path(), &["."]);
        let map = c.capture().unwrap();
        assert_eq!(map.len(), 1, "only a.txt captured");
        assert!(map.contains_key(&dir.path().join("a.txt")));
    }

    #[test]
    fn diff_detects_add_modify_delete() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), b"v1").unwrap();
        std::fs::write(dir.path().join("gone.txt"), b"gone").unwrap();
        let before = hashes(dir.path());

        std::fs::write(dir.path().join("a.txt"), b"v2").unwrap();
        std::fs::write(dir.path().join("new.txt"), b"new").unwrap();
        std::fs::remove_file(dir.path().join("gone.txt")).unwrap();

        let after = hashes(dir.path());
        let changes = WorkspaceChangeCollector::diff(&before, &after);
        assert_eq!(changes.len(), 3);
        assert_eq!(changes[0].path, dir.path().join("a.txt"));
        assert_eq!(changes[0].kind, CollectedChangeKind::Modify);
        assert_eq!(changes[1].path, dir.path().join("gone.txt"));
        assert_eq!(changes[1].kind, CollectedChangeKind::Delete);
        assert_eq!(changes[2].path, dir.path().join("new.txt"));
        assert_eq!(changes[2].kind, CollectedChangeKind::Add);
    }

    #[test]
    fn diff_is_empty_without_changes() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("same"), b"same").unwrap();
        let before = hashes(dir.path());
        let after = hashes(dir.path());
        assert!(WorkspaceChangeCollector::diff(&before, &after).is_empty());
    }

    #[test]
    fn symlink_escape_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.txt"), b"secret").unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("evil")).unwrap();
        std::fs::write(dir.path().join("ok.txt"), b"ok").unwrap();

        let c = collector(dir.path(), &["."]);
        let map = c.capture().unwrap();
        assert!(map.contains_key(&dir.path().join("ok.txt")));
        assert!(
            !map.keys().any(|p| p.starts_with(outside.path())),
            "outside files must never enter the capture"
        );
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn in_workspace_symlink_content_is_captured_under_link_path() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("real.txt"), b"data").unwrap();
        std::os::unix::fs::symlink(dir.path().join("real.txt"), dir.path().join("link.txt"))
            .unwrap();

        let c = collector(dir.path(), &["."]);
        let map = c.capture().unwrap();
        assert_eq!(
            map.get(&dir.path().join("link.txt")).unwrap(),
            &sha256_hex(b"data")
        );
    }

    #[test]
    fn symlink_cycle_terminates() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("a")).unwrap();
        std::fs::create_dir_all(dir.path().join("b")).unwrap();
        std::fs::write(dir.path().join("a/f.txt"), b"f").unwrap();
        std::os::unix::fs::symlink(dir.path().join("b"), dir.path().join("a/to_b")).unwrap();
        std::os::unix::fs::symlink(dir.path().join("a"), dir.path().join("b/to_a")).unwrap();

        let c = collector(dir.path(), &["."]);
        let map = c.capture().unwrap();
        assert!(map.contains_key(&dir.path().join("a/f.txt")));
    }
}
