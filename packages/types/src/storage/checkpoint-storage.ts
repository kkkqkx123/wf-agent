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
 * Checkpoint storage record — core fields stored as independent columns
 *
 * These fields are required for all checkpoint types (FULL and DELTA).
 * They support entity-based querying, chain traversal, and cleanup policies.
 *
 * Storage layer (SQLite/Postgres) stores these as separate indexed columns:
 *   entity_type, entity_id, timestamp, checkpoint_type,
 *   base_checkpoint_id, previous_checkpoint_id, blob_size
 */
export interface CheckpointStorageRecord {
  /** Entity type - identifies which entity this checkpoint belongs to */
  entityType: CheckpointEntityType;
  /** Entity ID - the specific entity identifier (workflowId, agentLoopId, taskId, etc.) */
  entityId: ID;
  /** Creation timestamp */
  timestamp: Timestamp;
  /** Checkpoint type (FULL or DELTA) */
  checkpointType?: 'FULL' | 'DELTA';
  /** Base checkpoint ID (for delta checkpoints) */
  baseCheckpointId?: ID;
  /** Previous checkpoint ID (for delta checkpoints) */
  previousCheckpointId?: ID;
  /** BLOB size in bytes (for size-based cleanup policies) */
  blobSize?: number;
}

/**
 * Checkpoint storage metadata — extends record with optional user-facing metadata
 *
 * These fields are stored as TEXT columns in the storage layer.
 * They are only populated for FULL checkpoints to avoid duplication
 * across the delta chain. DELTA checkpoints store the core record only.
 *
 * Storage layer stores these as separate columns:
 *   tags (TEXT), custom_fields (TEXT)
 */
export interface CheckpointStorageMetadata extends CheckpointStorageRecord {
  /** Tag array (for categorization and retrieval) */
  tags?: string[];
  /** Custom metadata fields */
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
 * Tier definition for tiered retention policy
 */
export interface RetentionTier {
  /** Minimum age in days for this tier to apply */
  minAgeDays: number;
  /** Maximum age in days for this tier (optional, unbounded if omitted) */
  maxAgeDays?: number;
  /** Retention interval: keep at least one checkpoint per N days */
  retentionIntervalDays: number;
}

/**
 * Types of Clearance Strategies
 */
export type CleanupStrategyType = "time" | "count" | "size" | "tiered";

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
 * Tiered retention policy configuration
 * Keeps checkpoints based on age-based tiers with different retention granularity
 */
export interface TieredCleanupPolicy {
  /** Type of strategy */
  type: "tiered";
  /** Retention tiers sorted by age ascending */
  tiers: RetentionTier[];
  /** Minimum number of reservations (to prevent deletion of all checkpoints) */
  minRetention?: number;
}

/**
 * Cleanup policy configuration (federation type)
 */
export type CleanupPolicy =
  | TimeBasedCleanupPolicy
  | CountBasedCleanupPolicy
  | SizeBasedCleanupPolicy
  | TieredCleanupPolicy;

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
