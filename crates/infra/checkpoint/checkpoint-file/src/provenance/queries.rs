//! Commit-graph query helpers: partition lines, per-actor and per-path
//! change listings. Index hits win; empty indexes fall back to a bounded
//! graph scan.

use crate::file::git_write::map_git_error;
use crate::file::util::sha256_hex;
use crate::git_store::{
    GitCommit, GitStore, REF_EDIT_PREFIX, REF_FEAT_PREFIX, REF_HUMAN, REF_MAIN, REF_REVIEW_PREFIX,
    TRAILER_ACTOR,
};
use crate::storage::SqliteStorage;
use checkpoint_base::error::CheckpointError;

use super::types::{DeltaSummary, PartitionView};

/// Maximum commits a fallback graph scan may walk when the source index is
/// empty. Larger stores must rebuild the index; cold full scans stay bounded.
const MAX_FALLBACK_SCAN_COMMITS: usize = 5000;

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

/// Commits touching one actor: index fast path, bounded graph scan fallback.
/// The fallback scans at most `MAX_FALLBACK_SCAN_COMMITS` commits; larger
/// stores must rebuild the source index instead of cold scanning.
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
    tracing::warn!(
        actor = %actor,
        "source index empty for actor; falling back to graph scan"
    );
    let all = git.all_commits().map_err(map_git_error)?;
    if all.len() > MAX_FALLBACK_SCAN_COMMITS {
        return Err(CheckpointError::Validation {
            reason: format!(
                "source index empty and graph holds {} commits, exceeding fallback cap {MAX_FALLBACK_SCAN_COMMITS}; rebuild the index",
                all.len()
            ),
        });
    }
    let mut out = Vec::new();
    for commit in all {
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
        ("main".to_string(), None)
    } else if name == REF_HUMAN {
        ("manual".to_string(), None)
    } else if name.strip_prefix(REF_REVIEW_PREFIX).is_some() {
        ("approval".to_string(), head.trailer(TRAILER_ACTOR))
    } else if let Some(name) = name.strip_prefix(REF_FEAT_PREFIX) {
        ("mainline".to_string(), Some(name.to_string()))
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

/// Changes touching `path` across every line: only commits whose tree
/// contains `path` are listed. Similarly-named or similarly-contented files
/// are never folded in: rename following lives in `file_timeline`, and a
/// path history must not silently absorb unrelated files.
pub fn list_changes_by_path(
    git: &GitStore,
    storage: &SqliteStorage,
    path: &str,
    time_range: Option<(i64, i64)>,
) -> Result<Vec<DeltaSummary>, CheckpointError> {
    let indexed = storage.find_commits_by_path(path, 0)?;
    let commits: Vec<GitCommit> = if indexed.is_empty() {
        // Fallback: scan every reachable commit's tree for the path, bounded.
        tracing::warn!(
            path = %path,
            "source index empty for path; falling back to graph scan"
        );
        let all = git.all_commits().map_err(map_git_error)?;
        if all.len() > MAX_FALLBACK_SCAN_COMMITS {
            return Err(CheckpointError::Validation {
                reason: format!(
                    "source index empty and graph holds {} commits, exceeding fallback cap {MAX_FALLBACK_SCAN_COMMITS}; rebuild the index",
                    all.len()
                ),
            });
        }
        let mut found = Vec::new();
        for commit in all {
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
    // Path history is exact: no similarity expansion. A caller that wants
    // rename-spanning history uses `file_timeline`.
    let mut changes = Vec::new();
    for commit in commits {
        if let Some((start, end)) = time_range {
            if commit.committer_ts < start || commit.committer_ts > end {
                continue;
            }
        }
        for summary in summaries_for_commit(git, &commit)? {
            if summary.file == path {
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
