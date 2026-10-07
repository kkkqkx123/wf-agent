//! Provenance queries over the commit DAG.
//!
//! Reads expand trees and walk the commit graph; the source index only
//! accelerates hot queries (actor / path) and every query falls back to a
//! graph scan when the index is empty or damaged. Diffs render with the
//! retained line-diff display capability (presentation only, never used
//! for storage addressing).

use std::collections::{HashMap, HashSet};

use crate::storage::SqliteStorage;

use crate::approval::ConflictView;
use crate::file::git_write::map_git_error;
use crate::file::util::sha256_hex;
use crate::file::FileContentEntry;
use crate::git_store::{
    edit_ref_for_actor, feat_ref_for_name, GitCommit, GitStore, REF_EDIT_PREFIX, REF_FEAT_PREFIX,
    REF_HUMAN, REF_MAIN, REF_REVIEW_PREFIX, TRAILER_ACTOR,
};
use checkpoint_base::common::diff::{diff_stats_for_text, unified_diff_text};
use checkpoint_base::error::CheckpointError;

/// One recorded change of a commit.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DeltaSummary {
    /// Relative file path.
    pub file: String,
    /// Origin: actor id / `human` / `merge` / `review`.
    pub source: String,
    /// Change time (Unix milliseconds).
    pub timestamp: i64,
    /// Commit id (hex).
    pub snapshot_id: String,
    /// Content hash (SHA-256 hex) of the resulting file bytes.
    pub hash: String,
    /// Optional human-readable description of the edit intent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Read view of a ref line (the branch-pointer replacement).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PartitionView {
    pub partition_id: String,
    pub name: String,
    /// `manual` | `agent` | `approval` | `integrated` | `staged`.
    pub kind: String,
    /// Actor id for per-actor lines (agent/approval), `None` otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    /// Commit id (hex) of the ref head.
    pub current_snapshot: String,
    /// Number of commits reachable from the head.
    pub history_len: usize,
    /// Creation time of the oldest reachable commit.
    pub created_at: i64,
    /// Time of the head commit.
    pub updated_at: i64,
}

/// File content of a workspace view at its current ref state
/// (`get_actor_workspace`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WorkspaceFile {
    pub path: String,
    pub content: Vec<u8>,
    pub hash: String,
    pub timestamp: i64,
}

/// Kind of a per-file difference between two workspace states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileDiffKind {
    Added,
    Modified,
    Deleted,
    Unchanged,
}

/// Per-file difference view (`diff_actors` /.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FileDiffView {
    pub path: String,
    pub kind: FileDiffKind,
    /// Unified diff (text files only); `None` for binary content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additions: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deletions: Option<usize>,
}

/// Whether a path matches the optional filter (plain substring match).
fn path_matches(path: &str, filter: Option<&str>) -> bool {
    match filter {
        Some(filter) if !filter.is_empty() => path.contains(filter),
        _ => true,
    }
}

fn intent_line(message: &str) -> Option<String> {
    message
        .lines()
        .next()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
}

/// Expand one commit into per-file change summaries.
fn summaries_for_commit(
    git: &GitStore,
    commit: &GitCommit,
) -> Result<Vec<DeltaSummary>, CheckpointError> {
    let files = git.tree_to_bytes(&commit.tree).map_err(map_git_error)?;
    let source = commit
        .trailer(TRAILER_ACTOR)
        .unwrap_or_else(|| "agent".to_string());
    let message = intent_line(&commit.message);
    let mut out: Vec<DeltaSummary> = files
        .iter()
        .map(|(path, bytes)| DeltaSummary {
            file: path.clone(),
            source: source.clone(),
            timestamp: commit.committer_ts,
            snapshot_id: commit.id.clone(),
            hash: sha256_hex(bytes),
            message: message.clone(),
        })
        .collect();
    out.sort_by(|a, b| a.file.cmp(&b.file));
    Ok(out)
}

/// Commits touching one actor: index fast path, graph scan fallback.
fn commits_for_actor(
    git: &GitStore,
    storage: &SqliteStorage,
    actor: &str,
) -> Result<Vec<GitCommit>, CheckpointError> {
    let indexed = storage.find_commits_by_actor(actor, 0)?;
    if !indexed.is_empty() {
        let mut out = Vec::with_capacity(indexed.len());
        for entry in indexed {
            if let Ok(commit) = git.read_commit(&entry.commit_id) {
                out.push(commit);
            }
        }
        if !out.is_empty() {
            return Ok(out);
        }
    }
    // Fallback: scan the graph for the actor trailer.
    let mut out = Vec::new();
    for commit in git.all_commits().map_err(map_git_error)? {
        if commit.trailer(TRAILER_ACTOR).as_deref() == Some(actor) {
            out.push(commit);
        }
    }
    Ok(out)
}

