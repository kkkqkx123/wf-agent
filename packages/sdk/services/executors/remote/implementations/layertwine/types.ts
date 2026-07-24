/**
 * Layertwine gRPC Executor Type Definitions
 *
 * Aligned with Layertwine Rust side gRPC proto definitions
 * Field naming uses snake_case to match proto (GrpcClient uses keepCase: true)
 */

// ── Init ──
export interface LayertwineInitRequest {
  db_path?: string;
  git_repo?: string;
  git_ref?: string;
}

export interface LayertwineInitResponse {
  db_path: string;
  manual_partition_id: string;
  staged_partition_id: string;
  branch: string;
}

// ── Edit ──
export interface LayertwineEditRequest {
  file: string;
  content?: string;
}

export interface LayertwineEditResponse {
  snapshot_id: string;
  staged_snapshot_id?: string;
}

// ── Status ──
export interface LayertwineStatusResponse {
  partitions: LayertwinePartitionInfo[];
}

export interface LayertwinePartitionInfo {
  layer: string;
  name: string;
  current_snapshot: string;
  history_len: number;
}

// ── Commit ──
export interface LayertwineCommitRequest {
  message: string;
  author?: string;
}

export interface LayertwineCommitResponse {
  checkpoint_id: string;
  message: string;
}

// ── Log ──
export interface LayertwineLogRequest {
  count?: number;
}

export interface LayertwineLogResponse {
  checkpoints: LayertwineCheckpointInfo[];
  total: number;
}

export interface LayertwineCheckpointInfo {
  id: string;
  author: string;
  message: string;
  parents: string[];
  snapshots: string[];
  created_at: number;
  git_anchor?: string;
}

// ── Branch ──
export interface LayertwineBranchCreateRequest {
  name: string;
}

export interface LayertwineBranchCreateResponse {
  name: string;
  head: string;
}

export interface LayertwineBranchSwitchRequest {
  name: string;
}

export interface LayertwineBranchSwitchResponse {
  name: string;
  checkpoint_id: string;
}

export interface LayertwineBranchListResponse {
  branches: LayertwineBranchInfo[];
  current?: string;
}

export interface LayertwineBranchInfo {
  name: string;
  head: string;
  updated_at: string;
  is_current: boolean;
}

// ── Agent ──
export interface LayertwineAgentEditRequest {
  agent_id: string;
  file: string;
  content?: string;
}

export interface LayertwineAgentEditResponse {
  snapshot_id: string;
}

export interface LayertwineAgentSubmitRequest {
  agent_id: string;
}

export interface LayertwineAgentSubmitResponse {
  snapshot_id: string;
}

export interface LayertwineApproveRequest {
  agent_id: string;
}

export interface LayertwineApproveResponse {
  integrated_snapshot_id: string;
  staged_snapshot_id: string;
}

export interface LayertwineBackupRequest {
  snapshot_id: string;
  label?: string;
}

export interface LayertwineBackupResponse {
  backup_path: string;
  size: number;
}

// ── Checkpoint Restore ──
// Maps to proto CheckpointRestoreRequest / CheckpointRestoreResponse
export interface LayertwineCheckpointRestoreRequest {
  checkpoint_id: string;
  source_filter?: string[];
}

export interface LayertwineRestoredSnapshotInfo {
  snapshot_id: string;
  source: string;
  content_hex: string;
  content_type: string;
}

export interface LayertwineCheckpointRestoreResponse {
  checkpoint: LayertwineCheckpointInfo;
  snapshots: LayertwineRestoredSnapshotInfo[];
  ancestry: string[];
}

// ── Time-based Restore ──
// Maps to proto CheckpointRestoreByTimeRequest / CheckpointRestoreByTimeResponse
export interface LayertwineRestoreByTimeRequest {
  target_time: number;
  source_filter?: string[];
}

export interface LayertwineRestoreByTimeResponse {
  checkpoint: LayertwineCheckpointInfo;
  snapshots: LayertwineRestoredSnapshotInfo[];
  ancestry: string[];
}

// ── Checkpoint Diff ──
// Maps to proto CheckpointDiffRequest / CheckpointDiffResponse
export interface LayertwineDiffRequest {
  from_id: string;
  to_id: string;
}

export interface LayertwineDiffResponse {
  from_id: string;
  to_id: string;
  added: string[];
  removed: string[];
  modified: string[];
  total_changes: number;
}
