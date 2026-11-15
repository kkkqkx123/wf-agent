//! Workspace project-root discovery shared by the server directory scope
//! and the CLI workspace resolution.
//!
//! A project root is the nearest ancestor (inclusive) carrying a
//! version-control marker. A `.git` file marks a linked worktree, so its
//! `gitdir` pointer is followed back to the main checkout; an unreadable
//! pointer keeps the worktree directory itself.

use std::path::{Path, PathBuf};

/// Marker entries identifying a version-controlled project root. A `.git`
/// entry may be a file (linked worktree), so callers check file versus
/// directory instead of assuming a directory.
pub const PROJECT_MARKERS: &[&str] = &[".git", ".hg"];

/// Find the owning project root for a directory: nearest ancestor
/// (inclusive) carrying a version-control marker, or the directory itself
/// when version control is absent.
pub fn discover_project_root(directory: &Path) -> PathBuf {
    let mut current = directory.to_path_buf();
    loop {
        for marker in PROJECT_MARKERS {
            let candidate = current.join(marker);
            if *marker == ".git" {
                if candidate.is_dir() {
                    return current;
                }
                if candidate.is_file() {
                    return main_checkout_for(&current, &candidate)
                        .unwrap_or_else(|| current.clone());
                }
            } else if candidate.exists() {
                return current;
            }
        }
        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => return directory.to_path_buf(),
        }
    }
}

/// Main checkout owning a linked worktree: parse the `.git` file's
/// `gitdir` pointer and, when it points inside a `worktrees` directory,
/// return the checkout holding that administrative directory. Returns
/// `None` when the pointer is unreadable or already points at the main
/// checkout, letting the caller keep the worktree directory itself.
fn main_checkout_for(worktree: &Path, dot_git: &Path) -> Option<PathBuf> {
    let content = std::fs::read_to_string(dot_git).ok()?;
    let gitdir = content.strip_prefix("gitdir:")?.trim();
    if gitdir.is_empty() {
        return None;
    }
    let gitdir = if Path::new(gitdir).is_absolute() {
        PathBuf::from(gitdir)
    } else {
        worktree.join(gitdir)
    };
    let mut current = gitdir.as_path();
    while let Some(name) = current.file_name().and_then(|n| n.to_str()) {
        if name == "worktrees" {
            // `<main>/.git/worktrees/<name>` -> `<main>/.git>` -> `<main>`.
            let dot = current.parent()?;
            if dot.file_name().and_then(|n| n.to_str()) != Some(".git") {
                return None;
            }
            return dot.parent().map(Path::to_path_buf);
        }
        current = current.parent()?;
    }
    None
}

/// Whether two directory spellings name the same directory. Canonical
/// paths compare equal; unresolvable spellings fall back to a trimmed
/// string comparison so a stale value never matches silently.
pub fn same_directory(a: &str, b: &str) -> bool {
    let left = a.trim();
    let right = b.trim();
    if left.is_empty() || right.is_empty() {
        return false;
    }
    match (std::fs::canonicalize(left), std::fs::canonicalize(right)) {
        (Ok(current), Ok(target)) => current == target,
        _ => left == right,
    }
}

/// Content fingerprint identifying a project across machines. Canonical
/// paths never match between a local checkout and its remote counterpart,
/// so affinity checks compare this instead: the version-control origin
/// plus commit first, then a committed marker file for checkouts without
/// version control. Every field is optional; an empty fingerprint means
/// the project carries no cross-machine identity and callers must fall
/// back to weak verification.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectFingerprint {
    pub git_origin: Option<String>,
    pub git_head: Option<String>,
    pub marker_id: Option<String>,
}

impl ProjectFingerprint {
    pub fn is_empty(&self) -> bool {
        self.git_origin.is_none() && self.git_head.is_none() && self.marker_id.is_none()
    }

    /// Same project when the version-control origin matches or the marker
    /// ids match. Commit equality is informational only: a stale checkout
    /// of the same project is still the same project.
    pub fn same_project(&self, other: &Self) -> bool {
        match (&self.git_origin, &other.git_origin) {
            (Some(a), Some(b)) if a == b => return true,
            _ => {}
        }
        match (&self.marker_id, &other.marker_id) {
            (Some(a), Some(b)) if a == b => return true,
            _ => {}
        }
        false
    }
}

/// File inside the project carrying a stable id for checkouts without
/// version control. Committed to the repo so every clone shares it.
pub const PROJECT_ID_FILE: &str = ".wf/project-id";

/// Bound for one metadata probe so a wedged helper cannot stall startup.
const FINGERPRINT_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

/// Read the content fingerprint of a project root. Version control wins;
/// the marker file covers plain checkouts. Every probe is time-bounded
/// and every failure collapses to `None`, so callers always get an answer.
pub async fn project_fingerprint(project: &Path) -> ProjectFingerprint {
    let (git_origin, git_head) = tokio::join!(
        probe_git(project, &["remote", "get-url", "origin"]),
        probe_git(project, &["rev-parse", "HEAD"]),
    );
    ProjectFingerprint {
        git_origin,
        git_head,
        marker_id: read_marker_id(project).await,
    }
}

