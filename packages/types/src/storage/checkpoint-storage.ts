/**
 * Checkpoint store type definition
 * Defining checkpoint store-related metadata, query options, and cleanup policies
 */

import type { ID, Timestamp } from "../common.js";

/**
 * Entity type for checkpoint storage
 * Identifies which entity type the checkpoint belongs to
 */
export type CheckpointEntityType = 'workflow' | 'agent' | 'task';

/**
 * Checkpoint storage metadata
 * Metadata information for indexing and querying
 * Entity-based design for efficient checkpoint management
 */
export interface CheckpointStorageMetadata {
  /** Entity type - identifies which entity this checkpoint belongs to (required) */
  entityType: CheckpointEntityType;
  /** Entity ID - the specific entity identifier (workflowId, agentLoopId, taskId, etc.) (required) */
  entityId: ID;
  /** Creating timestamp (required) */
  timestamp: Timestamp;
  /** Tagged arrays (for categorization and retrieval) */
  tags?: string[];
  /** Customizing metadata fields */
  customFields?: Record<string, unknown>;
}

/**
 * Checkpoint stored list query option
 * Support for basic filtering and paging
 * Entity-aware filtering as primary query mechanism
 */
export interface CheckpointStorageListOptions {
  /** Filter by entity type (required for efficient queries) */
  entityType?: CheckpointEntityType;
  /** Filter by entity ID (required for efficient queries) */
  entityId?: ID;
  /** Filter by tag (match any tag) */
  tags?: string[];
  /** Filter by timestamp range - start */
  timestampFrom?: Timestamp;
  /** Filter by timestamp range - end */
  timestampTo?: Timestamp;
  /** Filter by checkpoint type */
  type?: 'FULL' | 'DELTA';
  /** Maximum number of returns (paged) */
  limit?: number;
  /** Offset (paging) */
  offset?: number;
  /** Sort Fields */
  sortBy?: 'timestamp' | 'size' | 'id';
  /** sorting direction */
  sortOrder?: 'asc' | 'desc';
}

/**
 * Checkpoint information (with IDs and metadata)
 * Used for cleanup strategy decisions
 */
export interface CheckpointInfo {
  /** Checkpoint ID */
  checkpointId: string;
  /** Checkpoint metadata */
  metadata: CheckpointStorageMetadata;
}

/**
 * Types of Clearance Strategies
 */
export type CleanupStrategyType = "time" | "count" | "size";

/**
 * Time-based cleanup policy configuration
 */
export interface TimeBasedCleanupPolicy {
  /** Type of strategy */
  type: "time";
  /** Number of days of retention */
  retentionDays: number;
  /** Minimum number of reservations (to prevent deletion of all checkpoints) */
  minRetention?: number;
}

/**
 * Volume-based cleanup policy configuration
 */
export interface CountBasedCleanupPolicy {
  /** Type of strategy */
  type: "count";
  /** Maximum number of reservations */
  maxCount: number;
  /** Minimum number of reservations (to prevent deletion of all checkpoints) */
  minRetention?: number;
}

/**
 * Storage space-based cleanup policy configuration
 */
export interface SizeBasedCleanupPolicy {
  /** Type of strategy */
  type: "size";
  /** Maximum storage space (bytes) */
  maxSizeBytes: number;
  /** Minimum number of reservations (to prevent deletion of all checkpoints) */
  minRetention?: number;
}

/**
 * Cleanup policy configuration (federation type)
 */
export type CleanupPolicy =
  | TimeBasedCleanupPolicy
  | CountBasedCleanupPolicy
  | SizeBasedCleanupPolicy;

/**
 * Liquidation results
 */
export interface CleanupResult {
  /** List of deleted checkpoint IDs */
  deletedCheckpointIds: string[];
  /** Number of checkpoints deleted */
  deletedCount: number;
  /** Storage space released (bytes) */
  freedSpaceBytes: number;
  /** Number of remaining checkpoints */
  remainingCount: number;
}

/**
 * Checkpoint Cleanup Policy Interface
 *
 * Provides a unified interface for cleanup policy enforcement
 */
export interface CheckpointCleanupStrategy {
  /**
   * Implementing a cleanup strategy
   *
   * @param checkpoints List of information about all checkpoints (with IDs and metadata)
   * @returns List of checkpoint IDs to be deleted
   */
  execute(checkpoints: CheckpointInfo[]): string[];
}
