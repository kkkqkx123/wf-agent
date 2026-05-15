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
 * - Inherits from BaseStorageAdapter and provides standard CRUD operations.
 * - packages/storage provides an implementation of CheckpointStorageAdapter based on this interface.
 * - Applications can use CheckpointStorageAdapter directly or implement it themselves.
 * - Supports multiple entity types (workflow, agent, task) through metadata fields.
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
   * @param options Query options for filtering
   * @returns Array of checkpoint info with IDs and metadata
   */
  listWithMetadata(options?: CheckpointStorageListOptions): Promise<Array<{
    id: string;
    metadata: CheckpointStorageMetadata;
  }>>;

  /**
   * List checkpoints for a specific entity
   * Entity-aware filtering for multi-entity checkpoint storage
   * 
   * @param entityId The entity ID (workflowId, agentLoopId, etc.)
   * @param entityType Optional entity type filter ('workflow', 'agent', 'task')
   * @param options Additional query options
   * @returns Array of checkpoint IDs
   */
  listByEntity(
    entityId: string,
    entityType?: string,
    options?: Omit<CheckpointStorageListOptions, 'executionId' | 'workflowId'>
  ): Promise<string[]>;

  /**
   * Get the latest checkpoint for a specific entity
   * 
   * @param entityId The entity ID
   * @param entityType Optional entity type filter
   * @returns Latest checkpoint ID or null if none exists
   */
  getLatestByEntity(entityId: string, entityType?: string): Promise<string | null>;

  /**
   * Delete all checkpoints for a specific entity
   * 
   * @param entityId The entity ID
   * @param entityType Optional entity type filter
   * @returns Number of deleted checkpoints
   */
  deleteByEntity(entityId: string, entityType?: string): Promise<number>;
}
