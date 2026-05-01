/**
 * In-Memory Agent Loop Entity Storage Adapter
 * Fast, isolated storage for agent loop lifecycle management
 */

import type { 
  AgentLoopStorageMetadata, 
  AgentLoopStorageListOptions 
} from "@wf-agent/types";
import type { AgentLoopStorageAdapter } from "../types/adapter/agent-loop-adapter.js";
import { AgentLoopStatus } from "@wf-agent/types";
import { BaseMemoryStorage, type MemoryStorageConfig } from "./base-memory-storage.js";

/**
 * Memory-based agent loop entity storage implementation
 * Implements AgentLoopStorageAdapter interface with in-memory storage
 */
export class MemoryAgentLoopStorage
  extends BaseMemoryStorage<AgentLoopStorageMetadata, AgentLoopStorageListOptions>
  implements AgentLoopStorageAdapter
{
  constructor(config: MemoryStorageConfig = {}) {
    super(config);
  }

  /**
   * Update agent loop status
   */
  async updateAgentLoopStatus(agentLoopId: string, status: AgentLoopStatus): Promise<void> {
    this.ensureInitialized();
    await this.simulateLatency();

    const entry = this.store.get(agentLoopId);
    if (!entry) {
      throw new Error(`Agent loop ${agentLoopId} not found`);
    }

    // Update metadata with new status
    const updatedMetadata: AgentLoopStorageMetadata = {
      ...entry.metadata,
      status,
      updatedAt: Date.now(),
    };

    if (status === AgentLoopStatus.COMPLETED || status === AgentLoopStatus.FAILED || status === AgentLoopStatus.CANCELLED) {
      updatedMetadata.completedAt = Date.now();
    }

    // Save with updated metadata
    await this.save(agentLoopId, entry.data, updatedMetadata);
  }

  /**
   * List agent loops by status
   */
  async listByStatus(status: AgentLoopStatus): Promise<string[]> {
    this.ensureInitialized();
    await this.simulateLatency();

    const ids = Array.from(this.store.keys()).filter(id => {
      const entry = this.store.get(id);
      return entry?.metadata.status === status;
    });

    return ids;
  }

  /**
   * Get agent loop statistics (implements AgentLoopStorageAdapter interface)
   */
  async getAgentLoopStats(): Promise<{
    total: number;
    byStatus: Record<string, number>;
  }> {
    this.ensureInitialized();
    await this.simulateLatency();

    const allIds = Array.from(this.store.keys());
    const stats: Record<string, number> = {};

    // Initialize all statuses to 0
    Object.values(AgentLoopStatus).forEach(status => {
      stats[status] = 0;
    });

    for (const id of allIds) {
      const entry = this.store.get(id);
      if (entry) {
        const status = entry.metadata.status;
        stats[status] = (stats[status] || 0) + 1;
      }
    }

    return {
      total: allIds.length,
      byStatus: stats,
    };
  }

  /**
   * List agent loop IDs with filtering support
   */
  override async list(options?: AgentLoopStorageListOptions): Promise<string[]> {
    this.ensureInitialized();
    await this.simulateLatency();

    let ids = Array.from(this.store.keys());

    // Apply filters if provided
    if (options) {
      if (options.status) {
        ids = ids.filter(id => {
          const entry = this.store.get(id);
          return entry?.metadata.status === options.status;
        });
      }

      if (options.profileId) {
        ids = ids.filter(id => {
          const entry = this.store.get(id);
          return entry?.metadata.profileId === options.profileId;
        });
      }

      if (options.tags && options.tags.length > 0) {
        ids = ids.filter(id => {
          const entry = this.store.get(id);
          const metadataTags = entry?.metadata.tags || [];
          return options.tags!.some(tag => metadataTags.includes(tag));
        });
      }

      if (options.createdAfter) {
        ids = ids.filter(id => {
          const entry = this.store.get(id);
          return (entry?.metadata.createdAt ?? 0) >= options.createdAfter!;
        });
      }

      if (options.createdBefore) {
        ids = ids.filter(id => {
          const entry = this.store.get(id);
          return (entry?.metadata.createdAt ?? 0) <= options.createdBefore!;
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
