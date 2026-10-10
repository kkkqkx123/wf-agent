//! File-checkpoint value types: stored state, projections, metadata,
//! content entries, options and restore results.

use std::collections::HashMap;

use wf_types::config::file_checkpoint::FailureBehavior;

fn is_false(v: &bool) -> bool {
    !*v
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct FileState {
    pub path: String,
    pub hash: String,
    pub size: u64,
    pub last_modified: i64,
    /// Deletion projection marker: the actor partition
    /// stores an empty content for the path; `deleted = true` excludes the
    /// path from workspace restores so the file is removed from the
    /// workspace. Skipped when `false` to keep the historical JSON shape.
    #[serde(default, skip_serializing_if = "is_false")]
    pub deleted: bool,
}

/// Lightweight projection of a file checkpoint.
///
/// The authoritative model lives in the Git object store; this struct is
/// the read model exposed to coordinators / API consumers, keeping the
/// historical field shape so downstream code stays unchanged. Every
/// checkpoint is a "full" projection of the actor partition's latest
/// per-file state.
///
/// Naming: `FileProjection` is the preferred alias at new call sites; the
/// `FileCheckpoint` name is retained for the wire shape.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct FileCheckpoint {
    /// File checkpoint id.
    pub id: String,
    /// Checkpoint creation time (Unix milliseconds).
    pub timestamp: i64,
    pub full_hash: String,
    pub files: Vec<FileState>,
    /// Directories that contained no files at snapshot time; recreated on
    /// workspace restore. Kept in the projection index.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub empty_dirs: Option<Vec<String>>,
}

/// Preferred alias for new call sites: this value is always a full
/// projection, never an incremental delta chain.
pub type FileProjection = FileCheckpoint;

/// Metadata for indexing and querying file checkpoints.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct FileCheckpointMetadata {
    pub id: String,
    pub entity_id: String,
    pub timestamp: i64,
    pub checkpoint_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_checkpoint_id: Option<String>,
    pub file_count: u64,
    pub full_hash: String,
    pub total_size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_fields: Option<HashMap<String, serde_json::Value>>,
}

impl From<&FileCheckpoint> for FileCheckpointMetadata {
    fn from(checkpoint: &FileCheckpoint) -> Self {
        Self {
            id: checkpoint.id.clone(),
            entity_id: String::new(),
            timestamp: checkpoint.timestamp,
            checkpoint_type: "full".to_string(),
            base_checkpoint_id: None,
            file_count: checkpoint.files.len() as u64,
            full_hash: checkpoint.full_hash.clone(),
            total_size: checkpoint.files.iter().map(|f| f.size).sum(),
            tags: None,
            custom_fields: None,
        }
    }
}

/// A file's path and full content, used by content-level file checkpointing.
#[derive(Debug, Clone)]
pub struct FileContentEntry {
    pub path: String,
    pub content: Vec<u8>,
    pub deleted: bool,
}

impl FileContentEntry {
    pub fn new(path: impl Into<String>, content: Vec<u8>) -> Self {
        Self {
            path: path.into(),
            content,
            deleted: false,
        }
    }

    pub fn deleted(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            content: Vec::new(),
            deleted: true,
        }
    }
}

/// Options controlling checkpoint decisions and per-file error tolerance.
#[derive(Debug, Clone)]
pub struct FileCheckpointOptions {
    /// Per-file error handling during scan/restore.
    pub failure_behavior: FailureBehavior,
    /// Additional ignore patterns applied while scanning the workspace.
    pub custom_ignore_patterns: Vec<String>,
}

impl Default for FileCheckpointOptions {
    fn default() -> Self {
        Self {
            failure_behavior: FailureBehavior::Warn,
            custom_ignore_patterns: Vec::new(),
        }
    }
}

/// Result of a workspace-aligned restore.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WorkspaceRestoreResult {
    /// Files written back to disk.
    pub restored: usize,
    /// Extra files deleted from the workspace.
    pub deleted: usize,
    /// Files already matching the target state (skipped).
    pub skipped: usize,
}
