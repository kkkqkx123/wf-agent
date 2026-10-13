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

/// One commit on a partition line, newest first in walk order.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PartitionCommitView {
    /// Commit id (hex).
    pub snapshot_id: String,
    /// Parent commit ids (hex); more than one marks a merge commit.
    pub parents: Vec<String>,
    /// First message line (intent), absent when the message is blank.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Owning actor from the `Wf-Actor` trailer, absent when unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    /// Edit session from the `Wf-Session` trailer, absent when unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    /// Origin tool from the `Wf-Tool` trailer, absent when unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    /// Commit time (Unix milliseconds).
    pub timestamp: i64,
    /// True when the commit has more than one parent.
    pub is_merge: bool,
    /// True when the commit carries the conflict-unresolved state.
    pub has_conflicts: bool,
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

/// One hunk of a per-file difference: the `@@` header plus its raw
/// unified lines (`+`/`-`/context), mirroring the user-repo git shape.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DiffHunkView {
    pub header: String,
    pub lines: Vec<String>,
}

/// Per-file difference view (`diff_actors` / `diff_against_main`), in the
/// same structured shape as the user-repo git diffs: hunk rows plus
/// binary/truncation flags and line counts. Added, deleted and unchanged
/// files carry no hunks.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FileDiffView {
    pub path: String,
    pub kind: FileDiffKind,
    pub hunks: Vec<DiffHunkView>,
    pub additions: usize,
    pub deletions: usize,
    pub binary: bool,
    pub truncated: bool,
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

/// Source-index health for the ops channel (rebuilt via
/// `rebuild_source_index` after loss; queries fall back to graph scans).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct IndexStatus {
    /// Rows present in the source index.
    pub indexed_commits: usize,
    /// Commits reachable in the object store.
    pub total_commits: usize,
    /// True when the index is empty while the graph is non-empty, or
    /// when counts diverge.
    pub needs_rebuild: bool,
}
