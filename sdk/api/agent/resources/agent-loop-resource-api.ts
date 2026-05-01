/**
 * AgentLoopResourceAPI - Agent Loop Entity Resource Management API
 * Inherits GenericResourceAPI, provides unified CRUD operations for agent loop entities
 *
 * Responsibilities:
 * - Manages agent loop entity lifecycle (create, read, update, delete)
 * - Integrates with AgentLoopStateManager for persistence
 * - Provides filtering and querying capabilities
 */

import { CrudResourceAPI } from "../../shared/resources/generic-resource-api.js";
import type { ID, AgentLoopStatus } from "@wf-agent/types";
import type { AgentLoopEntity } from "../../../agent/entities/agent-loop-entity.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import { isSuccess, getData } from "../../shared/types/execution-result.js";

const logger = createContextualLogger({ operation: "AgentLoopResourceAPI" });

/**
 * Agent Loop Filter
 */
export interface AgentLoopFilter {
  /** ID list */
  ids?: ID[];
  /** Status filter */
  status?: AgentLoopStatus;
  /** Profile ID */
  profileId?: string;
  /** Tag array */
  tags?: string[];
  /** Time range for creation */
  createdAfter?: number;
  createdBefore?: number;
}

/**
 * Agent Loop Summary
 */
export interface AgentLoopSummary {
  /** Agent Loop ID */
  id: ID;
  /** Status */
  status: AgentLoopStatus;
  /** Current iteration */
  currentIteration: number;
  /** Tool call count */
  toolCallCount: number;
  /** Start time */
  startTime: number | null;
  /** End time */
  endTime: number | null;
  /** Profile ID */
  profileId?: string;
}

/**
 * Agent Loop Storage Interface
 */
export interface AgentLoopStorage {
  /** Save agent loop entity */
  saveAgentLoop: (entity: AgentLoopEntity) => Promise<void>;
  /** Load agent loop entity */
  loadAgentLoop: (agentLoopId: string) => Promise<AgentLoopEntity | null>;
  /** Update agent loop status */
  updateStatus: (agentLoopId: string, status: AgentLoopStatus) => Promise<void>;
  /** List agent loops by status */
  listByStatus: (status: AgentLoopStatus) => Promise<string[]>;
  /** Delete agent loop */
  deleteAgentLoop: (agentLoopId: string) => Promise<void>;
  /** List all agent loop IDs */
  listAll: () => Promise<string[]>;
}

/**
 * AgentLoopResourceAPI - Agent Loop Entity Resource Management API
 */
export class AgentLoopResourceAPI extends CrudResourceAPI<
  AgentLoopEntity,
  string,
  AgentLoopFilter
