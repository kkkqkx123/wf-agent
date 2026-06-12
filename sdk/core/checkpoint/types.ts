/**
 * Common interfaces and types for universal checkpoint system
 */

import type { BaseCheckpoint, DeltaStorageConfig } from "@wf-agent/types";

/**
 * Storage adapter interface for checkpoint persistence
 *
 * Note: This adapter works with serialized data (Uint8Array).
 * Serialization/deserialization is handled externally by StateCodec.
 * The adapter is agnostic to the specific checkpoint type.
 * Entity-based design for efficient checkpoint management.
 */
export interface CheckpointStorageAdapter<TMetadata = unknown> {
  save(id: string, data: Uint8Array, metadata: TMetadata): Promise<void>;
  load(id: string): Promise<Uint8Array | null>;
  delete(id: string): Promise<void>;
  list(options?: Record<string, unknown>): Promise<string[]>;
  /**
   * List checkpoints with metadata only (without loading BLOB data)
   * More efficient for cleanup operations
   */
  listWithMetadata(options?: Record<string, unknown>): Promise<
    Array<{
      id: string;
      metadata: TMetadata;
    }>
  >;
  /**
   * List checkpoints for a specific entity with metadata
   * Optimized for entity-level queries using database indexes
   */
  listByEntityWithMetadata(
    entityId: string,
    entityType: string,
    options?: { limit?: number; offset?: number },
  ): Promise<
    Array<{
      id: string;
      metadata: TMetadata;
    }>
  >;
  /**
   * Get the latest N checkpoints for a specific entity
   * Optimized for quick recovery scenarios
   */
  getLatestByEntity(
    entityId: string,
    entityType: string,
    count?: number,
    includeData?: boolean,
  ): Promise<
    Array<{
      id: string;
      metadata: TMetadata;
      data?: Uint8Array;
    }>
  >;
  /**
   * Delete checkpoints for a specific entity with advanced options
   * Supports batch deletion with retention policies
   */
  deleteByEntity(
    entityId: string,
    entityType: string,
    options?: {
      keepLatest?: number;
      olderThan?: number;
    },
  ): Promise<number>;
  initialize?(): Promise<void>;
  close?(): Promise<void>;
}

/**
 * Minimal entity interface for checkpointing
 * Only requires an ID - state extraction is handled by coordinator
 */
export interface CheckpointableEntity {
  id: string;
}

/**
 * Checkpoint dependencies interface
 * Generic enough to work with any checkpoint type
 */
export interface CheckpointDependencies<TCheckpoint extends BaseCheckpoint<unknown, unknown>> {
  saveCheckpoint: (checkpoint: TCheckpoint) => Promise<string>;
  getCheckpoint: (id: string) => Promise<TCheckpoint | null>;
  listCheckpoints: (parentId: string) => Promise<string[]>;
  deltaConfig?: DeltaStorageConfig;
}

/**
 * Delta restoration result
 */
export interface DeltaRestoreResult<TState> {
  snapshot: TState;
  metadata: {
    checkpointChain: string[];
    baseCheckpointId: string;
  };
}
