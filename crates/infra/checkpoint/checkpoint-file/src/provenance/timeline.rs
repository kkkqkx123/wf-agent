//! Rename-following version timeline: walks the commit graph, linking a
//! name that disappears to similar content appearing under a new name.

use std::collections::{HashMap, HashSet};

use crate::file::git_write::map_git_error;
use crate::file::util::sha256_hex;
use crate::git_store::{GitCommit, GitStore, TRAILER_ACTOR};
use crate::storage::SqliteStorage;
use checkpoint_base::error::CheckpointError;

/// Maximum commits a timeline graph scan may walk when the source index has
/// no row for the path. Larger stores must rebuild the index.
const MAX_TIMELINE_SCAN_COMMITS: usize = 5000;

/// Candidate commits for a timeline: the inverted path index wins, so cost
/// tracks the path's own history instead of the whole store. Only when the
/// index is empty does a bounded full-graph scan run.
fn candidate_commits(
    git: &GitStore,
    storage: &SqliteStorage,
    path: &str,
) -> Result<Vec<GitCommit>, CheckpointError> {
    let indexed = storage.find_commits_by_path(path, 0)?;
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
    tracing::warn!(
        path = %path,
        "source index empty for timeline; falling back to graph scan"
    );
    let all = git.all_commits().map_err(map_git_error)?;
    if all.len() > MAX_TIMELINE_SCAN_COMMITS {
        return Err(CheckpointError::Validation {
            reason: format!(
                "source index empty and graph holds {} commits, exceeding timeline cap {MAX_TIMELINE_SCAN_COMMITS}; rebuild the index",
                all.len()
            ),
        });
    }
    Ok(all)
}

/// Rename similarity threshold aligned with standard rename detection
/// defaults: contents with at least half their lines in common count as
/// the same file moved, so timelines span renames with small edits.
pub const RENAME_SIMILARITY_THRESHOLD: f64 = 0.5;

/// Files shorter than this (line count on either side) only count as a
/// rename on identical bytes: similarity scores on boilerplate headers and
/// tiny files misfire too easily to trust.
pub const RENAME_EXACT_MATCH_MAX_LINES: usize = 8;

fn line_count(bytes: &[u8]) -> usize {
    bytes.iter().filter(|&&b| b == b'\n').count()
}

/// Content similarity for rename following: identical bytes score `1.0`,
/// binary content requires identical bytes, text content uses the retained
/// line-diff similarity. Display still renders via unified diffs.
pub(super) fn content_similarity(a: &[u8], b: &[u8]) -> f64 {
    if a == b {
        return 1.0;
    }
    if checkpoint_base::common::is_binary(a) || checkpoint_base::common::is_binary(b) {
        return 0.0;
    }
    let (Ok(before), Ok(after)) = (std::str::from_utf8(a), std::str::from_utf8(b)) else {
        return 0.0;
    };
    checkpoint_base::common::diff::diff_stats_for_text(before, after).similarity
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
/// rename/move tracing via content-similarity detection (similar contents
/// appearing under a new name while the old name disappears in the same
/// commit count as a rename). Only index-selected candidate commits (plus
/// their parents for rename comparison) are expanded; blob bytes load only
/// for vanished/appeared files, and identical blob ids short-circuit
/// without reading content. No move table is consulted.
pub fn file_timeline(
    git: &GitStore,
    storage: &SqliteStorage,
    path: &str,
) -> Result<FileTimeline, CheckpointError> {
    let mut commits = candidate_commits(git, storage, path)?;
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
        // Rename detection: a name disappearing while similar content
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
        // Deterministic order first: tree maps iterate randomly, so sort
        // both sides by path before scoring. Pairs then resolve in global
        // score order with path tie-breaks, making timelines repeatable.
        disappeared.sort_by(|a, b| a.0.cmp(&b.0));
        appeared.sort_by(|a, b| a.0.cmp(&b.0));
        // Blob bytes load once per id: vanished/appeared blobs are read a
        // single time and shared across the score matrix. Unreadable blobs
        // make their pairs ineligible instead of failing the timeline.
        let mut blob_bytes: HashMap<&str, Vec<u8>> = HashMap::new();
        for (_, blob) in disappeared.iter().chain(appeared.iter()) {
            if !blob_bytes.contains_key(blob.as_str()) {
                if let Ok(bytes) = git.read_blob(blob) {
                    blob_bytes.insert(blob.as_str(), bytes);
                }
            }
        }
        let pair_score =
            |old_blob: &str, new_blob: &str, cache: &HashMap<&str, Vec<u8>>| -> Option<f64> {
                // Identical blob ids short-circuit without byte reads.
                if old_blob == new_blob {
                    return Some(1.0);
                }
                let (Some(old_bytes), Some(new_bytes)) = (cache.get(old_blob), cache.get(new_blob))
                else {
                    return None;
                };
                // Small files require identical bytes (checked above):
                // similarity on a handful of lines is boilerplate noise.
                if line_count(old_bytes) < RENAME_EXACT_MATCH_MAX_LINES
                    || line_count(new_bytes) < RENAME_EXACT_MATCH_MAX_LINES
                {
                    return None;
                }
                let score = content_similarity(old_bytes, new_bytes);
                (score >= RENAME_SIMILARITY_THRESHOLD).then_some(score)
            };
        // Score matrix over index pairs.
        let mut pairs: Vec<(usize, usize, f64)> = Vec::new();
        for (oi, (_, old_blob)) in disappeared.iter().enumerate() {
            for (ni, (_, new_blob)) in appeared.iter().enumerate() {
                if let Some(score) = pair_score(old_blob, new_blob, &blob_bytes) {
                    pairs.push((oi, ni, score));
                }
            }
        }
        pairs.sort_by(|a, b| {
            b.2.partial_cmp(&a.2)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(disappeared[a.0].0.cmp(&disappeared[b.0].0))
                .then(appeared[a.1].0.cmp(&appeared[b.1].0))
        });
        let mut link = |new_name: &str, old_name: &str| {
            moved_from
                .entry(new_name.to_string())
                .or_insert_with(|| old_name.to_string());
            if known_names.contains(old_name) {
                known_names.insert(new_name.to_string());
            }
        };
        // Phase one: one-to-one main-chain links in global score order, so
        // competing olds no longer depend on iteration luck.
        let mut used_old = vec![false; disappeared.len()];
        let mut used_new = vec![false; appeared.len()];
        for (oi, ni, _) in &pairs {
            if used_old[*oi] || used_new[*ni] {
                continue;
            }
            used_old[*oi] = true;
            used_new[*ni] = true;
            link(&appeared[*ni].0.clone(), &disappeared[*oi].0.clone());
        }
        // Phase two: split branches. An unmatched appearance still linking
        // to an old name becomes a same-origin branch entry instead of a
        // forged single chain; the first (highest-score) link keeps the
        // main `moved_from`.
        for (oi, ni, _) in &pairs {
            if used_new[*ni] {
                continue;
            }
            used_new[*ni] = true;
            link(&appeared[*ni].0.clone(), &disappeared[*oi].0.clone());
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
