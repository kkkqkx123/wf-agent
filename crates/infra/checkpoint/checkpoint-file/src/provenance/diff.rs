//! Presentation-only diff rendering between two workspace states.

use std::collections::HashMap;

use crate::git_store::GitStore;
use checkpoint_base::common::diff::{diff_stats_for_text, unified_diff_text};
use checkpoint_base::error::CheckpointError;

use super::types::{DiffHunkView, FileDiffKind, FileDiffView, WorkspaceFile};
use super::workspace::{actor_tree_ids, main_tree_ids};
use crate::file::git_write::map_git_error;
use crate::file::util::sha256_hex;

/// Maximum rendered diff lines per file; longer diffs are cut with
/// `truncated: true`. The user-repo git diffs share the same cap, so
/// both surfaces keep one contract.
pub const MAX_DIFF_LINES: usize = 400;

fn empty_view(path: &str, kind: FileDiffKind) -> FileDiffView {
    FileDiffView {
        path: path.to_string(),
        kind,
        hunks: Vec::new(),
        additions: 0,
        deletions: 0,
        binary: false,
        truncated: false,
    }
}

/// Split unified text into `@@` hunk rows, capping total content lines.
/// Lines before the first hunk header cannot occur in generator output
/// and are dropped.
fn split_hunks(text: &str) -> (Vec<DiffHunkView>, bool) {
    let mut hunks = Vec::new();
    let mut current: Option<DiffHunkView> = None;
    let mut lines = 0usize;
    let mut truncated = false;
    for raw in text.lines() {
        if raw.starts_with("@@ ") {
            if let Some(hunk) = current.take() {
                hunks.push(hunk);
            }
            current = Some(DiffHunkView {
                header: raw.to_string(),
                lines: Vec::new(),
            });
        } else if let Some(hunk) = current.as_mut() {
            if lines >= MAX_DIFF_LINES {
                truncated = true;
                break;
            }
            hunk.lines.push(raw.to_string());
            lines += 1;
        }
    }
    if let Some(hunk) = current.take() {
        hunks.push(hunk);
    }
    (hunks, truncated)
}

/// Per-file diff between two workspace states. Binary files report
/// `Modified` without a diff.
pub fn diff_workspaces(a: &[WorkspaceFile], b: &[WorkspaceFile]) -> Vec<FileDiffView> {
    let a_map: HashMap<&str, &WorkspaceFile> = a.iter().map(|f| (f.path.as_str(), f)).collect();
    let b_map: HashMap<&str, &WorkspaceFile> = b.iter().map(|f| (f.path.as_str(), f)).collect();

    let mut paths: Vec<&str> = a_map
        .keys()
        .chain(b_map.keys())
        .copied()
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    paths.sort_unstable();

    let mut views = Vec::new();
    for path in paths {
        match (a_map.get(path), b_map.get(path)) {
            (None, Some(_bf)) => views.push(empty_view(path, FileDiffKind::Added)),
            (Some(_af), None) => views.push(empty_view(path, FileDiffKind::Deleted)),
            (None, None) => {} // unreachable: paths are the key union
            (Some(af), Some(bf)) => {
                if af.hash == bf.hash {
                    views.push(empty_view(path, FileDiffKind::Unchanged));
                    continue;
                }
                views.push(structured_view(path, &af.content, &bf.content));
            }
        }
    }
    views
}

/// Structured diff of two byte slices: hunk rows plus stats for text,
/// a binary marker when either side is not valid UTF-8.
fn structured_view(path: &str, before: &[u8], after: &[u8]) -> FileDiffView {
    let (Ok(before), Ok(after)) = (std::str::from_utf8(before), std::str::from_utf8(after)) else {
        let mut view = empty_view(path, FileDiffKind::Modified);
        view.binary = true;
        return view;
    };
    let text = unified_diff_text(before, after, 3, None, None);
    let stats = diff_stats_for_text(before, after);
    let (hunks, truncated) = split_hunks(&text);
    FileDiffView {
        path: path.to_string(),
        kind: FileDiffKind::Modified,
        hunks,
        additions: stats.added_lines,
        deletions: stats.removed_lines,
        binary: false,
        truncated,
    }
}

/// Change kind and line counts for one path between an optional before and
/// after state. `before == None` marks an addition, `after == None` a
/// deletion. Identical bytes report `Unchanged` with zero counts; binary
/// content reports the kind without line counts.
pub(crate) fn delta_counts(
    before: Option<&[u8]>,
    after: Option<&[u8]>,
) -> (FileDiffKind, usize, usize) {
    match (before, after) {
        (None, None) => (FileDiffKind::Unchanged, 0, 0),
        (None, Some(after)) => match std::str::from_utf8(after) {
            Ok(after) => {
                let stats = diff_stats_for_text("", after);
                (FileDiffKind::Added, stats.added_lines, 0)
            }
            Err(_) => (FileDiffKind::Added, 0, 0),
        },
        (Some(before), None) => match std::str::from_utf8(before) {
            Ok(before) => {
                let stats = diff_stats_for_text(before, "");
                (FileDiffKind::Deleted, 0, stats.removed_lines)
            }
            Err(_) => (FileDiffKind::Deleted, 0, 0),
        },
        (Some(before), Some(after)) => {
            if before == after {
                return (FileDiffKind::Unchanged, 0, 0);
            }
            match (std::str::from_utf8(before), std::str::from_utf8(after)) {
                (Ok(before), Ok(after)) => {
                    let stats = diff_stats_for_text(before, after);
                    (
                        FileDiffKind::Modified,
                        stats.added_lines,
                        stats.removed_lines,
                    )
                }
                _ => (FileDiffKind::Modified, 0, 0),
            }
        }
    }
}