/// All refs in deterministic order.
fn all_refs(git: &GitStore) -> Result<Vec<(String, String)>, CheckpointError> {
    git.list_refs("refs/wf/").map_err(map_git_error)
}

/// Classify a ref into the stable partition-kind vocabulary.
fn classify_ref(name: &str, head: &GitCommit) -> (String, Option<String>) {
    if name == REF_MAIN {
        ("staged".to_string(), None)
    } else if name == REF_HUMAN {
        ("manual".to_string(), None)
    } else if name.strip_prefix(REF_REVIEW_PREFIX).is_some() {
        ("approval".to_string(), head.trailer(TRAILER_ACTOR))
    } else if let Some(name) = name.strip_prefix(REF_FEAT_PREFIX) {
        ("integrated".to_string(), Some(name.to_string()))
    } else if let Some(actor) = name.strip_prefix(REF_EDIT_PREFIX) {
        ("agent".to_string(), Some(actor.to_string()))
    } else {
        ("agent".to_string(), None)
    }
}

/// All ref lines ordered by name (stable for tests).
pub fn list_partitions(
    git: &GitStore,
    _storage: &SqliteStorage,
) -> Result<Vec<PartitionView>, CheckpointError> {
    let mut refs = all_refs(git)?;
    refs.sort_by(|a, b| a.0.cmp(&b.0));
    let mut views = Vec::with_capacity(refs.len());
    for (name, head_id) in refs {
        let Ok(head) = git.read_commit(&head_id) else {
            continue;
        };
        let log = git.log(&head_id, 0).map_err(map_git_error)?;
        let created_at = log
            .iter()
            .map(|c| c.committer_ts)
            .min()
            .unwrap_or(head.committer_ts);
        let (kind, actor) = classify_ref(&name, &head);
        views.push(PartitionView {
            partition_id: name.clone(),
            name,
            kind,
            actor,
            current_snapshot: head_id,
            history_len: log.len(),
            created_at,
            updated_at: head.committer_ts,
        });
    }
    Ok(views)
}

/// Changes recorded on an actor's line, in chronological order.
///
/// `path_filter` is a plain substring match; `time_range` is
/// `[start, end]` milliseconds (inclusive), `None` = unbounded.
pub fn list_changes_by_actor(
    git: &GitStore,
    storage: &SqliteStorage,
    actor: &str,
    path_filter: Option<&str>,
    time_range: Option<(i64, i64)>,
) -> Result<Vec<DeltaSummary>, CheckpointError> {
    let mut commits = commits_for_actor(git, storage, actor)?;
    commits.sort_by(|a, b| a.committer_ts.cmp(&b.committer_ts).then(a.id.cmp(&b.id)));
    let mut changes = Vec::new();
    for commit in commits {
        if let Some((start, end)) = time_range {
            if commit.committer_ts < start || commit.committer_ts > end {
                continue;
            }
        }
        for summary in summaries_for_commit(git, &commit)? {
            if path_matches(&summary.file, path_filter) {
                changes.push(summary);
            }
        }
    }
    Ok(changes)
}

/// Changes touching `path` across every line, following renames: an
/// identical-content add paired with a same-commit delete counts as a
/// rename, so the timeline spans both names. `time_range` (inclusive
/// `(start, end)` timestamps) narrows the window.
pub fn list_changes_by_path(
    git: &GitStore,
    storage: &SqliteStorage,
    path: &str,
    time_range: Option<(i64, i64)>,
) -> Result<Vec<DeltaSummary>, CheckpointError> {
    let indexed = storage.find_commits_by_path(path, 0)?;
    let commits: Vec<GitCommit> = if indexed.is_empty() {
        // Fallback: scan every reachable commit's tree for the path.
        let mut found = Vec::new();
        for commit in git.all_commits().map_err(map_git_error)? {
            let Ok(files) = git.tree_to_files(&commit.tree) else {
                continue;
            };
            if files.contains_key(path) {
                found.push(commit);
            }
        }
        found
    } else {
        indexed
            .into_iter()
            .filter_map(|entry| git.read_commit(&entry.commit_id).ok())
            .collect()
    };
    // Rename follow: same-content blobs appearing under other names in the
    // same commits extend the path set (similarity = identical bytes).
    let mut names: HashSet<String> = HashSet::from([path.to_string()]);
    for commit in &commits {
        let Ok(files) = git.tree_to_bytes(&commit.tree) else {
            continue;
        };
        let Some(want) = files.get(path) else {
            continue;
        };
        for (other, bytes) in &files {
            if bytes == want {
                names.insert(other.clone());
            }
        }
    }
    let mut changes = Vec::new();
    for commit in commits {
        if let Some((start, end)) = time_range {
            if commit.committer_ts < start || commit.committer_ts > end {
                continue;
            }
        }
        for summary in summaries_for_commit(git, &commit)? {
            if names.contains(&summary.file) {
                changes.push(summary);
            }
        }
    }
    changes.sort_by(|a, b| {
        a.timestamp
            .cmp(&b.timestamp)
            .then(a.snapshot_id.cmp(&b.snapshot_id))
            .then(a.file.cmp(&b.file))
    });
    Ok(changes)
}

