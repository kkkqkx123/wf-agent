use serde::{Deserialize, Serialize};
use serde_json::Value;

// ── ApiError ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
    pub suggestion: Option<String>,
    pub details: Option<Value>,
}

impl ApiError {
    pub fn not_found(entity: impl Into<String>) -> Self {
        ApiError {
            code: "NOT_FOUND".into(),
            message: format!("{} not found", entity.into()),
            suggestion: Some("check that the name or ID is correct".into()),
            details: None,
        }
    }

    pub fn invalid_params(msg: impl Into<String>) -> Self {
        ApiError {
            code: "INVALID_PARAMS".into(),
            message: msg.into(),
            suggestion: Some("check the provided parameters".into()),
            details: None,
        }
    }

    pub fn storage(msg: impl Into<String>) -> Self {
        ApiError {
            code: "STORAGE_ERROR".into(),
            message: msg.into(),
            suggestion: Some("check database integrity and permissions".into()),
            details: None,
        }
    }

    pub fn engine(msg: impl Into<String>) -> Self {
        ApiError {
            code: "ENGINE_ERROR".into(),
            message: msg.into(),
            details: None,
            suggestion: None,
        }
    }

    pub fn state_machine(msg: impl Into<String>) -> Self {
        ApiError {
            code: "STATE_MACHINE_ERROR".into(),
            message: msg.into(),
            details: None,
            suggestion: None,
        }
    }

    pub fn checkpoint(msg: impl Into<String>) -> Self {
        ApiError {
            code: "CHECKPOINT_ERROR".into(),
            message: msg.into(),
            details: None,
            suggestion: None,
        }
    }

    pub fn git_sync(msg: impl Into<String>) -> Self {
        ApiError {
            code: "GIT_SYNC_ERROR".into(),
            message: msg.into(),
            details: None,
            suggestion: None,
        }
    }

    pub fn gc(msg: impl Into<String>) -> Self {
        ApiError {
            code: "GC_ERROR".into(),
            message: msg.into(),
            details: None,
            suggestion: None,
        }
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        ApiError {
            code: "INTERNAL_ERROR".into(),
            message: msg.into(),
            suggestion: None,
            details: None,
        }
    }

