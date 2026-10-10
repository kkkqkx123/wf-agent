//! Presentation-only diff rendering between two workspace states.

use std::collections::HashMap;

use crate::git_store::GitStore;
use checkpoint_base::common::diff::{diff_stats_for_text, unified_diff_text};
use checkpoint_base::error::CheckpointError;

use super::types::{FileDiffKind, FileDiffView, WorkspaceFile};
use super::workspace::{get_actor_workspace, get_main_workspace};

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

/// Diff between two actor workspaces.
pub fn diff_actors(
    git: &GitStore,
    actor_a: &str,
    actor_b: &str,
) -> Result<Vec<FileDiffView>, CheckpointError> {
    let a = get_actor_workspace(git, actor_a)?;
    let b = get_actor_workspace(git, actor_b)?;
    Ok(diff_workspaces(&a, &b))
}

/// Diff between an actor workspace and the main line.
pub fn diff_against_main(
    git: &GitStore,
    actor: &str,
) -> Result<Vec<FileDiffView>, CheckpointError> {
    let actor_files = get_actor_workspace(git, actor)?;
    let main_files = get_main_workspace(git)?;
    Ok(diff_workspaces(&actor_files, &main_files))
}