> {
  private storage: AgentLoopStorage;
  private entities: Map<string, AgentLoopEntity> = new Map();

  constructor(storage?: AgentLoopStorage) {
    super();
    this.storage = storage ?? this.createDefaultStorage();
  }

  /**
   * Create a default in-memory storage implementation
   */
  private createDefaultStorage(): AgentLoopStorage {
    return {
      saveAgentLoop: async (entity: AgentLoopEntity) => {
        this.entities.set(entity.id, entity);
      },
      loadAgentLoop: async (agentLoopId: string) => {
        return this.entities.get(agentLoopId) || null;
      },
      updateStatus: async (agentLoopId: string, status: AgentLoopStatus) => {
        const entity = this.entities.get(agentLoopId);
        if (entity) {
          // Note: In real implementation, update the state properly
          logger.debug("Updated agent loop status", { agentLoopId, status });
        }
      },
      listByStatus: async (status: AgentLoopStatus) => {
        const ids: string[] = [];
        for (const [id, entity] of this.entities.entries()) {
          if (entity.getStatus() === status) {
            ids.push(id);
          }
        }
        return ids;
      },
      deleteAgentLoop: async (agentLoopId: string) => {
        this.entities.delete(agentLoopId);
      },
      listAll: async () => {
        return Array.from(this.entities.keys());
      },
    };
  }

  // ============================================================================
  // Implement the abstract methods
  // ============================================================================

  /**
   * Get a single agent loop entity
   * @param id Agent Loop ID
   * @returns Agent Loop entity; returns null if it does not exist
   */
  protected async getResource(id: string): Promise<AgentLoopEntity | null> {
    return await this.storage.loadAgentLoop(id);
  }

  /**
   * Get all agent loop entities
   * @returns Array of agent loop entities
   */
  protected async getAllResources(): Promise<AgentLoopEntity[]> {
    const ids = await this.storage.listAll();
    const entities: AgentLoopEntity[] = [];

    for (const id of ids) {
      const entity = await this.storage.loadAgentLoop(id);
      if (entity) {
        entities.push(entity);
      }
    }

    return entities;
  }

  /**
   * Create agent loop entity
   * @param resource Agent Loop entity
   */
  protected async createResource(resource: AgentLoopEntity): Promise<void> {
    await this.storage.saveAgentLoop(resource);
    logger.info("Agent loop entity created", { agentLoopId: resource.id });
  }

  /**
   * Update agent loop entity
   * @param id Agent Loop ID
   * @param updates Partial updates (Note: Limited support as entities are mostly immutable)
   */
  protected async updateResource(_id: string, _updates: Partial<AgentLoopEntity>): Promise<void> {
    throw new Error(
      "Direct entity update is not supported. Use specific methods like updateStatus instead.",
    );
  }

  /**
   * Delete agent loop entity
   * @param id Agent Loop ID
   */
  protected async deleteResource(id: string): Promise<void> {
    await this.storage.deleteAgentLoop(id);
    logger.info("Agent loop entity deleted", { agentLoopId: id });
  }

  /**
   * Apply filter criteria
   */
  protected override applyFilter(
    entities: AgentLoopEntity[],
    filter: AgentLoopFilter,
  ): AgentLoopEntity[] {
    return entities.filter(entity => {
      if (filter.ids && !filter.ids.some(id => entity.id === id)) {
        return false;
      }
      if (filter.status && entity.getStatus() !== filter.status) {
        return false;
      }
      if (filter.profileId && entity.config.profileId !== filter.profileId) {
        return false;
      }
      if (filter.createdAfter && entity.state.startTime && entity.state.startTime < filter.createdAfter) {
        return false;
      }
      if (filter.createdBefore && entity.state.startTime && entity.state.startTime > filter.createdBefore) {
        return false;
      }
      // TODO: Add tag filtering when tags are supported
      return true;
    });
  }

  /**
   * Clear all agent loop entities
   */
  protected override async clearResources(): Promise<void> {
    const ids = await this.storage.listAll();
    for (const id of ids) {
      await this.storage.deleteAgentLoop(id);
    }
    this.entities.clear();
  }

  // ============================================================================
  // Agent Loop-specific methods
  // ============================================================================

  /**
   * Update agent loop status
   * @param agentLoopId Agent Loop ID
   * @param status New status
   */
  async updateStatus(agentLoopId: string, status: AgentLoopStatus): Promise<void> {
    await this.storage.updateStatus(agentLoopId, status);
    logger.info("Agent loop status updated", { agentLoopId, status });
  }

  /**
   * List agent loops by status
   * @param status Status to filter by
   * @returns Array of agent loop entities
   */
  async listByStatus(status: AgentLoopStatus): Promise<AgentLoopEntity[]> {
    const ids = await this.storage.listByStatus(status);
    const entities: AgentLoopEntity[] = [];

    for (const id of ids) {
      const entity = await this.storage.loadAgentLoop(id);
      if (entity) {
        entities.push(entity);
      }
    }

    return entities;
  }

  /**
   * Get agent loop summary
   * @param agentLoopId Agent Loop ID
   * @returns Summary information
   */
  async getSummary(agentLoopId: string): Promise<AgentLoopSummary | null> {
    const entity = await this.storage.loadAgentLoop(agentLoopId);
    if (!entity) {
      return null;
    }

    return {
      id: entity.id,
      status: entity.getStatus(),
      currentIteration: entity.state.currentIteration,
      toolCallCount: entity.state.toolCallCount,
      startTime: entity.state.startTime,
      endTime: entity.state.endTime,
      profileId: entity.config.profileId,
    };
  }

  /**
   * List agent loop summaries
   * @param filter Optional filter
   * @returns Array of summaries
   */
  async listSummaries(filter?: AgentLoopFilter): Promise<AgentLoopSummary[]> {
    const result = await this.getAll();
    
    if (!isSuccess(result) || !getData(result)) {
      return [];
    }

    const entities = getData(result)!;
    const filteredEntities = filter ? this.applyFilter(entities, filter) : entities;

    return filteredEntities.map(entity => ({
      id: entity.id,
      status: entity.getStatus(),
      currentIteration: entity.state.currentIteration,
      toolCallCount: entity.state.toolCallCount,
      startTime: entity.state.startTime,
      endTime: entity.state.endTime,
      profileId: entity.config.profileId,
    }));
  }

  /**
   * Get agent loop statistics
   * @returns Statistical information
   */
  async getStatistics(): Promise<{
    total: number;
    byStatus: Record<AgentLoopStatus, number>;
  }> {
    const result = await this.getAll();
    
    if (!isSuccess(result) || !getData(result)) {
      return {
        total: 0,
        byStatus: {} as Record<AgentLoopStatus, number>,
      };
    }

    const entities = getData(result)!;
    const byStatus: Record<string, number> = {};
    
    for (const entity of entities) {
      const status = entity.getStatus();
      byStatus[status] = (byStatus[status] || 0) + 1;
    }

    return {
      total: entities.length,
      byStatus: byStatus as Record<AgentLoopStatus, number>,
    };
  }

  /**
   * Get the storage instance
   * @returns Storage instance
   */
  getStorage(): AgentLoopStorage {
    return this.storage;
  }
}
