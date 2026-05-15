/**
 * JSON File Checkpoint Storage Implementation
 * Checkpoint persistent storage based on JSON file system with metadata-data separation
 */

import * as path from "path";
import type { CheckpointStorageMetadata, CheckpointStorageListOptions } from "@wf-agent/types";
import type { CheckpointStorageAdapter } from "../types/adapter/checkpoint-adapter.js";
import type { CheckpointOptions } from "../types/checkpoint-options.js";
import { BaseJsonStorage, BaseJsonStorageConfig } from "./base-json-storage.js";

/**
 * JSON File Checkpoint Storage
 * Implements the CheckpointStorageAdapter interface
 */
export class JsonCheckpointStorage
  extends BaseJsonStorage<CheckpointStorageMetadata, CheckpointOptions>
  implements CheckpointStorageAdapter
{
  constructor(config: BaseJsonStorageConfig) {
    super(config);
  }

  /**
   * Get metadata directory path for checkpoints
   */
  protected override getMetadataDir(): string {
    return path.join(this.config.baseDir, "metadata", "checkpoint");
  }

  /**
   * Get data directory path for checkpoints
   */
  protected override getDataDir(): string {
    return path.join(this.config.baseDir, "data", "checkpoint");
  }

  /**
   * List checkpoint IDs
   */
  async list(options?: CheckpointStorageListOptions): Promise<string[]> {
    this.ensureInitialized();

    let ids = this.getAllIds();

    // Apply filtering
    if (options) {
      ids = ids.filter(id => {
        const entry = this["metadataIndex"].get(id);
        if (!entry) return false;

        const metadata = entry.metadata;

        if (options.executionId && metadata.executionId !== options.executionId) {
          return false;
        }

        if (options.workflowId && metadata.workflowId !== options.workflowId) {
          return false;
        }

        if (options.tags && options.tags.length > 0) {
          if (!metadata.tags || !options.tags.some(tag => metadata.tags!.includes(tag))) {
            return false;
          }
        }

        return true;
      });
    }

    // Sort by timestamp descending
    ids.sort((a, b) => {
      const metaA = this["metadataIndex"].get(a)?.metadata;
      const metaB = this["metadataIndex"].get(b)?.metadata;
      return (metaB?.timestamp ?? 0) - (metaA?.timestamp ?? 0);
    });

    // Pagination
    const offset = options?.offset ?? 0;
    const limit = options?.limit ?? ids.length;

    return ids.slice(offset, offset + limit);
  }

  /**
   * List checkpoints with metadata only
   * More efficient for cleanup operations
   */
  async listWithMetadata(options?: CheckpointStorageListOptions): Promise<Array<{
    id: string;
    metadata: CheckpointStorageMetadata;
  }>> {
    this.ensureInitialized();

    let items = Array.from(this["metadataIndex"].entries()).map(([id, entry]) => ({
      id,
      metadata: entry.metadata,
    }));

    // Apply filtering
    if (options) {
      items = items.filter(item => {
        const metadata = item.metadata;

        if (options.executionId && metadata.executionId !== options.executionId) {
          return false;
        }

        if (options.workflowId && metadata.workflowId !== options.workflowId) {
          return false;
        }

        if (options.tags && options.tags.length > 0) {
          if (!metadata.tags || !options.tags.some(tag => metadata.tags!.includes(tag))) {
            return false;
          }
        }

        return true;
      });
    }

    // Sort by timestamp descending
    items.sort((a, b) => b.metadata.timestamp - a.metadata.timestamp);

    // Pagination
    const offset = options?.offset ?? 0;
    const limit = options?.limit ?? items.length;

    return items.slice(offset, offset + limit);
  }

  /**
   * List checkpoints for a specific entity with metadata
   */
  async listByEntityWithMetadata(
    entityId: string,
    entityType: string,
    options?: { limit?: number; offset?: number }
  ): Promise<Array<{ id: string; metadata: CheckpointStorageMetadata }>> {
    this.ensureInitialized();

    const items = this.getAllIds()
      .map(id => {
        const entry = this["metadataIndex"].get(id);
        if (!entry) return null;
        
        const metadata = entry.metadata;
        if (metadata.entityId !== entityId || metadata.entityType !== entityType) {
          return null;
        }
        
        return { id, metadata };
      })
      .filter((item): item is NonNullable<typeof item> => item !== null);

    // Sort by timestamp descending
    items.sort((a, b) => (b.metadata.timestamp ?? 0) - (a.metadata.timestamp ?? 0));

    // Pagination
    const offset = options?.offset ?? 0;
    const limit = options?.limit ?? items.length;

    return items.slice(offset, offset + limit);
  }

  /**
   * Get the latest N checkpoints for a specific entity
   */
  async getLatestByEntity(
    entityId: string,
    entityType: string,
    count: number = 1,
    includeData: boolean = false
  ): Promise<Array<{ id: string; metadata: CheckpointStorageMetadata; data?: Uint8Array }>> {
    const list = await this.listByEntityWithMetadata(entityId, entityType, { limit: count });
    
    const results: Array<{ id: string; metadata: CheckpointStorageMetadata; data?: Uint8Array }> = [];
    
    for (const item of list) {
      const result: any = { id: item.id, metadata: item.metadata };
      if (includeData) {
        result.data = await this.load(item.id);
      }
      results.push(result);
    }
    
    return results;
  }

  /**
   * Delete checkpoints for a specific entity with advanced options
   */
  async deleteByEntity(
    entityId: string,
    entityType: string,
    options?: { keepLatest?: number; olderThan?: number }
  ): Promise<number> {
    let list = await this.listByEntityWithMetadata(entityId, entityType);
    
    // Apply time-based filter
    if (options?.olderThan) {
      list = list.filter(item => (item.metadata.timestamp ?? 0) < options.olderThan!);
    }
    
    // Apply keepLatest filter
    if (options?.keepLatest && options.keepLatest > 0) {
      list = list.slice(options.keepLatest);
    }
    
    let deletedCount = 0;
    for (const item of list) {
      await this.delete(item.id);
      deletedCount++;
    }

    return deletedCount;
  }
}
