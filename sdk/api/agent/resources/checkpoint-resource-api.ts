/**
 * AgentLoopCheckpointResourceAPI - Agent Loop Checkpoint Resource Management API
 * Inherits GenericResourceAPI, provides unified CRUD operations
 *
 * Responsibilities:
 * - Encapsulates AgentLoopCheckpointCoordinator, provides checkpoint creation, restoration, and query functionality
 * - Supports full checkpoints and incremental checkpoints
 * - Provides checkpoint statistics
 */

import { CrudResourceAPI } from "../../shared/resources/generic-resource-api.js";
import type { AgentLoopCheckpoint, ID } from "@wf-agent/types";
import { getErrorMessage, isSuccess, getData } from "../../shared/types/execution-result.js";
import type { AgentLoopEntity } from "../../../agent/entities/agent-loop-entity.js";
import {
  AgentLoopCheckpointCoordinator,
  type CheckpointDependencies,
  type CheckpointOptions,
} from "../../../agent/checkpoint/checkpoint-coordinator.js";

/**
 * Checkpoint Filter
 */
export interface AgentLoopCheckpointFilter {
  /** ID list */
  ids?: ID[];
  /** Agent Loop ID */
  agentLoopId?: ID;
  /** Checkpoint types */
  type?: "FULL" | "DELTA";
  /** Time range */
  timestampRange?: {
    start?: number;
    end?: number;
  };
}

/**
 * Checkpoint storage interface
 */
export interface CheckpointStorage {
  /** Save checkpoints */
  saveCheckpoint: (checkpoint: AgentLoopCheckpoint) => Promise<string>;
  /** Get the checkpoint */
  getCheckpoint: (id: string) => Promise<AgentLoopCheckpoint | null>;
  /** List the checkpoints */
  listCheckpoints: (agentLoopId: string) => Promise<string[]>;
  /** Delete checkpoints. */
  deleteCheckpoint: (id: string) => Promise<void>;
}

/**
 * AgentLoopCheckpointResourceAPI - Agent Loop Checkpoint Resource Management API
 */
export class AgentLoopCheckpointResourceAPI extends CrudResourceAPI<
  AgentLoopCheckpoint,
  string,
  AgentLoopCheckpointFilter
