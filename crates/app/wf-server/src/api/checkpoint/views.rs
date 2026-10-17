//! Named response views for the checkpoint domain.
//!
//! utoipa annotations need schema-visible types; the domain returns infra
//! structs and plain scalars that must not grow `ToSchema` impls (transport
//! documentation stays transport-owned). Each view mirrors the exact JSON
//! shape of what `envelope::ok` serializes today — including the domain
//! `skip_serializing_if` attributes — so switching an annotation from an
//! untyped envelope to a named view changes the documented schema only;
//! the response bytes are unchanged.

use serde::Serialize;
use utoipa::ToSchema;

/// Empty payload (`data: null`, e.g. the metadata PUT): utoipa cannot
/// render `()` in a generic envelope, so the unit response is named.
#[derive(Serialize, ToSchema)]
pub(crate) struct UnitDoc {}

/// Summary view of a created file checkpoint
/// (`wf_api::checkpoint::file::FileCheckpointSummary` mirrored).
#[derive(Serialize, ToSchema)]
pub(crate) struct FileCheckpointSummaryDoc {
    /// Commit id (hex) of the created workspace checkpoint.
    id: String,
    /// Creation time (Unix milliseconds).
    timestamp: i64,
    /// Number of files captured.
    file_count: usize,
}

/// Source-index health (`wf_checkpoint::provenance::IndexStatus` mirrored):
/// a non-empty graph with an empty (or diverging) index sets
/// `needs_rebuild`.
#[derive(Serialize, ToSchema)]
pub(crate) struct IndexStatusDoc {
    /// Rows present in the source index.
    indexed_commits: usize,
    /// Commits reachable in the object store.
    total_commits: usize,
    /// True when the index is empty while the graph is non-empty, or when
    /// counts diverge.
    needs_rebuild: bool,
}

/// Entity metadata document: a bare map of arbitrary JSON values, or
/// `null` when the entity has no metadata (transparent over the stored
/// `HashMap<String, serde_json::Value>`).
#[derive(Serialize, ToSchema)]
#[serde(transparent)]
pub(crate) struct EntityMetadataDoc {
    #[schema(value_type = Object)]
    metadata: Option<std::collections::HashMap<String, serde_json::Value>>,
}

/// One stored execution checkpoint (`wf_types::Checkpoint` mirrored).
#[derive(Serialize, ToSchema)]
pub(crate) struct CheckpointDoc {
    id: String,
    entity_type: String,
    entity_id: String,
    /// Forward link to the parent execution, when the owning entity has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_entity_id: Option<String>,
    /// Wire-compatible `"FULL"` / `"DELTA"` literal union.
    checkpoint_type: String,
    /// Record time (Unix milliseconds).
    timestamp: i64,
    /// `active` | `completed` | `expired` | `corrupted` (snake_case).
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    previous_checkpoint_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    base_checkpoint_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chain_root_id: Option<String>,
    /// Position within the checkpoint chain, when chained.
    #[serde(skip_serializing_if = "Option::is_none")]
    chain_position: Option<u32>,
    /// Encoded blob size in bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    blob_size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    custom_fields: Option<std::collections::HashMap<String, serde_json::Value>>,
}

/// One transition between consecutive checkpoints of a chain
/// (`wf_api::checkpoint::record::CheckpointTransitionView` mirrored).
#[derive(Serialize, ToSchema)]
pub(crate) struct CheckpointTransitionDoc {
    from_checkpoint_id: String,
    to_checkpoint_id: String,
    /// Milliseconds between the two checkpoints.
    elapsed: i64,
    /// `FULL` | `DELTA`.
    r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    trigger_description: Option<String>,
}

/// Start / end timestamps spanned by a checkpoint chain.
#[derive(Serialize, ToSchema)]
pub(crate) struct CheckpointTimeRangeDoc {
    start: i64,
    end: i64,
}

/// Capped chain analysis of one execution's checkpoints (the transport
/// `CappedChainView` mirrored): full structure up to a hard cap with an
/// explicit truncation flag and pre-truncation total.
#[derive(Serialize, ToSchema)]
pub(crate) struct CappedChainDoc {
    execution_id: String,
    checkpoints: Vec<CheckpointDoc>,
    transitions: Vec<CheckpointTransitionDoc>,
    total_elapsed: i64,
    checkpoint_count: usize,
    time_range: CheckpointTimeRangeDoc,
    truncated: bool,
    total: usize,
}

