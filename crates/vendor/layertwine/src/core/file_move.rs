use serde::{Deserialize, Serialize};

/// Records a file rename or move operation. When a file is moved from
/// `from_path` to `to_path`, the system creates a `FileMove` record that
/// links the old path's history to the new path's history, enabling
/// chain-of-custody追溯 across renames.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileMove {
    /// Source path (before the move).
    pub from_path: String,
    /// Destination path (after the move).
    pub to_path: String,
    /// Timestamp of the move operation (millis since epoch).
    pub timestamp: i64,
    /// Who performed the move (source label, e.g. "manual", "agent:loop-1").
    pub source: String,
}

impl FileMove {
    /// Create a new file move record.
    pub fn new(from_path: String, to_path: String, source: String) -> Self {
        FileMove {
            from_path,
            to_path,
            timestamp: chrono::Utc::now().timestamp_millis(),
            source,
        }
    }
}

/// A single entry in a file's version timeline. Each entry corresponds to
/// one snapshot that touched the file, optionally preceded by a move
/// operation if the file was renamed before this version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTimelineEntry {
    /// File path at this point in time (may differ from the original if renamed).
    pub path: String,
    /// Snapshot id (hex).
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTimeline {
    /// Original path (the first path this file was known at).
    pub original_path: String,
    /// All versions in chronological order.
    pub entries: Vec<FileTimelineEntry>,
}
