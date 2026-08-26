use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::CheckpointError;
use crate::file_util::sha256_hex;
use crate::scan::WorkspaceScanner;

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
pub struct WorkspaceChangeCollector {
    base_dir: PathBuf,
    scope: Vec<PathBuf>,
    scanner: WorkspaceScanner,
}

impl WorkspaceChangeCollector {
    /// Build the collector. `allowed_write` prefixes are absolute paths, or
    /// relative paths resolved against `base_dir`; prefixes outside the
    /// workspace are excluded from the scope.
    pub fn new(base_dir: &Path, allowed_write: &[String], scanner: WorkspaceScanner) -> Self {
        let normalized_base = normalize(base_dir);
        let mut scope = Vec::new();
        for prefix in allowed_write {
            let candidate = if Path::new(prefix).is_absolute() {
                normalize(Path::new(prefix))
            } else {
                normalize(&base_dir.join(prefix))
            };
            if candidate.starts_with(&normalized_base) {
                scope.push(candidate);
            }
        }
        scope.sort();
        scope.dedup();
        Self {
            base_dir: base_dir.to_path_buf(),
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
        for prefix in &self.scope {
            self.collect_dir(prefix, &mut hashes)?;
        }
        Ok(hashes)
    }

    fn collect_dir(
        &self,
        dir: &Path,
        out: &mut HashMap<PathBuf, String>,
    ) -> Result<(), CheckpointError> {
        if !dir.is_dir() {
            return Ok(());
        }
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                self.collect_dir(&path, out)?;
            } else if file_type.is_file() && !file_type.is_symlink() {
                let relative = path
                    .strip_prefix(&self.base_dir)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                if self.scanner.is_ignored(&relative) {
                    continue;
                }
                let content = std::fs::read(&path)?;
                out.insert(path, sha256_hex(&content));
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

/// Lexical path normalization (no filesystem access).
fn normalize(path: &Path) -> PathBuf {
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
        std::fs::write(dir.path().join("a.txt"), b"same").unwrap();
        let before = hashes(dir.path());
        let after = hashes(dir.path());
        assert!(WorkspaceChangeCollector::diff(&before, &after).is_empty());
    }
}