/// Restored execution state (`wf_api::RestoredCheckpoint` mirrored); the
/// live entity handle is `#[serde(skip)]` and never serializes.
#[derive(Serialize, ToSchema)]
pub(crate) struct RestoredCheckpointDoc {
    checkpoint_id: String,
    execution_id: String,
    status: String,
    current_node_id: Option<String>,
    node_results: Option<std::collections::HashMap<String, serde_json::Value>>,
    variables: std::collections::BTreeMap<String, serde_json::Value>,
}

/// One file change recorded on an actor partition
/// (`wf_checkpoint::provenance::DeltaSummary` mirrored).
#[derive(Serialize, ToSchema)]
pub(crate) struct DeltaSummaryDoc {
    /// Relative file path.
    file: String,
    /// Origin: actor id / `human` / `merge` / `review`.
    source: String,
    /// Change time (Unix milliseconds).
    timestamp: i64,
    /// Commit id (hex).
    snapshot_id: String,
    /// Content hash (SHA-256 hex) of the resulting file bytes.
    hash: String,
    /// Optional human-readable description of the edit intent.
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

/// Read view of one ref line in the checkpoint store
/// (`wf_checkpoint::provenance::PartitionView` mirrored).
#[derive(Serialize, ToSchema)]
pub(crate) struct PartitionDoc {
    partition_id: String,
    name: String,
    /// `manual` | `agent` | `approval` | `mainline` | `main`.
    kind: String,
    /// Actor id for per-actor lines, absent otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    actor: Option<String>,
    /// Commit id (hex) of the ref head.
    current_snapshot: String,
    /// Commits reachable from the head, saturated at the walk cap.
    history_len: usize,
    /// Creation time of the oldest reachable commit (Unix milliseconds).
    created_at: i64,
    /// Time of the head commit (Unix milliseconds).
    updated_at: i64,
}

/// One commit in a partition history view
/// (`wf_checkpoint::provenance::PartitionCommitView` mirrored).
#[derive(Serialize, ToSchema)]
pub(crate) struct PartitionHistoryEntryDoc {
    /// Commit id (hex).
    snapshot_id: String,
    /// Parent commit ids (hex); more than one marks a merge commit.
    parents: Vec<String>,
    /// First message line (intent), absent when the message is blank.
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    /// Owning actor from the `Wf-Actor` trailer, absent when unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    actor: Option<String>,
    /// Edit session from the `Wf-Session` trailer, absent when unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    session: Option<String>,
    /// Origin tool from the `Wf-Tool` trailer, absent when unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    tool: Option<String>,
    /// Commit time (Unix milliseconds).
    timestamp: i64,
    /// True when the commit has more than one parent.
    is_merge: bool,
    /// True when the commit carries the conflict-unresolved state.
    has_conflicts: bool,
}

/// Reconstructed file of an actor partition
/// (`wf_checkpoint::provenance::WorkspaceFile` mirrored): `content` is the
/// raw bytes as a JSON number array (metadata consumers read path, hash and
/// timestamp only).
#[derive(Serialize, ToSchema)]
pub(crate) struct WorkspaceFileDoc {
    path: String,
    content: Vec<u8>,
    /// Content hash (SHA-256 hex).
    hash: String,
    timestamp: i64,
}

/// One hunk of a per-file difference (`DiffHunkView` mirrored).
#[derive(Serialize, ToSchema)]
pub(crate) struct DiffHunkDoc {
    /// The `@@` hunk header.
    header: String,
    /// Raw unified lines (`+`/`-`/context).
    lines: Vec<String>,
}

/// Per-file difference view (`wf_checkpoint::provenance::FileDiffView`
/// mirrored): the same structured shape the user-repo git diffs return,
/// so both surfaces share one schema.
#[derive(Serialize, ToSchema)]
pub(crate) struct FileDiffDoc {
    path: String,
    /// `added` | `modified` | `deleted` | `unchanged` (snake_case).
    kind: String,
    hunks: Vec<DiffHunkDoc>,
    additions: usize,
    deletions: usize,
    /// True for non-text content (no hunks rendered).
    binary: bool,
    /// True when hunk lines were cut at the render cap.
    truncated: bool,
}

/// One unresolved conflict region inside a merged file
/// (`wf_checkpoint::approval::ConflictView` mirrored).
#[derive(Serialize, ToSchema)]
pub(crate) struct ConflictRegionDoc {
    /// Relative file path the conflict applies to.
    file: String,
    /// Start line in the merged output (0-indexed).
    start_line: usize,
    base: Vec<String>,
    ours: Vec<String>,
    theirs: Vec<String>,
}

/// A file whose merge commit carries the unresolved-conflict flag
/// (`wf_checkpoint::provenance::ConflictFile` mirrored).
#[derive(Serialize, ToSchema)]
pub(crate) struct ConflictFileDoc {
    /// Relative file path.
    path: String,
    /// Commit id (hex) of the conflicted merge commit.
    snapshot_id: String,
    /// Ref the conflict lives on (`refs/wf/feat/*` or `refs/wf/main`).
    partition: String,
    /// Conflict regions re-derived from the standard markers on disk.
    conflicts: Vec<ConflictRegionDoc>,
}

/// One entry in a file's version timeline
/// (`wf_checkpoint::provenance::FileTimelineEntry` mirrored).
#[derive(Serialize, ToSchema)]
pub(crate) struct FileTimelineEntryDoc {
    /// File path at this point in time.
    path: String,
    /// Commit id (hex).
    snapshot_id: String,
    /// Content hash (SHA-256 hex) of the resulting file bytes.
    content_hash: String,
    /// Change time (Unix milliseconds).
    timestamp: i64,
    /// Origin label (e.g. "manual", "agent:loop-1").
    source: String,
    /// When this entry follows a rename, the previous path.
    #[serde(skip_serializing_if = "Option::is_none")]
    moved_from: Option<String>,
}

/// Capped version timeline for one file path
/// (`wf_api::checkpoint::provenance::FileTimelineView` mirrored): total is
/// counted before truncation.
#[derive(Serialize, ToSchema)]
pub(crate) struct FileTimelineDoc {
    /// Original path (the first path this file was known at).
    original_path: String,
    /// Entries are chronological, so the earliest page is stable.
    entries: Vec<FileTimelineEntryDoc>,
    truncated: bool,
    total: usize,
}

/// Read view of one workspace file with sandbox checks
/// (`wf_api::checkpoint::provenance::FileContentView` mirrored): reads are
/// capped at 1 MiB with `truncated: true`; binaries report metadata only
/// (`content` absent, `is_binary: true`).
#[derive(Serialize, ToSchema)]
pub(crate) struct FileContentDoc {
    path: String,
    actor: String,
    /// Content hash (SHA-256 hex).
    hash: String,
    /// Content size in bytes (pre-truncation).
    size: usize,
    is_binary: bool,
    /// UTF-8 content; absent for binaries.
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    truncated: bool,
    timestamp: i64,
}

/// One directory tree entry (metadata only, no content bytes)
/// (`wf_api::checkpoint::provenance::FileTreeEntry` mirrored).
#[derive(Serialize, ToSchema)]
pub(crate) struct FileTreeEntryDoc {
    path: String,
    /// Content hash (SHA-256 hex).
    hash: String,
    /// Content size in bytes.
    size: usize,
    timestamp: i64,
}

/// Capped directory tree view (the transport-local `FileTreeView` in
/// `file_provenance` mirrored): entries are sorted by path before
/// truncation so the first page is stable.
#[derive(Serialize, ToSchema)]
pub(crate) struct FileTreeDoc {
    entries: Vec<FileTreeEntryDoc>,
    truncated: bool,
    /// Total entry count before truncation.
    total: usize,
}

/// One staged edit group (`wf_checkpoint::file::session::EditGroup`
/// mirrored): the group id stringifies to `sess-<ts>-<pid>-<n>`.
#[derive(Serialize, ToSchema)]
pub(crate) struct EditGroupDoc {
    id: String,
    /// Optional human-readable label (e.g. "format file").
    label: Option<String>,
    /// Creation time (Unix milliseconds).
    created_at: i64,
}

/// One pending layered approval (`wf_checkpoint::approval::PendingApproval`
/// mirrored).
#[derive(Serialize, ToSchema)]
pub(crate) struct PendingApprovalDoc {
    /// Actor id string (e.g. `agent:{loop_id}`).
    actor: String,
    /// Approval snapshot id (hex).
    snapshot_id: String,
    /// Submission time (Unix milliseconds).
    submitted_at: i64,
    /// The submitted changes (chunked per file, chronological).
    changes: Vec<DeltaSummaryDoc>,
}

/// Outcome of an approve / merge operation
/// (`wf_checkpoint::approval::MergeOutcome` mirrored).
#[derive(Serialize, ToSchema)]
pub(crate) struct MergeOutcomeDoc {
    /// Whether the changes were merged into the feature.
    merged: bool,
    /// Resulting snapshot id (hex) of the target partition.
    snapshot_id: String,
    /// Conflicts detected by the three-way merge (empty when clean).
    conflicts: Vec<ConflictRegionDoc>,
    /// Distinct files with conflicts (sorted).
    conflict_files: Vec<String>,
    /// Human-readable outcome message.
    message: String,
}

/// One true conflict region parsed back from the diff3 markers the merge
/// driver embeds (`wf_checkpoint::file::git_merge::GitConflictRegion`
/// mirrored); line numbers are 0-indexed into the merged output,
/// `end_line` exclusive.
#[derive(Serialize, ToSchema)]
pub(crate) struct GitConflictRegionDoc {
    start_line: usize,
    end_line: usize,
    base_lines: Vec<String>,
    ours_lines: Vec<String>,
    theirs_lines: Vec<String>,
}

/// One conflicted file captured during a merge
/// (`wf_checkpoint::file::git_merge::GitConflictDetail` mirrored): binary
/// conflicts carry no regions and stay file-level only.
#[derive(Serialize, ToSchema)]
pub(crate) struct GitConflictDetailDoc {
    file: String,
    binary: bool,
    regions: Vec<GitConflictRegionDoc>,
}

/// Outcome of merging an actor submission into a feature ref
/// (`wf_checkpoint::file::git_merge::GitMergeOutcome` mirrored).
#[derive(Serialize, ToSchema)]
pub(crate) struct GitMergeOutcomeDoc {
    /// Merge commit id (hex).
    commit_id: String,
    parents: Vec<String>,
    /// Distinct files with conflicts (empty when merged cleanly).
    conflict_files: Vec<String>,
    details: Vec<GitConflictDetailDoc>,
}

/// Result of merging features into main, one merge commit per feature
/// (`wf_checkpoint::MergeCommitResult` mirrored): a conflicted feature
/// aborts the sequence while earlier merges stay individually revertable.
#[derive(Serialize, ToSchema)]
pub(crate) struct MergeCommitDoc {
    merge_result: GitMergeOutcomeDoc,
    /// Aggregate checkpoint id (hex) covering the merged features.
    checkpoint_id: String,
}

/// Statistics of one object-store cleanup run
/// (`wf_checkpoint::gc::GcStats` mirrored).
#[derive(Serialize, ToSchema)]
pub(crate) struct GcStatsDoc {
    /// Unreachable commits dropped by the sweep.
    removed_checkpoints: u64,
    /// Unreachable trees dropped with the removed commits.
    removed_snapshots: u64,
    /// Unreachable blobs dropped by the reclaim sweep.
    reclaimed_snapshots: u64,
}

/// Read-only object-store statistics for the storage dashboard
/// (`wf_checkpoint::gc::StoreStats` mirrored).
#[derive(Serialize, ToSchema)]
pub(crate) struct StoreStatsDoc {
    /// Commits reachable from any ref.
    commits: u64,
    /// Refs under the `refs/wf/` namespace.
    refs: u64,
    /// Loose objects currently in the object store.
    loose_objects: u64,
    /// Total bytes occupied by the bare repository directory.
    store_bytes: u64,
    /// Time of the last completed GC run (Unix milliseconds), absent when
    /// no GC has run yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    last_gc_ms: Option<i64>,
}

/// Health of the state-to-file association for one state checkpoint
/// (`wf_checkpoint::file::StateFileLinkStatus` flattened).
#[derive(Serialize, ToSchema)]
pub(crate) struct StateFileLinkDoc {
    /// State checkpoint id that was queried.
    checkpoint_id: String,
    /// `linked` when the durable link exists, `pending` when the link write
    /// failed and awaits startup compensation, `missing` when no link was
    /// ever recorded.
    state: String,
    /// Linked file commit id; present for `linked` and `pending`.
    #[serde(skip_serializing_if = "Option::is_none")]
    file_commit_id: Option<String>,
}
