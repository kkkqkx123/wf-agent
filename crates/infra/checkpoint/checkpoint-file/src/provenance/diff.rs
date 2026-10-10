//! Presentation-only diff rendering between two workspace states.

use std::collections::HashMap;

use crate::git_store::GitStore;
use checkpoint_base::common::diff::{diff_stats_for_text, unified_diff_text};
use checkpoint_base::error::CheckpointError;

use super::types::{FileDiffKind, FileDiffView, WorkspaceFile};
use super::workspace::{actor_tree_ids, main_tree_ids};
use crate::file::git_write::map_git_error;
use crate::file::util::sha256_hex;

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
            (None, Some(_bf)) => views.push(FileDiffView {
                path: path.to_string(),
                kind: FileDiffKind::Added,
                diff: None,
                additions: None,
                deletions: None,
            }),
            (Some(_af), None) => views.push(FileDiffView {
                path: path.to_string(),
                kind: FileDiffKind::Deleted,
                diff: None,
                additions: None,
                deletions: None,
            }),
            (None, None) => {} // unreachable: paths are the key union
            (Some(af), Some(bf)) => {
                if af.hash == bf.hash {
                    views.push(FileDiffView {
                        path: path.to_string(),
                        kind: FileDiffKind::Unchanged,
                        diff: None,
                        additions: None,
                        deletions: None,
                    });
                    continue;
                }
                let (diff, additions, deletions) = text_diff(&af.content, &bf.content);
                views.push(FileDiffView {
                    path: path.to_string(),
                    kind: FileDiffKind::Modified,
                    diff,
                    additions,
                    deletions,
                });
            }
        }
    }
    views
}

/// Build a unified diff when both contents are valid UTF-8 text, otherwise
/// `(None, None, None)` (binary: reported as changed, never expanded).
fn text_diff(before: &[u8], after: &[u8]) -> (Option<String>, Option<usize>, Option<usize>) {
    let (Ok(before), Ok(after)) = (std::str::from_utf8(before), std::str::from_utf8(after)) else {
        return (None, None, None);
    };
    let diff = unified_diff_text(before, after, 3, None, None);
    let stats = diff_stats_for_text(before, after);
    (
        Some(diff),
        Some(stats.added_lines),
        Some(stats.removed_lines),
    )
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
            (None, Some(_)) => views.push(FileDiffView {
                path: path.to_string(),
                kind: FileDiffKind::Added,
                diff: None,
                additions: None,
                deletions: None,
            }),
            (Some(_), None) => views.push(FileDiffView {
                path: path.to_string(),
                kind: FileDiffKind::Deleted,
                diff: None,
                additions: None,
                deletions: None,
            }),
            (None, None) => {}
            (Some((_, blob_a)), Some((_, blob_b))) => {
                if blob_a == blob_b {
                    views.push(FileDiffView {
                        path: path.to_string(),
                        kind: FileDiffKind::Unchanged,
                        diff: None,
                        additions: None,
                        deletions: None,
                    });
                    continue;
                }
                let bytes_a = git.read_blob(blob_a).map_err(map_git_error)?;
                let bytes_b = git.read_blob(blob_b).map_err(map_git_error)?;
                if sha256_hex(&bytes_a) == sha256_hex(&bytes_b) {
                    views.push(FileDiffView {
                        path: path.to_string(),
                        kind: FileDiffKind::Unchanged,
                        diff: None,
                        additions: None,
                        deletions: None,
                    });
                    continue;
                }
                let (diff, additions, deletions) = text_diff(&bytes_a, &bytes_b);
                views.push(FileDiffView {
                    path: path.to_string(),
                    kind: FileDiffKind::Modified,
                    diff,
                    additions,
                    deletions,
                });
            }
        }
    }
    Ok(views)
}
