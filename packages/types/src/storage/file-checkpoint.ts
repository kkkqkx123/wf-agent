/**
 * File Checkpoint Type Definitions
 * Data types for workspace file state checkpoint persistence
 */

import type { ID, Timestamp } from '../common.js';

/**
 * File change type
 */
export type FileChangeType = 'added' | 'modified' | 'deleted';

/**
 * Single file change record within a checkpoint
 */
export interface FileChangeRecord {
  /** File path relative to workspace root */
  path: string;
  /** Type of change */
  type: FileChangeType;
  /** File content hash */
  hash: string;
}

/**
 * File checkpoint storage metadata
 * Metadata for indexing and querying file checkpoints
 */
export interface FileCheckpointMetadata {
  /** Entity ID (workflowExecutionId) */
  entityId: ID;
  /** Timestamp of checkpoint creation */
  timestamp: Timestamp;
  /** Checkpoint type: full or incremental */
  type: 'full' | 'incremental';
  /** Base checkpoint ID (for incremental checkpoints) */
  baseCheckpointId?: ID;
  /** List of file changes (for incremental checkpoints) */
  changes?: FileChangeRecord[];
  /** Number of files in the checkpoint */
  fileCount: number;
  /** Complete hash snapshot of all tracked files (path -> hash) */
  fileHashSnapshot: Record<string, string>;
  /** List of empty directories at checkpoint time */
  emptyDirs: string[];
  /** Total size of backed-up file content (bytes) */
  totalSize: number;
  /** Workspace root path at checkpoint time */
  workspaceRoot: string;
  /** Tags for categorization */
  tags?: string[];
  /** Custom metadata fields */
  customFields?: Record<string, unknown>;
}

/**
 * File checkpoint list query options
 */
export interface FileCheckpointListOptions {
  /** Filter by entity ID */
  entityId?: ID;
  /** Filter by checkpoint type */
  type?: 'full' | 'incremental';
  /** Timestamp range - start */
  timestampFrom?: Timestamp;
  /** Timestamp range - end */
  timestampTo?: Timestamp;
  /** Search tags (match any) */
  tags?: string[];
  /** Maximum returns */
  limit?: number;
  /** Pagination offset */
  offset?: number;
  /** Sort field */
  sortBy?: 'timestamp' | 'fileCount' | 'totalSize';
  /** Sort direction */
  sortOrder?: 'asc' | 'desc';
}

/**
 * File checkpoint info (with ID and metadata)
 */
export interface FileCheckpointInfo {
  /** Checkpoint ID */
  checkpointId: ID;
  /** Metadata */
  metadata: FileCheckpointMetadata;
}

/**
 * File checkpoint configuration alias (re-exported from config)
 * @deprecated Import FileCheckpointConfig from '@wf-agent/types' directly
 */