async fn probe_git(project: &Path, args: &[&str]) -> Option<String> {
    let mut command = tokio::process::Command::new("git");
    command.arg("-C").arg(project).args(args);
    let output = tokio::time::timeout(FINGERPRINT_PROBE_TIMEOUT, command.output()).await;
    match output {
        Ok(Ok(output)) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if text.is_empty() {
                None
            } else {
                Some(text)
            }
        }
        _ => None,
    }
}

async fn read_marker_id(project: &Path) -> Option<String> {
    let text = tokio::fs::read_to_string(project.join(PROJECT_ID_FILE))
        .await
        .ok()?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Stable id for a canonical project path. FNV-1a over the path bytes
/// stays identical across restarts so cached entries keep their identity
/// without a persisted table.
pub fn project_id_for(project: &Path) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in project.to_string_lossy().as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_dir(prefix: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("wf-workspace-{prefix}-{}", crate::generate_id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn discovery_walks_to_vcs_root() {
        let root = unique_dir("project");
        std::fs::create_dir(root.join(".git")).unwrap();
        let nested = root.join("a").join("b");
        std::fs::create_dir_all(&nested).unwrap();
        let canonical_root = root.canonicalize().unwrap();
        let canonical_nested = nested.canonicalize().unwrap();
        assert_eq!(discover_project_root(&canonical_nested), canonical_root);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn discovery_falls_back_to_self() {
        let dir = unique_dir("plain");
        let canonical = dir.canonicalize().unwrap();
        assert_eq!(discover_project_root(&canonical), canonical);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn fingerprint_reads_marker_without_vcs() {
        let dir = unique_dir("marker");
        let canonical = dir.canonicalize().unwrap();
        std::fs::create_dir_all(canonical.join(".wf")).unwrap();
        std::fs::write(canonical.join(".wf").join("project-id"), "proj-1\n").unwrap();
        let fingerprint = project_fingerprint(&canonical).await;
        assert_eq!(fingerprint.marker_id.as_deref(), Some("proj-1"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fingerprint_match_prefers_origin_then_marker() {
        let with_origin = |origin: &str| ProjectFingerprint {
            git_origin: Some(origin.to_string()),
            git_head: Some("abc".to_string()),
            marker_id: None,
        };
        assert!(
            with_origin("git@example:repo.git").same_project(&with_origin("git@example:repo.git"))
        );
        assert!(!with_origin("git@example:a.git").same_project(&with_origin("git@example:b.git")));
        let marked = |id: &str| ProjectFingerprint {
            marker_id: Some(id.to_string()),
            ..ProjectFingerprint::default()
        };
        assert!(marked("x").same_project(&marked("x")));
        assert!(!marked("x").same_project(&marked("y")));
        assert!(!ProjectFingerprint::default().same_project(&marked("x")));
        assert!(ProjectFingerprint::default().is_empty());
    }

    #[test]
    fn discovery_resolves_linked_worktree_to_main() {
        let root = unique_dir("main");
        std::fs::create_dir(root.join(".git")).unwrap();
        let canonical_root = root.canonicalize().unwrap();
        let worktree = unique_dir("linked");
        let gitdir = canonical_root.join(".git").join("worktrees").join("linked");
        std::fs::write(
            worktree.join(".git"),
            format!("gitdir: {}", gitdir.display()),
        )
        .unwrap();
        let canonical_worktree = worktree.canonicalize().unwrap();
        assert_eq!(discover_project_root(&canonical_worktree), canonical_root);
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&worktree);
    }

    #[test]
    fn discovery_keeps_unreadable_git_file() {
        let dir = unique_dir("gitfile");
        std::fs::write(dir.join(".git"), "not-a-gitdir-pointer").unwrap();
        let canonical = dir.canonicalize().unwrap();
        assert_eq!(discover_project_root(&canonical), canonical);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn same_directory_matches_canonical_and_self() {
        let dir = unique_dir("same");
        let canonical = dir.canonicalize().unwrap();
        let spellings = (
            canonical.to_string_lossy().into_owned(),
            dir.to_string_lossy().into_owned(),
        );
        assert!(same_directory(&spellings.0, &spellings.1));
        assert!(same_directory(&spellings.0, &spellings.0));
        assert!(!same_directory(&spellings.0, ""));
        assert!(!same_directory("", &spellings.1));
        assert!(!same_directory(
            &spellings.0,
            &format!("{}-sibling", spellings.0)
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn project_id_is_stable() {
        let dir = unique_dir("id");
        let canonical = dir.canonicalize().unwrap();
        assert_eq!(project_id_for(&canonical), project_id_for(&canonical));
        assert_eq!(project_id_for(&canonical).len(), 16);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
