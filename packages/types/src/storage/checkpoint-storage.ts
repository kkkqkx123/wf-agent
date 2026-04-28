/**
 * Checkpoint store type definition
 * Defining checkpoint store-related metadata, query options, and cleanup policies
 */

import type { ID, Timestamp } from "../common.js";

/**
 * Checkpoint storage metadata
 * Metadata information for indexing and querying
 */
export interface CheckpointStorageMetadata {
  /** Thread ID */
  threadId: ID;
  /** Workflow ID */
  workflowId: ID;
  /** Creating timestamps */
  timestamp: Timestamp;
  /** Tagged arrays (for categorization and retrieval) */
  tags?: string[];
  /** Customizing metadata fields */
  customFields?: Record<string, unknown>;
}

/**
 * Checkpoint stored list query option
 * Support for basic filtering and paging
 */
export interface CheckpointStorageListOptions {
  /** Filter by thread ID */
  threadId?: ID;
  /** Filter by Workflow ID */
  workflowId?: ID;
  /** Filter by tag (match any tag) */
  tags?: string[];
  /** Maximum number of returns (paged) */
  limit?: number;
  /** Offset (paging) */
  offset?: number;
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