/// Read one ref line's tree into workspace files.
fn workspace_files_for_tree(
    git: &GitStore,
    tree: &str,
    timestamp: i64,
) -> Result<Vec<WorkspaceFile>, CheckpointError> {
    let files = git.tree_to_bytes(tree).map_err(map_git_error)?;
    let mut out: Vec<WorkspaceFile> = files
        .into_iter()
        .map(|(path, content)| {
            let hash = sha256_hex(&content);
            WorkspaceFile {
                path,
                content,
                hash,
                timestamp,
            }
        })
        .collect();
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// Reconstructed file set of an actor's edit line
/// (`get_actor_workspace`).
pub fn get_actor_workspace(
    git: &GitStore,
    actor: &str,
) -> Result<Vec<WorkspaceFile>, CheckpointError> {
    let head = git
        .read_ref(&edit_ref_for_actor(actor))
        .map_err(map_git_error)?
        .ok_or_else(|| CheckpointError::NotFound {
            id: format!("actor workspace for '{actor}'"),
        })?;
    let commit = git.read_commit(&head).map_err(map_git_error)?;
    workspace_files_for_tree(git, &commit.tree, commit.committer_ts)
}

/// The main line's reconstructed file set (`diff_against_staged` base).
pub fn get_staged_workspace(git: &GitStore) -> Result<Vec<WorkspaceFile>, CheckpointError> {
    let head = git
        .read_ref(REF_MAIN)
        .map_err(map_git_error)?
        .ok_or_else(|| CheckpointError::NotFound {
            id: "main workspace".to_string(),
        })?;
    let commit = git.read_commit(&head).map_err(map_git_error)?;
    workspace_files_for_tree(git, &commit.tree, commit.committer_ts)
}

/// The feature line's reconstructed file set.
pub fn get_feature_workspace(
    git: &GitStore,
    feature: &str,
) -> Result<Vec<WorkspaceFile>, CheckpointError> {
    let head = git
        .read_ref(&feat_ref_for_name(feature))
        .map_err(map_git_error)?
        .ok_or_else(|| CheckpointError::NotFound {
            id: format!("feature '{feature}'"),
        })?;
    let commit = git.read_commit(&head).map_err(map_git_error)?;
    workspace_files_for_tree(git, &commit.tree, commit.committer_ts)
}

/// A file whose merge commit carries the unresolved-conflict flag.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ConflictFile {
    /// Relative file path.
    pub path: String,
    /// Commit id (hex) of the conflicted merge commit.
    pub snapshot_id: String,
    /// Ref the conflict lives on (`refs/wf/feat/*` or `refs/wf/main`).
    pub partition: String,
    /// Conflict regions re-derived from the standard markers on disk.
    pub conflicts: Vec<ConflictView>,
}

/// Parse standard conflict markers from stored bytes into read views.
pub fn parse_marker_conflicts(path: &str, bytes: &[u8]) -> Vec<ConflictView> {
    let text = String::from_utf8_lossy(bytes);
    let mut out = Vec::new();
    let mut ours: Vec<String> = Vec::new();
    let mut theirs: Vec<String> = Vec::new();
    let mut state = 0u8;
    let mut start_line = 0usize;
    for (idx, line) in text.lines().enumerate() {
        match (state, line) {
            (0, "<<<<<<< ours") => {
                state = 1;
                start_line = idx;
                ours.clear();
                theirs.clear();
            }
            (1, "=======") => state = 2,
            (2, l) if l.starts_with(">>>>>>>") => {
                state = 0;
                out.push(ConflictView {
                    file: path.to_string(),
                    start_line,
                    base: vec![],
                    ours: std::mem::take(&mut ours),
                    theirs: std::mem::take(&mut theirs),
                });
            }
            (1, l) => ours.push(l.to_string()),
            (2, l) => theirs.push(l.to_string()),
            _ => {}
        }
    }
    out
}

