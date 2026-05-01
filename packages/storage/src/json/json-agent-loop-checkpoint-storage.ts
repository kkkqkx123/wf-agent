/**
 * JSON File Agent Loop Checkpoint Storage Implementation
 * Agent loop checkpoint persistent storage based on JSON file system with metadata-data separation
 */

import * as path from "path";
import type { 
  AgentCheckpointMetadata,
  AgentCheckpointListOptions
} from "@wf-agent/types";
import type { AgentLoopCheckpointStorageAdapter } from "../types/adapter/agent-loop-checkpoint-adapter.js";
import { BaseJsonStorage, BaseJsonStorageConfig } from "./base-json-storage.js";

/**
 * JSON File Agent Loop Checkpoint Storage
 * Implements the AgentLoopCheckpointStorageAdapter interface
 */
export class JsonAgentLoopCheckpointStorage
  extends BaseJsonStorage<AgentCheckpointMetadata>
  implements AgentLoopCheckpointStorageAdapter
{
  constructor(config: BaseJsonStorageConfig) {
    super(config);
  }

  /**
   * Get metadata directory path for agent loop checkpoints
   */
  protected override getMetadataDir(): string {
    return path.join(this.config.baseDir, "metadata", "agent-loop-checkpoint");
  }

  /**
   * Get data directory path for agent loop checkpoints
   */
  protected override getDataDir(): string {
    return path.join(this.config.baseDir, "data", "agent-loop-checkpoint");
  }

  /**
   * List checkpoint IDs for a specific agent loop
   */
  async listByAgentLoop(
    agentLoopId: string,
    options?: Omit<AgentCheckpointListOptions, 'agentLoopId'>
  ): Promise<string[]> {
    this.ensureInitialized();

    let ids = this.getAllIds().filter(id => {
      const entry = this["metadataIndex"].get(id);
      return entry?.metadata.agentLoopId === agentLoopId;
    });

    // Apply additional filters
    if (options) {
      ids = ids.filter(id => {
        const entry = this["metadataIndex"].get(id);
        if (!entry) return false;

        const metadata = entry.metadata;

        if (options.type && metadata.type !== options.type) {
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
   * Get the latest checkpoint ID for an agent loop
   */
  async getLatestCheckpoint(agentLoopId: string): Promise<string | null> {
    this.ensureInitialized();

    const ids = await this.listByAgentLoop(agentLoopId);
    
    if (ids.length === 0) {
      return null;
    }

    // Return the first ID (already sorted by timestamp descending)
    return ids[0] ?? null;
  }

  /**
   * Delete all checkpoints for an agent loop
   */
  async deleteByAgentLoop(agentLoopId: string): Promise<number> {
    this.ensureInitialized();

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
  async list(options?: AgentCheckpointListOptions): Promise<string[]> {
    this.ensureInitialized();

    let ids = this.getAllIds();

    // Apply filtering
    if (options) {
      ids = ids.filter(id => {
        const entry = this["metadataIndex"].get(id);
        if (!entry) return false;

        const metadata = entry.metadata;

        if (options.agentLoopId && metadata.agentLoopId !== options.agentLoopId) {
          return false;
        }

        if (options.type && metadata.type !== options.type) {
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
}
