/**
 * AgentExecutionRegistryAPI - Agent Execution Registry Management API
 *
 * Provides execution-focused query and management for agent loop executions.
 * Wraps AgentLoopRegistry to provide a simplified interface for execution
 * lifecycle management, similar to WorkflowExecutionRegistryAPI.
 *
 * Responsibilities:
 * - Query and filter agent executions
 * - Get execution summaries and status
 * - Retrieve execution statistics
 * - Clean up completed executions
 *
 * @template T - Resource type (defaults to AgentLoopEntity)
 * @template ID - Resource ID type (defaults to string)
 */
import { SimplifiedCrudResourceAPI } from "../../shared/resources/generic-resource-api.js";
import type { APIDependencyManager } from "@sdk/api/shared/core/sdk-dependencies.js";
import type { AgentLoopRegistry } from "../../../agent/registry/agent-loop-registry.js";
import type { AgentLoopEntity } from "../../../agent/entities/agent-loop-entity.js";
import { AgentLoopStatus, type ID } from "@wf-agent/types";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ operation: "AgentExecutionRegistryAPI" });

/**
 * Filter for querying agent executions
 */
export interface AgentExecutionFilter {
  /** Filter by execution status */
  status?: AgentLoopStatus;
  /** Filter by specific execution IDs */
  ids?: ID[];
  /** Filter by creation time range */
  createdAtRange?: {
    start?: number;
    end?: number;
  };
}

/**
 * Summary of an agent execution
 */
export interface AgentExecutionSummary {
  /** Execution ID */
  id: ID;
  /** Current status */
  status: AgentLoopStatus;
  /** Current iteration number */
  currentIteration: number;
  /** Tool call count */
  toolCallCount: number;
  /** Start time (milliseconds) */
  startTime: number | null;
  /** End time (milliseconds) */
  endTime: number | null;
  /** Total execution duration (milliseconds) */
  executionTime?: number;
}

/**
 * AgentExecutionRegistryAPI - Agent Execution Registry Management API
 *
 * @deprecated Use {@link AgentLoopRegistryAPI} instead. AgentLoopRegistryAPI now
 * provides all execution management methods previously only available in this API.
 * This class will be removed in a future major version.
 *
 * Wraps the AgentLoopRegistry to provide execution-focused query and management
 * capabilities. This is the agent counterpart to WorkflowExecutionRegistryAPI.
 *
 * Note: createResource and updateResource are not supported (throw errors).
 * Agent executions are created through the agent loop execution engine,
 * not through this API.
 */
export class AgentExecutionRegistryAPI extends SimplifiedCrudResourceAPI<
  AgentLoopEntity,
  ID,
  AgentExecutionFilter
