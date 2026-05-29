/**
 * In-Memory Checkpoint Storage Adapter
 * Fast, isolated checkpoint storage for testing
 */

import type { CheckpointStorageMetadata, CheckpointStorageListOptions } from "@wf-agent/types";
import type { CheckpointStorageAdapter } from "../types/adapter/checkpoint-adapter.js";
import { BaseMemoryStorage, type MemoryStorageConfig } from "./base-memory-storage.js";

/**
 * Memory-based checkpoint storage implementation
 * Implements CheckpointStorageAdapter interface with in-memory storage
 * Optimized with index structures for faster filtering
 */
export class MemoryCheckpointStorage
  extends BaseMemoryStorage<CheckpointStorageMetadata, CheckpointStorageListOptions>
  implements CheckpointStorageAdapter
{
  // Index structures for optimized queries
  private entityIndex: Map<string, Set<string>> = new Map();
  private tagIndex: Map<string, Set<string>> = new Map();

  constructor(config: MemoryStorageConfig = {}) {
    super(config);
  }

  /**
   * Update indexes when saving a checkpoint
   */
  private updateIndexes(id: string, metadata: CheckpointStorageMetadata): void {
    // Index by entityType and entityId (for entity-aware queries)
    if (metadata.entityId && metadata.entityType) {
      const key = `${metadata.entityType}:${metadata.entityId}`;
      if (!this.entityIndex.has(key)) {
        this.entityIndex.set(key, new Set());
      }
      this.entityIndex.get(key)!.add(id);
    }

    // Index by tags
    if (metadata.tags) {
      for (const tag of metadata.tags) {
        if (!this.tagIndex.has(tag)) {
          this.tagIndex.set(tag, new Set());
        }
        this.tagIndex.get(tag)!.add(id);
      }
    }
  }

  /**
   * Remove indexes when deleting a checkpoint
   */
  private removeIndexes(id: string, metadata: CheckpointStorageMetadata): void {
    // Remove from entity index
    if (metadata.entityId && metadata.entityType) {
      const key = `${metadata.entityType}:${metadata.entityId}`;
      const entitySet = this.entityIndex.get(key);
      if (entitySet) {
        entitySet.delete(id);
        if (entitySet.size === 0) {
          this.entityIndex.delete(key);
        }
      }
    }

    // Remove from tag index
    if (metadata.tags) {
      for (const tag of metadata.tags) {
        const tagSet = this.tagIndex.get(tag);
        if (tagSet) {
          tagSet.delete(id);
          if (tagSet.size === 0) {
            this.tagIndex.delete(tag);
          }
        }
      }
    }
  }

  /**
   * Save checkpoint with index updates
   */
  override async save(
    id: string,
    data: Uint8Array,
    metadata: CheckpointStorageMetadata,
  ): Promise<void> {
    await super.save(id, data, metadata);
    this.updateIndexes(id, metadata);
  }

  /**
   * Delete checkpoint with index cleanup
   */
  override async delete(id: string): Promise<void> {
    const entry = this.store.get(id);
    if (entry) {
      await super.delete(id);
      this.removeIndexes(id, entry.metadata);
    }
  }

  /**
   * Clear all checkpoints and indexes
   */
  override async clear(): Promise<void> {
    await super.clear();
    this.entityIndex.clear();
    this.tagIndex.clear();
  }

  /**
   * List checkpoint IDs with filtering support using indexes
   */
  override async list(options?: CheckpointStorageListOptions): Promise<string[]> {
    this.ensureInitialized();
    await this.simulateLatency();

    let ids: Set<string>;

    // Use indexes for efficient filtering
    if (options?.entityType && options?.entityId) {
      const key = `${options.entityType}:${options.entityId}`;
      ids = new Set(this.entityIndex.get(key) || []);
    } else if (options?.tags && options.tags.length > 0) {
      // For tags, find intersection of all tag sets
      const tagSets = options.tags.map(tag => this.tagIndex.get(tag) || new Set<string>());
      ids = new Set<string>(tagSets[0]);
      for (let i = 1; i < tagSets.length; i++) {
        ids = new Set([...ids].filter(id => tagSets[i]!.has(id)));
      }
    } else {
      // No filters, return all IDs
      ids = new Set(this.store.keys());
    }

    // Apply additional filters if needed
    if (options) {
      if (options.entityType && options.entityId) {
        const key = `${options.entityType}:${options.entityId}`;
        if (!this.entityIndex.has(key)) {
          // Already filtered by index, but double-check
        }
      }

      if (options.tags && options.tags.length > 0) {
        // Additional tag filtering for edge cases
        ids = new Set(
          [...ids].filter(id => {
            const entry = this.store.get(id);
            const metadataTags = entry?.metadata.tags || [];
            return options.tags!.some(tag => metadataTags.includes(tag));
          })
        );
      }
    }

    let resultIds = Array.from(ids);

    // Sort by timestamp descending
    resultIds.sort((a, b) => {
      const metaA = this.store.get(a)?.metadata;
      const metaB = this.store.get(b)?.metadata;
      return (metaB?.timestamp ?? 0) - (metaA?.timestamp ?? 0);
    });

    // Apply pagination
    if (options?.offset !== undefined || options?.limit !== undefined) {
      const offset = options.offset ?? 0;
      const limit = options.limit ?? resultIds.length;
      resultIds = resultIds.slice(offset, offset + limit);
    }

    return resultIds;
  }

  /**
   * List checkpoints with metadata only using indexes
   * More efficient for cleanup operations
   */
  async listWithMetadata(options?: CheckpointStorageListOptions): Promise<Array<{
    id: string;
    metadata: CheckpointStorageMetadata;
  }>> {
    this.ensureInitialized();
    await this.simulateLatency();

    let ids: Set<string>;

    // Use indexes for efficient filtering
    if (options?.entityType && options?.entityId) {
      const key = `${options.entityType}:${options.entityId}`;
      ids = new Set(this.entityIndex.get(key) || []);
    } else if (options?.tags && options.tags.length > 0) {
      // For tags, find intersection of all tag sets
      const tagSets = options.tags.map(tag => this.tagIndex.get(tag) || new Set<string>());
      ids = new Set<string>(tagSets[0]);
      for (let i = 1; i < tagSets.length; i++) {
        ids = new Set([...ids].filter(id => tagSets[i]!.has(id)));
      }
    } else {
      // No filters, return all IDs
      ids = new Set(this.store.keys());
    }

    // Build result items
    let items = [...ids].map(id => {
      const entry = this.store.get(id);
      return {
        id,
        metadata: entry!.metadata,
      };
    });

    // Sort by timestamp descending
    items.sort((a, b) => b.metadata.timestamp - a.metadata.timestamp);

    // Apply pagination
    if (options?.offset !== undefined || options?.limit !== undefined) {
      const offset = options.offset ?? 0;
      const limit = options.limit ?? items.length;
      items = items.slice(offset, offset + limit);
    }

    return items;
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
    await this.simulateLatency();

    const items = Array.from(this.store.keys())
      .map(id => {
        const entry = this.store.get(id);
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
      const result: { id: string; metadata: CheckpointStorageMetadata; data?: Uint8Array } = { id: item.id, metadata: item.metadata };
      if (includeData) {
        result.data = (await this.load(item.id)) ?? undefined;
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
