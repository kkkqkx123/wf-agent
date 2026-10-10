//! Read-only view types returned by provenance queries.

use crate::approval::ConflictView;

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
    /// `manual` | `agent` | `approval` | `mainline` | `main`.
    pub kind: String,
    /// Actor id for per-actor lines (agent/approval), `None` otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    /// Commit id (hex) of the ref head.
    pub current_snapshot: String,
    /// Number of commits reachable from the head, saturated at the
    /// partition walk cap. A value exactly at the cap means truncated.
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