> {
  private registry: AgentLoopRegistry;

  /**
   * Constructor
   * @param deps APIDependencyManager instance
   */
  constructor(deps: APIDependencyManager) {
    super();
    this.registry = deps.getAgentLoopRegistry();
  }

  // ============================================================================
  // Implementing Abstract Methods
  // ============================================================================

  /**
   * Get a single agent execution by ID
   * @param id Execution ID
   * @returns AgentLoopEntity or null
   */
  protected async getResource(id: ID): Promise<AgentLoopEntity | null> {
    const entity = await this.registry.get(id);
    return entity ?? null;
  }

  /**
   * Get all active agent executions
   * @returns Array of AgentLoopEntity
   */
  protected async getAllResources(): Promise<AgentLoopEntity[]> {
    return this.registry.getAll();
  }

  /**
   * Create resource is not supported for agent executions.
   * Agent executions are created through the agent loop execution engine.
   */
  protected async createResource(_resource: AgentLoopEntity): Promise<void> {
    throw new Error(
      "AgentExecutionRegistryAPI does not support createResource. " +
        "Agent executions are created through the agent loop execution engine.",
    );
  }

  /**
   * Update resource is not supported for agent executions.
   * Agent executions are managed through the agent loop execution engine.
   */
  protected async updateResource(
    _id: ID,
    _updates: Partial<AgentLoopEntity>,
  ): Promise<void> {
    throw new Error(
      "AgentExecutionRegistryAPI does not support updateResource. " +
        "Agent executions are managed through the agent loop execution engine.",
    );
  }

  /**
   * Delete (unregister) an agent execution
   * @param id Execution ID
   */
  protected async deleteResource(id: ID): Promise<void> {
    this.registry.unregister(id);
    logger.debug("Unregistered agent execution", { executionId: id });
  }

  /**
   * Apply filter criteria to agent executions
   * @param entities Agent loop entities
   * @param filter Filter criteria
   * @returns Filtered entities
   */
  protected override applyFilter(
    entities: AgentLoopEntity[],
    filter: AgentExecutionFilter,
  ): AgentLoopEntity[] {
    let filtered = entities;

    if (filter.status) {
      filtered = filtered.filter(e => e.getStatus() === filter.status);
    }

    if (filter.ids && filter.ids.length > 0) {
      const idSet = new Set(filter.ids);
      filtered = filtered.filter(e => idSet.has(e.id));
    }

    if (filter.createdAtRange) {
      const { start, end } = filter.createdAtRange;
      if (start !== undefined) {
        filtered = filtered.filter(e => (e.state.startTime ?? 0) >= start);
      }
      if (end !== undefined) {
        filtered = filtered.filter(e => (e.state.startTime ?? 0) <= end);
      }
    }

    return filtered;
  }

  /**
   * Clear all resources (for testing)
   */
  protected override async clearResources(): Promise<void> {
    this.registry.clear();
    logger.debug("Cleared all agent executions");
  }

  // ============================================================================
  // Execution-specific Query Methods
  // ============================================================================

  /**
   * Get execution summaries with optional filtering
   * @param filter Optional filter criteria
   * @returns Array of execution summaries
   */
  async getExecutionSummaries(filter?: AgentExecutionFilter): Promise<AgentExecutionSummary[]> {
    const entities = filter
      ? this.applyFilter(this.registry.getAll(), filter)
      : this.registry.getAll();

    return entities.map(entity => {
      const startTime = entity.state.startTime ?? null;
      const endTime = entity.state.endTime ?? null;
      return {
        id: entity.id,
        status: entity.getStatus(),
        currentIteration: entity.state.currentIteration,
        toolCallCount: entity.state.toolCallCount,
        startTime,
        endTime,
        executionTime: startTime && endTime ? endTime - startTime : undefined,
      };
    });
  }

  /**
   * Get execution status by ID
   * @param id Execution ID
   * @returns AgentLoopStatus or null
   */
  async getExecutionStatus(id: ID): Promise<AgentLoopStatus | null> {
    const entity = await this.registry.get(id);
    return entity?.getStatus() ?? null;
  }

  /**
   * Get running agent executions
   * @returns Array of running executions
   */
  async getRunningExecutions(): Promise<AgentLoopEntity[]> {
    return this.registry.getRunning();
  }

  /**
   * Get paused agent executions
   * @returns Array of paused executions
   */
  async getPausedExecutions(): Promise<AgentLoopEntity[]> {
    return this.registry.getPaused();
  }

  /**
   * Get completed agent executions
   * @returns Array of completed executions
   */
  async getCompletedExecutions(): Promise<AgentLoopEntity[]> {
    return this.registry.getCompleted();
  }

  /**
   * Get failed agent executions
   * @returns Array of failed executions
   */
  async getFailedExecutions(): Promise<AgentLoopEntity[]> {
    return this.registry.getFailed();
  }

  /**
   * Get execution statistics
   * @returns Execution statistics
   */
  async getExecutionStatistics(): Promise<{
    total: number;
    running: number;
    paused: number;
    completed: number;
    failed: number;
    cancelled: number;
  }> {
    const all = this.registry.getAll();
    return {
      total: all.length,
      running: all.filter(e => e.getStatus() === AgentLoopStatus.RUNNING).length,
      paused: all.filter(e => e.getStatus() === AgentLoopStatus.PAUSED).length,
      completed: all.filter(e => e.getStatus() === AgentLoopStatus.COMPLETED).length,
      failed: all.filter(e => e.getStatus() === AgentLoopStatus.FAILED).length,
      cancelled: all.filter(e => e.getStatus() === AgentLoopStatus.CANCELLED).length,
    };
  }

  /**
   * Clean up completed and failed executions
   * @returns Number of executions cleaned up
   */
  async cleanupCompletedExecutions(): Promise<number> {
    const count = this.registry.cleanupTerminated();
    logger.debug("Cleaned up terminated agent executions", { count });
    return count;
  }

  /**
   * Check if an execution exists
   * @param id Execution ID
   * @returns True if the execution exists
   */
  async hasExecution(id: ID): Promise<boolean> {
    return this.registry.has(id);
  }

  /**
   * Get total execution count
   * @returns Number of executions
   */
  async getExecutionCount(): Promise<number> {
    return this.registry.size();
  }

  /**
   * Get the underlying registry instance
   * @returns AgentLoopRegistry instance
   */
  getRegistry(): AgentLoopRegistry {
    return this.registry;
  }
}