/**
 * JSON File Agent Loop Entity Storage Implementation
 * Agent loop lifecycle persistent storage based on JSON file system with metadata-data separation
 */

import * as path from "path";
import type {
  AgentEntityMetadata,
  AgentEntityListOptions
} from "@wf-agent/types";
import type { AgentLoopStorageAdapter } from "../types/adapter/agent-loop-adapter.js";
import { AgentLoopStatus } from "@wf-agent/types";
import { BaseJsonStorage, BaseJsonStorageConfig } from "./base-json-storage.js";

/**
 * JSON File Agent Loop Entity Storage
 * Implements the AgentLoopStorageAdapter interface
 */
export class JsonAgentLoopStorage
  extends BaseJsonStorage<AgentEntityMetadata>
  implements AgentLoopStorageAdapter
{
  constructor(config: BaseJsonStorageConfig) {
    super(config);
  }

  /**
   * Get metadata directory path for agent loops
   */
  protected override getMetadataDir(): string {
    return path.join(this.config.baseDir, "metadata", "agent-loop");
  }

  /**
   * Get data directory path for agent loops
   */
  protected override getDataDir(): string {
    return path.join(this.config.baseDir, "data", "agent-loop");
  }

  /**
   * Update agent loop status
   */
  async updateAgentLoopStatus(agentLoopId: string, status: AgentLoopStatus): Promise<void> {
    this.ensureInitialized();

    const entry = this["metadataIndex"].get(agentLoopId);
    if (!entry) {
      throw new Error(`Agent loop ${agentLoopId} not found`);
    }

    // Load existing data
    const data = await this.load(agentLoopId);
    if (!data) {
      throw new Error(`Agent loop data ${agentLoopId} not found`);
    }

    // Update metadata with new status
    const updatedMetadata: AgentEntityMetadata = {
      ...entry.metadata,
      status,
      updatedAt: Date.now(),
    };

    if (status === AgentLoopStatus.COMPLETED || 
        status === AgentLoopStatus.FAILED || 
        status === AgentLoopStatus.CANCELLED) {
      updatedMetadata.completedAt = Date.now();
    }

    // Save with updated metadata
    await this.save(agentLoopId, data, updatedMetadata);
  }

  /**
   * List agent loops by status
   */
  async listByStatus(status: AgentLoopStatus): Promise<string[]> {
    this.ensureInitialized();

    const ids = this.getAllIds().filter(id => {
      const entry = this["metadataIndex"].get(id);
      return entry?.metadata.status === status;
    });

    return ids;
  }

  /**
   * Get agent loop statistics
   */
  async getAgentLoopStats(): Promise<{
    total: number;
    byStatus: Record<string, number>;
  }> {
    this.ensureInitialized();

    const allIds = this.getAllIds();
    const stats: Record<string, number> = {};

    // Initialize all statuses to 0
    Object.values(AgentLoopStatus).forEach(status => {
      stats[status] = 0;
    });

    for (const id of allIds) {
      const entry = this["metadataIndex"].get(id);
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
  async list(options?: AgentEntityListOptions): Promise<string[]> {
    this.ensureInitialized();

    let ids = this.getAllIds();

    // Apply filtering
    if (options) {
      ids = ids.filter(id => {
        const entry = this["metadataIndex"].get(id);
        if (!entry) return false;

        const metadata = entry.metadata;

        if (options.status && metadata.status !== options.status) {
          return false;
        }

        if (options.profileId && metadata.profileId !== options.profileId) {
          return false;
        }

        if (options.tags && options.tags.length > 0) {
          if (!metadata.tags || !options.tags.some(tag => metadata.tags!.includes(tag))) {
            return false;
          }
        }

        if (options.createdAfter && metadata.createdAt < options.createdAfter) {
          return false;
        }

        if (options.createdBefore && metadata.createdAt > options.createdBefore) {
          return false;
        }

        return true;
      });
    }

    // Sort by createdAt descending
    ids.sort((a, b) => {
      const metaA = this["metadataIndex"].get(a)?.metadata;
      const metaB = this["metadataIndex"].get(b)?.metadata;
      return (metaB?.createdAt ?? 0) - (metaA?.createdAt ?? 0);
    });

    // Pagination
    const offset = options?.offset ?? 0;
    const limit = options?.limit ?? ids.length;

    return ids.slice(offset, offset + limit);
  }
}
