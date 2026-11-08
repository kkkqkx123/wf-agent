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