/// List files with unresolved merge conflicts across main and every
/// feature ref, enumerated from unresolved merge commits (never by
/// replaying history). Regions come from the standard markers stored in
/// the conflicted files.
pub fn list_conflicts(git: &GitStore) -> Result<Vec<ConflictFile>, CheckpointError> {
    let mut out = Vec::new();
    for (refname, commit_id, files) in git.list_unresolved_conflicts().map_err(map_git_error)? {
        let commit = git.read_commit(&commit_id).map_err(map_git_error)?;
        let stored = git.tree_to_bytes(&commit.tree).map_err(map_git_error)?;
        for file in files {
            let conflicts = stored
                .get(&file)
                .map(|bytes| parse_marker_conflicts(&file, bytes))
                .unwrap_or_default();
            out.push(ConflictFile {
                path: file,
                snapshot_id: commit_id.clone(),
                partition: refname.clone(),
                conflicts,
            });
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
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
pub fn diff_against_staged(
    git: &GitStore,
    actor: &str,
) -> Result<Vec<FileDiffView>, CheckpointError> {
    let actor_files = get_actor_workspace(git, actor)?;
    let staged_files = get_staged_workspace(git)?;
    Ok(diff_workspaces(&actor_files, &staged_files))
}

/// Read-only provenance service borrowing the stores.
///
/// Splits query ownership out of `FileCheckpointManager`: orchestration code
/// builds a reader from the manager's handles, while the manager's own
/// query methods delegate here to keep one implementation.
pub struct ProvenanceReader<'a> {
    git: &'a GitStore,
    storage: &'a SqliteStorage,
}

impl<'a> ProvenanceReader<'a> {
    pub fn new(git: &'a GitStore, storage: &'a SqliteStorage) -> Self {
        Self { git, storage }
    }

    pub fn list_partitions(&self) -> Result<Vec<PartitionView>, CheckpointError> {
        list_partitions(self.git, self.storage)
    }

    pub fn list_changes_by_actor(
        &self,
        actor: &str,
        path_filter: Option<&str>,
        time_range: Option<(i64, i64)>,
    ) -> Result<Vec<DeltaSummary>, CheckpointError> {
        list_changes_by_actor(self.git, self.storage, actor, path_filter, time_range)
    }

    pub fn list_changes_by_path(
        &self,
        path: &str,
        time_range: Option<(i64, i64)>,
    ) -> Result<Vec<DeltaSummary>, CheckpointError> {
        list_changes_by_path(self.git, self.storage, path, time_range)
    }

    pub fn get_actor_workspace(&self, actor: &str) -> Result<Vec<WorkspaceFile>, CheckpointError> {
        get_actor_workspace(self.git, actor)
    }

    pub fn diff_actors(
        &self,
        actor_a: &str,
        actor_b: &str,
    ) -> Result<Vec<FileDiffView>, CheckpointError> {
        diff_actors(self.git, actor_a, actor_b)
    }

    pub fn diff_against_staged(&self, actor: &str) -> Result<Vec<FileDiffView>, CheckpointError> {
        diff_against_staged(self.git, actor)
    }

    pub fn list_conflicts(&self) -> Result<Vec<ConflictFile>, CheckpointError> {
        list_conflicts(self.git)
    }

    pub fn file_timeline(&self, path: &str) -> Result<FileTimeline, CheckpointError> {
        file_timeline(self.git, self.storage, path)
    }
}

/// Convert a workspace file set into content entries (used by restore
/// callers / API projections).
pub fn workspace_entries(files: &[WorkspaceFile]) -> Vec<FileContentEntry> {
    files
        .iter()
        .map(|f| FileContentEntry::new(f.path.clone(), f.content.clone()))
        .collect()
}

/// A single entry in a file's version timeline, including optional move context.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FileTimelineEntry {
    /// File path at this point in time.
    pub path: String,
    /// Commit id (hex).
    pub snapshot_id: String,
    /// Content hash (SHA-256 hex) of the resulting file bytes.
    pub content_hash: String,
    /// Change time (Unix milliseconds).
    pub timestamp: i64,
    /// Origin label (e.g. "manual", "agent:loop-1").
    pub source: String,
    /// If this entry follows a rename, the previous path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub moved_from: Option<String>,
}

/// Complete timeline for a file, including renames/moves.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FileTimeline {
    /// Original path (the first path this file was known at).
    pub original_path: String,
    /// All versions in chronological order.
    pub entries: Vec<FileTimelineEntry>,
}

