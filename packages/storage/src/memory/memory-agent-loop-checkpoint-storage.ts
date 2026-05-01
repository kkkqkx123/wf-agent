/**
 * In-Memory Agent Loop Checkpoint Storage Adapter
 * Fast, isolated checkpoint storage for testing agent loops
 */

import type { 
  AgentLoopCheckpointStorageMetadata, 
  AgentLoopCheckpointStorageListOptions 
} from "@wf-agent/types";
import type { AgentLoopCheckpointStorageAdapter } from "../types/adapter/agent-loop-checkpoint-adapter.js";
import { BaseMemoryStorage, type MemoryStorageConfig } from "./base-memory-storage.js";

/**
 * Memory-based agent loop checkpoint storage implementation
 * Implements AgentLoopCheckpointStorageAdapter interface with in-memory storage
 */
export class MemoryAgentLoopCheckpointStorage
  extends BaseMemoryStorage<AgentLoopCheckpointStorageMetadata, AgentLoopCheckpointStorageListOptions>
  implements AgentLoopCheckpointStorageAdapter
{
  constructor(config: MemoryStorageConfig = {}) {
    super(config);
  }

  /**
   * List checkpoint IDs for a specific agent loop
   */
  async listByAgentLoop(
    agentLoopId: string,
    options?: Omit<AgentLoopCheckpointStorageListOptions, 'agentLoopId'>
  ): Promise<string[]> {
    this.ensureInitialized();
    await this.simulateLatency();

    let ids = Array.from(this.store.keys()).filter(id => {
      const entry = this.store.get(id);
      return entry?.metadata.agentLoopId === agentLoopId;
    });

    // Apply additional filters if provided
    if (options) {
      if (options.type) {
        ids = ids.filter(id => {
          const entry = this.store.get(id);
          return entry?.metadata.type === options.type;
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

  /**
   * Get the latest checkpoint ID for an agent loop
   */
  async getLatestCheckpoint(agentLoopId: string): Promise<string | null> {
    this.ensureInitialized();
    await this.simulateLatency();

    const ids = await this.listByAgentLoop(agentLoopId);
    
    if (ids.length === 0) {
      return null;
    }

    // Find the checkpoint with the latest timestamp
    let latestId: string | null = null;
    let latestTimestamp = 0;

    for (const id of ids) {
      const entry = this.store.get(id);
      if (entry && entry.metadata.timestamp > latestTimestamp) {
        latestTimestamp = entry.metadata.timestamp;
        latestId = id;
      }
    }

    return latestId;
  }

  /**
   * Delete all checkpoints for an agent loop
   */
  async deleteByAgentLoop(agentLoopId: string): Promise<number> {
    this.ensureInitialized();
    await this.simulateLatency();

    const ids = await this.listByAgentLoop(agentLoopId);
    let deletedCount = 0;

    for (const id of ids) {
      await this.delete(id);
      deletedCount++;
    }

    return deletedCount;
  }

  /**
   * List checkpoint IDs with filtering support
   */
  override async list(options?: AgentLoopCheckpointStorageListOptions): Promise<string[]> {
    this.ensureInitialized();
    await this.simulateLatency();

    let ids = Array.from(this.store.keys());

    // Apply filters if provided
    if (options) {
      if (options.agentLoopId) {
        ids = ids.filter(id => {
          const entry = this.store.get(id);
          return entry?.metadata.agentLoopId === options.agentLoopId;
        });
      }

      if (options.type) {
        ids = ids.filter(id => {
          const entry = this.store.get(id);
          return entry?.metadata.type === options.type;
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