    pub fn general(msg: impl Into<String>) -> Self {
        ApiError {
            code: "ERROR".into(),
            message: msg.into(),
            suggestion: None,
            details: None,
        }
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for ApiError {}

pub type ApiResult<T> = std::result::Result<T, ApiError>;

// ── Request types ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitRequest {
    pub db_path: Option<String>,
    pub git_repo: Option<String>,
    pub git_ref: Option<String>,
}

impl Default for InitRequest {
    fn default() -> Self {
        InitRequest {
            db_path: Some(".layertwine/layertwine.db".into()),
            git_repo: None,
            git_ref: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditRequest {
    pub file: String,
    pub content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEditRequest {
    pub agent_id: String,
    pub file: String,
    pub content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSubmitRequest {
    pub agent_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApproveRequest {
    pub agent_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitRequest {
    pub message: String,
    pub author: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogRequest {
    pub count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchCreateRequest {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchSwitchRequest {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeRequest {
    pub branch: String,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupRequest {
    pub snapshot_id: String,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreRequest {
    pub backup_id: String,
}

// ── Checkpoint restore types ──

/// Full or selective checkpoint restore
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointRestoreRequest {
    pub checkpoint_id: String,
    /// Optional source filter (e.g. ["agent://", "file://src/**"])
    pub source_filter: Option<Vec<String>>,
}

/// Time-based checkpoint restore
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointRestoreByTimeRequest {
    pub target_time: i64,
    /// Optional source filter
    pub source_filter: Option<Vec<String>>,
}

/// Diff between two checkpoints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointDiffRequest {
    pub from_id: String,
    pub to_id: String,
}

/// Rollback staged partition to a checkpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointRollbackRequest {
    pub checkpoint_id: String,
}

/// Restored snapshot info within a checkpoint restore response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoredSnapshotInfo {
    pub snapshot_id: String,
    pub source: String,
    pub content_hex: String,
    pub content_type: String,
}

/// Checkpoint restore response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointRestoreResponse {
    pub checkpoint: CheckpointInfo,
    pub snapshots: Vec<RestoredSnapshotInfo>,
    pub ancestry: Vec<String>,
}

/// Checkpoint diff response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointDiffResponse {
    pub from_id: String,
    pub to_id: String,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub modified: Vec<String>,
    pub total_changes: usize,
}

/// Checkpoint rollback response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointRollbackResponse {
    pub checkpoint_id: String,
    pub snapshot_ids: Vec<String>,
}

/// Restore checkpoint and apply to working directory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointRestoreApplyRequest {
    pub checkpoint_id: String,
    /// Optional source filter (e.g. ["file://src/**"])
    /// Defaults to all file:// sources if not set.
    pub source_filter: Option<Vec<String>>,
    /// Whether to update the staged partition pointer after restore
    pub update_staged: bool,
    /// Whether to skip writing files (just update staged partition)
    pub skip_write: bool,
}

/// Result of applying a restore to the working directory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointRestoreApplyResponse {
    pub checkpoint_id: String,
    pub files_written: Vec<String>,
    pub staged_updated: bool,
}

// ── GitCommit (sync to git)/Pull types ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitCommitRequest {
    pub git_repo: String,
    pub message: Option<String>,
}

// Backward compatibility alias
pub type PushRequest = GitCommitRequest;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequest {
    pub remote: Option<String>,
    pub git_repo: String,
    pub git_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GcRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShowRequest {
    pub show_what: String,
    pub target_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShowResponse {
    pub target: String,
    pub diffs: Vec<FileDiff>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDiff {
    pub file_path: String,
    pub unified_diff: String,
    pub inserts: usize,
    pub deletes: usize,
}

// ── Response types ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitResponse {
    pub db_path: String,
    pub manual_partition_id: String,
    pub staged_partition_id: String,
    pub branch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResponse {
    pub partitions: Vec<PartitionInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionInfo {
    pub layer: String,
    pub name: String,
    pub current_snapshot: String,
    pub history_len: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditResponse {
    pub snapshot_id: String,
    pub staged_snapshot_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitResponse {
    pub snapshot_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApproveResponse {
    pub integrated_snapshot_id: String,
    pub staged_snapshot_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitResponse {
    pub checkpoint_id: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogResponse {
    pub checkpoints: Vec<CheckpointInfo>,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointInfo {
    pub id: String,
    pub author: String,
    pub message: String,
    pub parents: Vec<String>,
    pub snapshots: Vec<String>,
    pub created_at: i64,
    pub git_anchor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchCreateResponse {
    pub name: String,
    pub head: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchSwitchResponse {
    pub name: String,
    pub checkpoint_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchListResponse {
    pub branches: Vec<BranchInfo>,
    pub current: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchInfo {
    pub name: String,
    pub head: String,
    pub updated_at: String,
    pub is_current: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeResponse {
    pub checkpoint_id: String,
    pub source_branch: String,
    pub target_branch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupResponse {
    pub backup_id: String,
    pub source_snapshot_id: String,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreResponse {
    pub backup_id: String,
    pub file: String,
    pub deltas_restored: usize,
    /// Snapshot ID of the merged result after 3-way merge into staged
    pub merged_snapshot_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GcResponse {
    pub removed_checkpoints: usize,
    pub removed_snapshots: usize,
    pub freed_bytes: u64,
    pub delta_chain_depth_triggered: bool,
}

// ── Compact (file maintenance) ──

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompactRequest {
    /// If true, use full VACUUM instead of incremental (requires exclusive lock).
    pub vacuum_full: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactResponse {
    /// Whether WAL checkpoint was performed.
    pub wal_checkpointed: bool,
    /// Free pages before compaction.
    pub freelist_before: i64,
    /// Total pages in database.
    pub total_pages: i64,
    /// Free pages after compaction.
    pub freelist_after: i64,
    /// Whether vacuum was actually executed.
    pub vacuum_performed: bool,
    /// Summary message.
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitCommitResponse {
    pub git_commit_hash: String,
}

// Backward compatibility alias
pub type PushResponse = GitCommitResponse;

// ── Clean types ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanRequest {
    /// Branch name to clean (all checkpoints + orphaned data)
    pub branch: Option<String>,
    /// Layer type to clean (e.g. "staged", "unified", "integrated")
    pub layer: Option<String>,
    /// Clean ALL layertwine storage (reset to initial state)
    pub all: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanResponse {
    pub removed_branches: usize,
    pub removed_checkpoints: usize,
    pub removed_snapshots: usize,
    pub removed_deltas: usize,
    pub removed_layers: usize,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullResponse {
    pub remote: String,
    pub git_ref: String,
}

// ── Approval-related types ──

/// Information about a pending approval
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalInfo {
    pub agent_id: String,
    pub partition_name: String,
    pub current_snapshot: String,
    pub history_len: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListPendingApprovalsResponse {
    pub approvals: Vec<ApprovalInfo>,
    pub total: usize,
}

/// Granular approve request (approve one agent, merge to integrated)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApproveAgentRequest {
    pub agent_id: String,
    /// Name for the integrated partition. Defaults to the agent_id if not provided.
    pub integrated_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApproveAgentResponse {
    pub agent_id: String,
    pub integrated_snapshot_id: String,
}

/// Reject a specific agent's submission
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RejectAgentRequest {
    pub agent_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RejectAgentResponse {
    pub agent_id: String,
    pub baseline_snapshot_id: String,
}

/// Merge integrated → unified
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeToUnifiedRequest {
    /// List of integration names to merge. If empty, all integrated partitions are used.
    pub integration_names: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeToUnifiedResponse {
    pub unified_snapshot_id: String,
    pub merged_count: usize,
}

/// Merge unified → staged
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeToStagedRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeToStagedResponse {
    pub staged_snapshot_id: String,
}
