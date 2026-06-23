/**
 * Layertwine gRPC Executor Type Definitions
 *
 * Aligned with Layertwine Rust side gRPC proto definitions
 */

// ── Init ──
export interface LayertwineInitRequest {
  dbPath?: string;
  gitRepo?: string;
  gitRef?: string;
}

export interface LayertwineInitResponse {
  dbPath: string;
  manualPartitionId: string;
  stagedPartitionId: string;
  branch: string;
}

// ── Edit ──
export interface LayertwineEditRequest {
  file: string;
  content?: string;
}

export interface LayertwineEditResponse {
  snapshotId: string;
  stagedSnapshotId?: string;
}

// ── Status ──
export interface LayertwineStatusResponse {
  partitions: LayertwinePartitionInfo[];
}

export interface LayertwinePartitionInfo {
  layer: string;
  name: string;
  currentSnapshot: string;
  historyLen: number;
}

// ── Commit ──
export interface LayertwineCommitRequest {
  message: string;
  author?: string;
  parentId?: string;
}

export interface LayertwineCommitResponse {
  checkpointId: string;
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
  createdAt: number;
  gitAnchor?: string;
  parentId?: string;
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
  checkpointId: string;
}

export interface LayertwineBranchListResponse {
  branches: LayertwineBranchInfo[];
  current?: string;
}

export interface LayertwineBranchInfo {
  name: string;
  head: string;
  updatedAt: string;
  isCurrent: boolean;
}

// ── Agent ──
export interface LayertwineAgentEditRequest {
  agentId: string;
  file: string;
  content?: string;
}

export interface LayertwineAgentEditResponse {
  snapshotId: string;
}

export interface LayertwineAgentSubmitRequest {
  agentId: string;
}

export interface LayertwineAgentSubmitResponse {
  checkpointId: string;
}

export interface LayertwineApproveRequest {
  agentId: string;
}

export interface LayertwineApproveResponse {
  approved: boolean;
}

export interface LayertwineBackupRequest {
  targetPath: string;
}

export interface LayertwineBackupResponse {
  backupPath: string;
  size: number;
}

// ── Checkpoint Restore ──
export interface LayertwineRestoreRequest {
  checkpointId: string;
}

export interface LayertwineSnapshotInfo {
  id: string;
  source: string;
  contentType: string;
  size: number;
  createdAt: number;
}

export interface LayertwineRestoreResponse {
  checkpointId: string;
  snapshots: LayertwineSnapshotInfo[];
  ancestry: string[];
  metadata: {
    author: string;
    message: string;
    createdAt: number;
    parentId?: string;
  };
}

// ── Selective Restore ──
export interface LayertwineSelectiveRestoreRequest {
  checkpointId: string;
  sources?: string[];
}

export interface LayertwineSelectiveRestoreResponse {
  checkpointId: string;
  snapshots: LayertwineSnapshotInfo[];
  metadata: {
    author: string;
    message: string;
    createdAt: number;
    parentId?: string;
  };
}

// ── Time-based Restore ──
export interface LayertwineRestoreByTimeRequest {
  timestamp: number;
  source?: string;
}

export interface LayertwineRestoreByTimeResponse {
  checkpointId: string;
  snapshots: LayertwineSnapshotInfo[];
  timestamp: number;
}

// ── Checkpoint Diff ──
export interface LayertwineDiffRequest {
  fromCheckpointId: string;
  toCheckpointId: string;
}

export interface LayertwineDiffResponse {
  added: string[];
  removed: string[];
  modified: string[];
}

// ── Snapshot Content Retrieve ──
export interface LayertwineGetSnapshotRequest {
  checkpointId: string;
  snapshotId: string;
}

export interface LayertwineGetSnapshotResponse {
  snapshotId: string;
  source: string;
  contentType: string;
  content: string | Buffer;
  size: number;
}
