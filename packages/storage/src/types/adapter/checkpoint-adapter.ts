/**
 * Checkpoint Storage Adapter Interface Definition
 * Defines a uniform interface for checkpoint persistence operations
 */

import type { CheckpointStorageMetadata, CheckpointStorageListOptions } from "@wf-agent/types";
import type { BaseStorageAdapter } from "./base-storage-adapter.js";
import type { CheckpointOptions } from "../checkpoint-options.js";

/**
 * Checkpoint Storage Adapter Interface
 *
 * Unified interface for defining checkpoint persistence operations
 * - Entity-based design for efficient checkpoint management
 * - Inherits from BaseStorageAdapter and provides standard CRUD operations.
 * - packages/storage provides an implementation of CheckpointStorageAdapter based on this interface.
 * - Applications can use CheckpointStorageAdapter directly or implement it themselves.
 */
export interface CheckpointStorageAdapter extends BaseStorageAdapter<
  CheckpointStorageMetadata,
  CheckpointStorageListOptions,
  CheckpointOptions
> {
  /**
   * List checkpoints with metadata only (without loading BLOB data)
   * More efficient for cleanup operations and metadata queries
   * 
   * @param options Query options for filtering (entityType and entityId recommended)
   * @returns Array of checkpoint info with IDs and metadata
   */
  listWithMetadata(options?: CheckpointStorageListOptions): Promise<Array<{
    id: string;
    metadata: CheckpointStorageMetadata;
  }>>;

  /**
   * List checkpoints for a specific entity with metadata
   * Optimized for entity-level queries using database indexes
   * 
   * @param entityId The entity ID (workflowId, agentLoopId, etc.)
   * @param entityType Entity type filter ('workflow', 'agent', 'task')
   * @param options Additional query options (limit, offset)
   * @returns Array of checkpoint info with IDs and metadata
   */
  listByEntityWithMetadata(
    entityId: string,
    entityType: string,
    options?: { limit?: number; offset?: number }
  ): Promise<Array<{
    id: string;
    metadata: CheckpointStorageMetadata;
  }>>;

  /**
   * Get the latest N checkpoints for a specific entity
   * Optimized for quick recovery scenarios
   * 
   * @param entityId The entity ID
   * @param entityType Entity type filter
   * @param count Number of latest checkpoints to retrieve (default: 1)
   * @param includeData Whether to include checkpoint data (default: false)
   * @returns Array of checkpoint info, optionally with data
   */
  getLatestByEntity(
    entityId: string,
    entityType: string,
    count?: number,
    includeData?: boolean
  ): Promise<Array<{
    id: string;
    metadata: CheckpointStorageMetadata;
    data?: Uint8Array;
  }>>;

  /**
   * Delete checkpoints for a specific entity with advanced options
   * Supports batch deletion with retention policies
   * 
   * @param entityId The entity ID
   * @param entityType Entity type filter
   * @param options Deletion options (keepLatest, olderThan)
   * @returns Number of deleted checkpoints
   */
  deleteByEntity(
    entityId: string,
    entityType: string,
    options?: {
      keepLatest?: number;  // Keep the latest N checkpoints
      olderThan?: number;   // Only delete checkpoints older than this timestamp
    }
  ): Promise<number>;

  /**
   * Get entity-level metadata (e.g., cleanup watermark)
   * 
   * @param entityType Entity type
   * @param entityId Entity ID
   * @returns Entity metadata or null if not found
   */
  getEntityMetadata(entityType: string, entityId: string): Promise<Record<string, unknown> | null>;

  /**
   * Update entity-level metadata (e.g., cleanup watermark)
   * 
   * @param entityType Entity type
   * @param entityId Entity ID
   * @param metadata Metadata key-value pairs to store
   */
  setEntityMetadata(entityType: string, entityId: string, metadata: Record<string, unknown>): Promise<void>;
}
