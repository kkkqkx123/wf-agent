//! Rename-following version timeline: walks the commit graph, linking a
//! name that disappears to similar content appearing under a new name.

use std::collections::{HashMap, HashSet};

use crate::file::git_write::map_git_error;
use crate::file::util::sha256_hex;
use crate::git_store::{GitStore, TRAILER_ACTOR};
use crate::storage::SqliteStorage;
use checkpoint_base::error::CheckpointError;

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
        for (new_name, new_blob) in &appeared {
            let Ok(new_bytes) = git.read_blob(new_blob) else {
                continue;
            };
            let mut best: Option<(String, f64)> = None;
            for (old_name, old_blob) in &disappeared {
                if old_blob == new_blob {
                    best = Some((old_name.clone(), 1.0));
                    break;
                }
                let Ok(old_bytes) = git.read_blob(old_blob) else {
                    continue;
                };
                // Small files require identical bytes (checked above):
                // similarity on a handful of lines is boilerplate noise.
                if line_count(&old_bytes) < RENAME_EXACT_MATCH_MAX_LINES
                    || line_count(&new_bytes) < RENAME_EXACT_MATCH_MAX_LINES
                {
                    continue;
                }
                let score = content_similarity(&old_bytes, &new_bytes);
                if score >= RENAME_SIMILARITY_THRESHOLD
                    && best.as_ref().is_none_or(|(_, s)| score > *s)
                {
                    best = Some((old_name.clone(), score));
                }
            }
            if let Some((old_name, _)) = best {
                moved_from
                    .entry(new_name.clone())
                    .or_insert_with(|| old_name.clone());
                if known_names.contains(&old_name) {
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