/// Diff between two actor workspaces. Identifier maps are compared first;
/// only files with differing identifiers load bytes for content diff.
pub fn diff_actors(
    git: &GitStore,
    actor_a: &str,
    actor_b: &str,
) -> Result<Vec<FileDiffView>, CheckpointError> {
    let a = actor_tree_ids(git, actor_a)?;
    let b = actor_tree_ids(git, actor_b)?;
    diff_ids(git, &a, &b)
}

/// Diff between an actor workspace and the main line. Same lazy loading as
/// actor diffs: unchanged files never pay blob reads.
pub fn diff_against_main(
    git: &GitStore,
    actor: &str,
) -> Result<Vec<FileDiffView>, CheckpointError> {
    let actor_files = actor_tree_ids(git, actor)?;
    let main_files = main_tree_ids(git)?;
    diff_ids(git, &actor_files, &main_files)
}

fn diff_ids(
    git: &GitStore,
    a: &std::collections::HashMap<String, (String, String)>,
    b: &std::collections::HashMap<String, (String, String)>,
) -> Result<Vec<FileDiffView>, CheckpointError> {
    use std::collections::HashSet;
    let mut paths: Vec<&str> = a
        .keys()
        .chain(b.keys())
        .map(String::as_str)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    paths.sort_unstable();
    let mut views = Vec::with_capacity(paths.len());
    for path in paths {
        match (a.get(path), b.get(path)) {
            (None, Some(_)) => views.push(empty_view(path, FileDiffKind::Added)),
            (Some(_), None) => views.push(empty_view(path, FileDiffKind::Deleted)),
            (None, None) => {}
            (Some((_, blob_a)), Some((_, blob_b))) => {
                if blob_a == blob_b {
                    views.push(empty_view(path, FileDiffKind::Unchanged));
                    continue;
                }
                let bytes_a = git.read_blob(blob_a).map_err(map_git_error)?;
                let bytes_b = git.read_blob(blob_b).map_err(map_git_error)?;
                if sha256_hex(&bytes_a) == sha256_hex(&bytes_b) {
                    views.push(empty_view(path, FileDiffKind::Unchanged));
                    continue;
                }
                views.push(structured_view(path, &bytes_a, &bytes_b));
            }
        }
    }
    Ok(views)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_hunks_groups_header_lines() {
        let text = "@@ -1,2 +1,3 @@\n-old\n+new\n+extra\n context\n@@ -10 +11 @@\n tail\n";
        let (hunks, truncated) = split_hunks(text);
        assert!(!truncated);
        assert_eq!(hunks.len(), 2);
        assert_eq!(hunks[0].header, "@@ -1,2 +1,3 @@");
        assert_eq!(hunks[0].lines.len(), 4);
        assert_eq!(hunks[1].lines, vec![" tail".to_string()]);
    }

    #[test]
    fn structured_view_reports_binary_without_hunks() {
        let view = structured_view("img.png", &[0xff, 0xd8], &[0xff, 0xd9]);
        assert_eq!(view.kind, FileDiffKind::Modified);
        assert!(view.binary);
        assert!(view.hunks.is_empty());
        assert!(!view.truncated);
    }

    #[test]
    fn structured_view_truncates_long_diffs() {
        let before = "same\n".repeat(10);
        let after = "same\n".repeat(10) + &"new\n".repeat(MAX_DIFF_LINES + 50);
        let view = structured_view("big.txt", before.as_bytes(), after.as_bytes());
        assert!(!view.binary);
        assert!(view.truncated);
        assert_eq!(
            view.hunks.iter().map(|h| h.lines.len()).sum::<usize>(),
            MAX_DIFF_LINES
        );
        assert!(view.additions >= MAX_DIFF_LINES);
    }

    #[test]
    fn delta_counts_classifies_add_modify_delete() {
        let (kind, add, del) = delta_counts(None, Some(b"one\ntwo\n"));
        assert_eq!(kind, FileDiffKind::Added);
        assert_eq!((add, del), (2, 0));

        let (kind, add, del) = delta_counts(Some(b"one\ntwo\n"), Some(b"one\nthree\n"));
        assert_eq!(kind, FileDiffKind::Modified);
        assert_eq!((add, del), (1, 1));

        let (kind, add, del) = delta_counts(Some(b"one\ntwo\n"), None);
        assert_eq!(kind, FileDiffKind::Deleted);
        assert_eq!((add, del), (0, 2));

        let (kind, add, del) = delta_counts(Some(b"same"), Some(b"same"));
        assert_eq!(kind, FileDiffKind::Unchanged);
        assert_eq!((add, del), (0, 0));

        // Binary content reports the kind without line counts.
        let (kind, add, del) = delta_counts(None, Some(&[0xff, 0x00]));
        assert_eq!(kind, FileDiffKind::Added);
        assert_eq!((add, del), (0, 0));
    }
}