/// Build the complete version timeline for a file path, including
/// rename/move tracing via content-similarity detection (identical bytes
/// appearing under a new name while the old name disappears in the same
/// commit count as a rename). Walks the commit graph with rename
/// following; no move table is consulted.
pub fn file_timeline(
    git: &GitStore,
    _storage: &SqliteStorage,
    path: &str,
) -> Result<FileTimeline, CheckpointError> {
    let mut commits = git.all_commits().map_err(map_git_error)?;
    commits.sort_by(|a, b| a.committer_ts.cmp(&b.committer_ts).then(a.id.cmp(&b.id)));
    // Current blob per path as we sweep chronologically, so renames link
    // across consecutive commits touching the file.
    let mut entries: Vec<FileTimelineEntry> = Vec::new();
    let mut known_names: HashSet<String> = HashSet::from([path.to_string()]);
    let mut moved_from: HashMap<String, String> = HashMap::new();
    let mut previous_blobs: HashMap<String, String> = HashMap::new();
    for commit in &commits {
        let Ok(files) = git.tree_to_files(&commit.tree) else {
            continue;
        };
        // Rename detection: a name disappearing while an identical blob
        // appears under a new name in the same commit links the two.
        let mut disappeared: Vec<(String, String)> = Vec::new();
        let mut appeared: Vec<(String, String)> = Vec::new();
        for parent_id in &commit.parents {
            let Ok(parent) = git.read_commit(parent_id) else {
                continue;
            };
            let Ok(parent_files) = git.tree_to_files(&parent.tree) else {
                continue;
            };
            for (name, (_, blob)) in &parent_files {
                if !files.contains_key(name) {
                    disappeared.push((name.clone(), blob.clone()));
                }
            }
            for (name, (_, blob)) in &files {
                if !parent_files.contains_key(name) {
                    appeared.push((name.clone(), blob.clone()));
                }
            }
        }
        for (new_name, blob) in &appeared {
            if let Some((old_name, _)) = disappeared.iter().find(|(_, b)| b == blob) {
                moved_from
                    .entry(new_name.clone())
                    .or_insert_with(|| old_name.clone());
                if known_names.contains(old_name) {
                    known_names.insert(new_name.clone());
                }
            }
        }
        for name in known_names.clone() {
            if let Some((_, blob)) = files.get(&name) {
                if previous_blobs.get(&name) != Some(blob) {
                    let bytes = git.read_blob(blob).map_err(map_git_error)?;
                    let source = commit
                        .trailer(TRAILER_ACTOR)
                        .unwrap_or_else(|| "agent".to_string());
                    entries.push(FileTimelineEntry {
                        moved_from: moved_from.get(&name).cloned(),
                        path: name.clone(),
                        snapshot_id: commit.id.clone(),
                        content_hash: sha256_hex(&bytes),
                        timestamp: commit.committer_ts,
                        source,
                    });
                    previous_blobs.insert(name, blob.clone());
                }
            } else {
                previous_blobs.remove(&name);
            }
        }
    }
    let original_path = moved_from
        .get(path)
        .cloned()
        .unwrap_or_else(|| path.to_string());
    Ok(FileTimeline {
        original_path,
        entries,
    })
}

/// Rebuild the source index from the commit graph: drop every row, then
/// re-record one entry per reachable commit. Used after index loss and
/// after bulk ref deletions.
pub fn rebuild_source_index(
    git: &GitStore,
    storage: &SqliteStorage,
) -> Result<usize, CheckpointError> {
    storage.clear_source_index()?;
    let mut commits = git.all_commits().map_err(map_git_error)?;
    commits.sort_by_key(|a| a.committer_ts);
    let mut count = 0;
    for commit in commits {
        let files = git.tree_to_files(&commit.tree).map_err(map_git_error)?;
        let mut paths: Vec<String> = files.into_keys().collect();
        paths.sort();
        storage.record_source_index(&crate::storage::SourceIndexEntry {
            commit_id: commit.id.clone(),
            actor: commit.trailer(TRAILER_ACTOR).unwrap_or_default(),
            session: commit
                .trailer(crate::git_store::TRAILER_SESSION)
                .unwrap_or_default(),
            tool: commit
                .trailer(crate::git_store::TRAILER_TOOL)
                .unwrap_or_default(),
            paths,
            timestamp: commit.committer_ts,
        })?;
        count += 1;
    }
    Ok(count)
}

/// Unresolved merge commits and their conflict files (conflict-list view).
pub fn unresolved_merges(
    git: &GitStore,
) -> Result<Vec<(String, String, Vec<String>)>, CheckpointError> {
    git.list_unresolved_conflicts().map_err(map_git_error)
}