> {
  private storage: CheckpointStorage;
  private checkpoints: Map<string, AgentLoopCheckpoint> = new Map();
  private checkpointsByAgentLoop: Map<string, string[]> = new Map();

  constructor(storage?: CheckpointStorage) {
    super();
    this.storage = storage ?? this.createDefaultStorage();
  }

  /**
   * Create a default storage implementation
   */
  private createDefaultStorage(): CheckpointStorage {
    return {
      saveCheckpoint: async (checkpoint: AgentLoopCheckpoint) => {
        this.checkpoints.set(checkpoint.id, checkpoint);
        const list = this.checkpointsByAgentLoop.get(checkpoint.agentLoopId) || [];
        list.unshift(checkpoint.id);
        this.checkpointsByAgentLoop.set(checkpoint.agentLoopId, list);
        return checkpoint.id;
      },
      getCheckpoint: async (id: string) => {
        return this.checkpoints.get(id) || null;
      },
      listCheckpoints: async (agentLoopId: string) => {
        return this.checkpointsByAgentLoop.get(agentLoopId) || [];
      },
      deleteCheckpoint: async (id: string) => {
        const checkpoint = this.checkpoints.get(id);
        if (checkpoint) {
          this.checkpoints.delete(id);
          const list = this.checkpointsByAgentLoop.get(checkpoint.agentLoopId) || [];
          const index = list.indexOf(id);
          if (index > -1) {
            list.splice(index, 1);
          }
        }
      },
    };
  }

  // ============================================================================
  // Implement the abstract method
  // ============================================================================

  /**
   * Get a single checkpoint
   * @param id: Checkpoint ID
   * @returns: Checkpoint object; returns null if it does not exist
   */
  protected async getResource(id: string): Promise<AgentLoopCheckpoint | null> {
    return this.storage.getCheckpoint(id);
  }

  /**
   * Get all checkpoints
   * @returns Array of checkpoints
   */
  protected async getAllResources(): Promise<AgentLoopCheckpoint[]> {
    const checkpoints: AgentLoopCheckpoint[] = [];
    for (const checkpoint of this.checkpoints.values()) {
      checkpoints.push(checkpoint);
    }
    return checkpoints;
  }

  /**
   * Create checkpoint - Implemented via createCheckpoint method
   * @param resource Checkpoint object
   */
  protected async createResource(resource: AgentLoopCheckpoint): Promise<void> {
    await this.storage.saveCheckpoint(resource);
  }

  /**
   * Update checkpoint - Not supported, checkpoints are immutable
   * @param id Checkpoint ID
   * @param updates Partial updates
   */
  protected async updateResource(
    _id: string,
    _updates: Partial<AgentLoopCheckpoint>,
  ): Promise<void> {
    throw new Error("Checkpoint update via API is not supported. Checkpoints are immutable.");
  }

  /**
   * Delete a checkpoint
   * @param id Checkpoint ID
   */
  protected async deleteResource(id: string): Promise<void> {
    await this.storage.deleteCheckpoint(id);
  }

  /**
   * Apply filter criteria
   */
  protected override applyFilter(
    checkpoints: AgentLoopCheckpoint[],
    filter: AgentLoopCheckpointFilter,
  ): AgentLoopCheckpoint[] {
    return checkpoints.filter(cp => {
      if (filter.ids && !filter.ids.some(id => cp.id === id)) {
        return false;
      }
      if (filter.agentLoopId && cp.agentLoopId !== filter.agentLoopId) {
        return false;
      }
      if (filter.type && cp.type !== filter.type) {
        return false;
      }
      if (filter.timestampRange?.start && cp.timestamp < filter.timestampRange.start) {
        return false;
      }
      if (filter.timestampRange?.end && cp.timestamp > filter.timestampRange.end) {
        return false;
      }
      return true;
    });
  }

  /**
   * Clear all checkpoints.
   */
  protected override async clearResources(): Promise<void> {
    this.checkpoints.clear();
    this.checkpointsByAgentLoop.clear();
  }

  // ============================================================================
  // Checkpoint-specific method
  // ============================================================================

  /**
   * Create an Agent Loop checkpoint
   * @param entity Agent Loop entity
   * @param options Creation options
   * @returns Checkpoint ID
   */
  async createCheckpoint(entity: AgentLoopEntity, options?: CheckpointOptions): Promise<string> {
    const dependencies: CheckpointDependencies = {
      saveCheckpoint: this.storage.saveCheckpoint,
      getCheckpoint: this.storage.getCheckpoint,
      listCheckpoints: this.storage.listCheckpoints,
    };

    const coordinator = new AgentLoopCheckpointCoordinator();
    return coordinator.createCheckpoint(entity, dependencies, options);
  }

  /**
   * Restore the Agent Loop from a checkpoint
   * @param checkpointId: Checkpoint ID
   * @returns: The restored Agent Loop entity
   */
  async restoreFromCheckpoint(checkpointId: string): Promise<AgentLoopEntity> {
    const dependencies: CheckpointDependencies = {
      saveCheckpoint: this.storage.saveCheckpoint,
      getCheckpoint: this.storage.getCheckpoint,
      listCheckpoints: this.storage.listCheckpoints,
    };

    const coordinator = new AgentLoopCheckpointCoordinator();
    return coordinator.restoreFromCheckpoint(checkpointId, dependencies);
  }

  /**
   * Get the list of checkpoints for the Agent Loop
   * @param agentLoopId Agent Loop ID
   * @returns Array of checkpoints
   */
  async getAgentLoopCheckpoints(agentLoopId: string): Promise<AgentLoopCheckpoint[]> {
    const checkpointIds = await this.storage.listCheckpoints(agentLoopId);
    const checkpoints: AgentLoopCheckpoint[] = [];

    for (const id of checkpointIds) {
      const checkpoint = await this.storage.getCheckpoint(id);
      if (checkpoint) {
        checkpoints.push(checkpoint);
      }
    }

    return checkpoints;
  }

  /**
   * Get the latest checkpoint
   * @param agentLoopId Agent Loop ID
   * @returns The latest checkpoint; if it does not exist, return null
   */
  async getLatestCheckpoint(agentLoopId: string): Promise<AgentLoopCheckpoint | null> {
    const checkpointIds = await this.storage.listCheckpoints(agentLoopId);
    if (checkpointIds.length === 0) {
      return null;
    }

    return this.storage.getCheckpoint(checkpointIds[0]!);
  }

  /**
   * Retrieve checkpoint statistics information
   * @returns Statistical information
   */
  async getCheckpointStatistics(): Promise<{
    total: number;
    byAgentLoop: Record<string, number>;
    byType: Record<string, number>;
  }> {
    const result = await this.getAll();
    if (!isSuccess(result)) {
      throw new Error(getErrorMessage(result) || "Failed to get checkpoint statistics");
    }
    const checkpoints = getData(result) || [];

    const byAgentLoop: Record<string, number> = {};
    const byType: Record<string, number> = {};

    for (const checkpoint of checkpoints) {
      const agentLoopId = checkpoint.agentLoopId ?? "unknown";
      const type = checkpoint.type ?? "unknown";
      byAgentLoop[agentLoopId] = (byAgentLoop[agentLoopId] || 0) + 1;
      byType[type] = (byType[type] || 0) + 1;
    }

    return {
      total: checkpoints.length,
      byAgentLoop,
      byType,
    };
  }

  /**
   * Delete all checkpoints for the Agent Loop
   * @param agentLoopId Agent Loop ID
   * @returns Number of checkpoints deleted
   */
  async deleteAgentLoopCheckpoints(agentLoopId: string): Promise<number> {
    const checkpointIds = await this.storage.listCheckpoints(agentLoopId);
    let count = 0;

    for (const id of checkpointIds) {
      await this.storage.deleteCheckpoint(id);
      count++;
    }

    return count;
  }

  /**
   * Get the checkpoint chain
   * @param checkpointId Checkpoint ID
   * @returns Checkpoint chain (from the latest to the oldest)
   */
  async getCheckpointChain(checkpointId: string): Promise<AgentLoopCheckpoint[]> {
    const chain: AgentLoopCheckpoint[] = [];
    let currentId: string | undefined = checkpointId;

    while (currentId) {
      const checkpoint = await this.storage.getCheckpoint(currentId);
      if (!checkpoint) {
        break;
      }
      chain.push(checkpoint);
      currentId = checkpoint.previousCheckpointId;
    }

    return chain;
  }

  /**
   * Get the storage instance
   * @returns Storage instance
   */
  getStorage(): CheckpointStorage {
    return this.storage;
  }
}
