/**
 * AgentLoopCheckpointResourceAPI - Agent Loop Checkpoint Resource Management API
 * Inherits BaseCheckpointResourceAPI, provides unified checkpoint operations
 *
 * Responsibilities:
 * - Encapsulates AgentLoopCheckpointCoordinator, provides checkpoint creation, restoration, and query functionality
 * - Supports full checkpoints and incremental checkpoints
 * - Provides checkpoint statistics
 */

import { BaseCheckpointResourceAPI, type BaseCheckpointStatistics } from "../../shared/resources/checkpoint-base.js";
import type { AgentLoopCheckpoint, ID } from "@wf-agent/types";
import type { AgentLoopEntity } from "../../../agent/entities/agent-loop-entity.js";
import {
  AgentLoopCheckpointCoordinator,
  type CheckpointDependencies,
} from "../../../agent/checkpoint/checkpoint-coordinator.js";
import { AgentLoopCheckpointStateManager } from "../../../agent/checkpoint/checkpoint-state-manager.js";
import type { EventRegistry } from "../../../shared/registry/event-registry.js";
import { buildCheckpointRestoredEvent } from "../../../shared/events/builders/index.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ operation: "AgentLoopCheckpointResourceAPI" });

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
  /** Tags to filter by */
  tags?: string[];
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
 * Agent Loop checkpoint statistics with agent-specific breakdown
 */
export interface AgentLoopCheckpointStatistics extends BaseCheckpointStatistics {
  byAgentLoop: Record<string, number>;
}

/**
 * AgentLoopCheckpointResourceAPI - Agent Loop Checkpoint Resource Management API
 */
export class AgentLoopCheckpointResourceAPI extends BaseCheckpointResourceAPI<
  AgentLoopCheckpoint,
  AgentLoopCheckpointFilter
> {
  private storage: CheckpointStorage;
  private stateManager?: AgentLoopCheckpointStateManager;
  private checkpoints: Map<string, AgentLoopCheckpoint> = new Map();
  private checkpointsByAgentLoop: Map<string, string[]> = new Map();

  constructor(storage?: CheckpointStorage, stateManager?: AgentLoopCheckpointStateManager, eventManager?: EventRegistry) {
    super();
    this.storage = storage ?? this.createDefaultStorage();
    this.stateManager = stateManager;
    this.eventManager = eventManager;
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
  // Implement BaseCheckpointResourceAPI abstract helpers
  // ============================================================================

  protected getEntityId(checkpoint: AgentLoopCheckpoint): string {
    return checkpoint.agentLoopId;
  }

  protected getCheckpointType(checkpoint: AgentLoopCheckpoint): string {
    return checkpoint.type || "FULL";
  }

  protected getCheckpointTimestamp(checkpoint: AgentLoopCheckpoint): number {
    return checkpoint.timestamp;
  }

  protected getCheckpointPreviousId(checkpoint: AgentLoopCheckpoint): string | undefined {
    return checkpoint.previousCheckpointId;
  }

  protected async getCheckpointById(id: string): Promise<AgentLoopCheckpoint | null> {
    if (this.stateManager) {
      return await this.stateManager.getCheckpoint(id);
    }
    return this.storage.getCheckpoint(id);
  }

  // ============================================================================
  // Implement SimplifiedCrudResourceAPI abstract methods
  // ============================================================================

  /**
   * Get a single checkpoint
   * @param id: Checkpoint ID
   * @returns: Checkpoint object; returns null if it does not exist
   */
  protected async getResource(id: string): Promise<AgentLoopCheckpoint | null> {
    // Use state manager if available for better performance and features
    if (this.stateManager) {
      return await this.stateManager.getCheckpoint(id);
    }
    return this.storage.getCheckpoint(id);
  }

  /**
   * Get all checkpoints
   * @returns Array of checkpoints
   */
  protected async getAllResources(): Promise<AgentLoopCheckpoint[]> {
    // If state manager is available, use it for listing
    if (this.stateManager) {
      const checkpointIds = await this.stateManager.list();
      const checkpoints: AgentLoopCheckpoint[] = [];

      for (const id of checkpointIds) {
        const checkpoint = await this.stateManager.getCheckpoint(id);
        if (checkpoint) {
          checkpoints.push(checkpoint);
        }
      }
      return checkpoints;
    }

    // Fallback to in-memory storage
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
    // Use state manager if available for better performance and features
    if (this.stateManager) {
      await this.stateManager.saveCheckpoint(resource);
    } else {
      await this.storage.saveCheckpoint(resource);
    }
  }

  /**
   * Update checkpoint - Not supported, checkpoints are immutable
   * @param _id Checkpoint ID
   * @param _updates Partial updates
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
    // Use state manager if available for better performance and features
    if (this.stateManager) {
      await this.stateManager.deleteCheckpoint(id);
    } else {
      await this.storage.deleteCheckpoint(id);
    }
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
      if (filter.tags && filter.tags.length > 0) {
        const cpTags = cp.metadata?.tags ?? [];
        if (!filter.tags.every(tag => cpTags.includes(tag))) {
          return false;
        }
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
    // If state manager is available, we can't clear all at once without proper API
    // This is a limitation - in production, use cleanup policies instead
    if (this.stateManager) {
      logger.warn(
        "clearResources() called with state manager - not fully supported. Use cleanup policies instead.",
      );
      // For now, just clear the in-memory cache
      this.checkpoints.clear();
      this.checkpointsByAgentLoop.clear();
    } else {
      this.checkpoints.clear();
      this.checkpointsByAgentLoop.clear();
    }
  }

  // ============================================================================
  // Checkpoint-specific method
  // ============================================================================

  /**
   * Create an Agent Loop checkpoint
   * @param entity Agent Loop entity
   * @param options Creation options (metadata)
   * @returns Checkpoint ID
   */
  async createCheckpoint(
    entity: AgentLoopEntity,
    options?: import("@wf-agent/types").CheckpointMetadata,
  ): Promise<string> {
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
    const entity = await coordinator.restoreFromCheckpoint(checkpointId, dependencies);

    // Emit checkpoint restored event if event manager is available
    if (this.eventManager) {
      const checkpoint = await this.storage.getCheckpoint(checkpointId);
      if (checkpoint) {
        await this.eventManager.emit(
          buildCheckpointRestoredEvent({
            executionId: checkpoint.agentLoopId,
            checkpointId,
            description: checkpoint.metadata?.description,
          }),
        );
      }
    }

    return entity;
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
   * Get checkpoint statistics with agent-specific breakdown
   * @returns Statistical information
   */
  override async getCheckpointStatistics(): Promise<AgentLoopCheckpointStatistics> {
    const baseStats = await super.getCheckpointStatistics();
    const checkpoints = await this.getAll();

    const byAgentLoop: Record<string, number> = {};
    for (const checkpoint of checkpoints) {
      const agentLoopId = checkpoint.agentLoopId ?? "unknown";
      byAgentLoop[agentLoopId] = (byAgentLoop[agentLoopId] || 0) + 1;
    }

    return {
      ...baseStats,
      byAgentLoop,
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
   * Get the storage instance
   * @returns Storage instance
   */
  getStorage(): CheckpointStorage {
    return this.storage;
  }
}