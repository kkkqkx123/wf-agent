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
  private executionIndex: Map<string, Set<string>> = new Map();
  private workflowIndex: Map<string, Set<string>> = new Map();
  private tagIndex: Map<string, Set<string>> = new Map();

  constructor(config: MemoryStorageConfig = {}) {
    super(config);
  }

  /**
   * Update indexes when saving a checkpoint
   */
  private updateIndexes(id: string, metadata: CheckpointStorageMetadata): void {
    // Index by executionId (if present)
    if (metadata.executionId) {
      if (!this.executionIndex.has(metadata.executionId)) {
        this.executionIndex.set(metadata.executionId, new Set());
      }
      this.executionIndex.get(metadata.executionId)!.add(id);
    }

    // Index by workflowId (if present)
    if (metadata.workflowId) {
      if (!this.workflowIndex.has(metadata.workflowId)) {
        this.workflowIndex.set(metadata.workflowId, new Set());
      }
      this.workflowIndex.get(metadata.workflowId)!.add(id);
    }

    // Index by entityId and entityType (for entity-aware queries)
    if (metadata.entityId && metadata.entityType) {
      const key = `${metadata.entityType}:${metadata.entityId}`;
      if (!this.executionIndex.has(key)) {
        this.executionIndex.set(key, new Set());
      }
      this.executionIndex.get(key)!.add(id);
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
    // Remove from execution index
    if (metadata.executionId) {
      const execSet = this.executionIndex.get(metadata.executionId);
      if (execSet) {
        execSet.delete(id);
        if (execSet.size === 0) {
          this.executionIndex.delete(metadata.executionId);
        }
      }
    }

    // Remove from workflow index
    if (metadata.workflowId) {
      const workflowSet = this.workflowIndex.get(metadata.workflowId);
      if (workflowSet) {
        workflowSet.delete(id);
        if (workflowSet.size === 0) {
          this.workflowIndex.delete(metadata.workflowId);
        }
      }
    }

    // Remove from entity index
    if (metadata.entityId && metadata.entityType) {
      const key = `${metadata.entityType}:${metadata.entityId}`;
      const entitySet = this.executionIndex.get(key);
      if (entitySet) {
        entitySet.delete(id);
        if (entitySet.size === 0) {
          this.executionIndex.delete(key);
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
    this.executionIndex.clear();
    this.workflowIndex.clear();
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
    if (options?.executionId) {
      ids = new Set(this.executionIndex.get(options.executionId) || []);
    } else if (options?.workflowId) {
      ids = new Set(this.workflowIndex.get(options.workflowId) || []);
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
      if (options.executionId && !this.executionIndex.has(options.executionId)) {
        // Already filtered by index, but double-check
      }
      
      if (options.workflowId && !this.workflowIndex.has(options.workflowId)) {
        // Already filtered by index, but double-check
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
    if (options?.executionId) {
      ids = new Set(this.executionIndex.get(options.executionId) || []);
    } else if (options?.workflowId) {
      ids = new Set(this.workflowIndex.get(options.workflowId) || []);
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
   * List checkpoints for a specific entity
   */
  async listByEntity(
    entityId: string,
    entityType?: string,
    options?: Omit<CheckpointStorageListOptions, 'executionId' | 'workflowId'>
  ): Promise<string[]> {
    this.ensureInitialized();
    await this.simulateLatency();

    let ids = Array.from(this.store.keys()).filter(id => {
      const entry = this.store.get(id);
      if (!entry) return false;
      
      const metadata = entry.metadata;
      if (metadata.entityId !== entityId) return false;
      if (entityType && metadata.entityType !== entityType) return false;
      
      return true;
    });

    // Sort by timestamp descending
    ids.sort((a, b) => {
      const metaA = this.store.get(a)?.metadata;
      const metaB = this.store.get(b)?.metadata;
      return (metaB?.timestamp ?? 0) - (metaA?.timestamp ?? 0);
    });

    // Pagination
    const offset = options?.offset ?? 0;
    const limit = options?.limit ?? ids.length;

    return ids.slice(offset, offset + limit);
  }

  /**
   * Get the latest checkpoint for a specific entity
   */
  async getLatestByEntity(entityId: string, entityType?: string): Promise<string | null> {
    this.ensureInitialized();
    await this.simulateLatency();

    const ids = await this.listByEntity(entityId, entityType, { limit: 1 });
    return ids.length > 0 ? ids[0]! : null;
  }

  /**
   * Delete all checkpoints for a specific entity
   */
  async deleteByEntity(entityId: string, entityType?: string): Promise<number> {
    this.ensureInitialized();
    await this.simulateLatency();

    const ids = await this.listByEntity(entityId, entityType);
    let deletedCount = 0;

    for (const id of ids) {
      await this.delete(id);
      deletedCount++;
    }

    return deletedCount;
  }
}
