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
 */
export class MemoryCheckpointStorage
  extends BaseMemoryStorage<CheckpointStorageMetadata, CheckpointStorageListOptions>
  implements CheckpointStorageAdapter
{
  constructor(config: MemoryStorageConfig = {}) {
    super(config);
  }

  /**
   * List checkpoint IDs with filtering support
   */
  override async list(options?: CheckpointStorageListOptions): Promise<string[]> {
    this.ensureInitialized();
    await this.simulateLatency();

    let ids = Array.from(this.store.keys());

    // Apply filters if provided
    if (options) {
      if (options.executionId) {
        ids = ids.filter(id => {
          const entry = this.store.get(id);
          return entry?.metadata.executionId === options.executionId;
        });
      }

      if (options.workflowId) {
        ids = ids.filter(id => {
          const entry = this.store.get(id);
          return entry?.metadata.workflowId === options.workflowId;
        });
      }

      if (options.tags && options.tags.length > 0) {
        ids = ids.filter(id => {
          const entry = this.store.get(id);
          const metadataTags = entry?.metadata.tags || [];
          return options.tags!.some(tag => metadataTags.includes(tag));
        });
      }
    }

    // Apply pagination
    if (options?.offset !== undefined || options?.limit !== undefined) {
      const offset = options.offset ?? 0;
      const limit = options.limit ?? ids.length;
      ids = ids.slice(offset, offset + limit);
    }

    return ids;
  }
}
