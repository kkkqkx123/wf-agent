/**
 * AgentLoopRegistryAPI - Agent Loop Registry Management API
 * Inherits GenericResourceAPI and provides unified CRUD operations.
 *
 * Responsibilities:
 * - Encapsulate AgentLoopRegistry, provide query and management of Agent Loop instances
 * - Provide statistical information, status query and other functions.
 * - Refer to the design pattern of WorkflowExecutionRegistryAPI.
 */

import { CrudResourceAPI } from "../../shared/resources/generic-resource-api.js";
import type { AgentLoopRegistry } from "../../../agent/stores/agent-loop-registry.js";
import type { AgentLoopEntity } from "../../../agent/entities/agent-loop-entity.js";
import { AgentLoopStatus, type ID } from "@wf-agent/types";
import type { ToolCallRecord } from "@wf-agent/types";
import { getErrorMessage, isSuccess, getData } from "../../shared/types/execution-result.js";
import type { APIDependencyManager } from "../../shared/core/sdk-dependencies.js";

/**
 * Agent Loop Filter
 */
export interface AgentLoopFilter {
  /** ID List */
  ids?: ID[];
  /** State Filtering */
  status?: AgentLoopStatus;
  /** Creation timeframe */
  createdAtRange?: {
    start?: number;
    end?: number;
  };
}

/**
 * Agent Loop Summary Information
 */
export interface AgentLoopSummary {
  /** Instance ID */
  id: ID;
  /** current state */
  status: AgentLoopStatus;
  /** Current number of iterations */
  currentIteration: number;
  /** Number of tool calls */
  toolCallCount: number;
  /** Starting time */
  startTime: number | null;
  /** end time */
  endTime: number | null;
  /** Execution time (milliseconds) */
  executionTime?: number;
}

/**
 * AgentLoopRegistryAPI - Agent Loop Registry Management API
 *
 * Core Responsibilities:
 * - Manage active AgentLoopEntity instances.
 * - Provide registration, query and deletion of instances
 * - Provide registration, query and deletion of instances.
 * - Provide statistical information
 */
export class AgentLoopRegistryAPI extends CrudResourceAPI<AgentLoopEntity, ID, AgentLoopFilter> {
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
   * Get a single Agent Loop instance
   * @param id instance ID
   * @returns Agent Loop entity, or null if it doesn't exist
   */
  protected async getResource(id: ID): Promise<AgentLoopEntity | null> {
    const entity = await this.registry.get(id);
    return entity || null;
  }

  /**
   * Get all Agent Loop instances
   * @returns Agent Loop Entity Array
   */
  protected async getAllResources(): Promise<AgentLoopEntity[]> {
    return this.registry.getAll();
  }

  /**
   * Create Agent Loop - Not supported, Agent Loops are created via AgentLoopEntity
   * @param resource Agent Loop entity
   */
  protected async createResource(_resource: AgentLoopEntity): Promise<void> {
    throw new Error(
      "Agent Loop creation via API is not supported. Agent Loops are created through AgentLoopEntity.",
    );
  }

  /**
   * Update Agent Loop - Not supported, Agent Loop state is managed via AgentLoopEntity
   * @param id Agent Loop ID
   * @param updates Partial updates
   */
  protected async updateResource(_id: ID, _updates: Partial<AgentLoopEntity>): Promise<void> {
    throw new Error(
      "Agent Loop update via API is not supported. Agent Loop state is managed through AgentLoopEntity.",
    );
  }

  /**
   * Deleting an Agent Loop Instance
   * First cleanup the entity resources, then unregister from the registry.
   * @param id Instance ID
   */
  protected async deleteResource(id: ID): Promise<void> {
    const entity = await this.registry.get(id);
    if (entity && typeof entity.cleanup === "function") {
      entity.cleanup();
    }
    this.registry.unregister(id);
  }

  /**
   * Apply Filtering Criteria
   * @param resources Agent Loop entity array
   * @param filter Filtering conditions
   * @returns Filtered Entity Array
   */
  protected override applyFilter(
    resources: AgentLoopEntity[],
    filter: AgentLoopFilter,
  ): AgentLoopEntity[] {
    return resources.filter(entity => {
      if (filter.ids && !filter.ids.some(id => entity.id === id)) {
        return false;
      }
      if (filter.status && entity.getStatus() !== filter.status) {
        return false;
      }
      if (filter.createdAtRange) {
        const startTime = entity.state.startTime;
        if (startTime === null) {
          return false;
        }
        if (filter.createdAtRange.start && startTime < filter.createdAtRange.start) {
          return false;
        }
        if (filter.createdAtRange.end && startTime > filter.createdAtRange.end) {
          return false;
        }
      }
      return true;
    });
  }

  /**
   * Empty all resources
   */
  protected override async clearResources(): Promise<void> {
    this.registry.clear();
  }

  // ============================================================================
  // Agent Loop Specific Methods
  // ============================================================================

  /**
   * Get a list of Agent Loop abstracts
   * @param filter filter criteria
   * @returns Agent Loop digest array
   */
  async getAgentLoopSummaries(filter?: AgentLoopFilter): Promise<AgentLoopSummary[]> {
    const result = await this.getAll(filter);
    if (!isSuccess(result)) {
      throw new Error(getErrorMessage(result) || "Failed to get agent loop summaries");
    }
    const entities = getData(result) || [];

    return entities.map(entity => {
      const startTime = entity.state.startTime;
      const endTime = entity.state.endTime;
      return {
        id: entity.id,
        status: entity.getStatus(),
        currentIteration: entity.state.currentIteration,
        toolCallCount: entity.state.toolCallCount,
        startTime,
        endTime,
        executionTime: startTime !== null && endTime !== null ? endTime - startTime : undefined,
      };
    });
  }

  /**
   * Get Agent Loop Status
   * @param id instance id
   * @returns the state, or null if it doesn't exist.
   */
  async getAgentLoopStatus(id: ID): Promise<AgentLoopStatus | null> {
    const result = await this.get(id);
    if (!isSuccess(result)) {
      throw new Error(getErrorMessage(result) || "Failed to get agent loop status");
    }
    const entity = getData(result);
    if (!entity) {
      return null;
    }
    return entity.getStatus();
  }

  /**
   * Getting a Running Agent Loop
   * @returns Agent Loop Entity Array
   */
  async getRunningAgentLoops(): Promise<AgentLoopEntity[]> {
    return this.registry.getRunning();
  }

  /**
   * Getting a Suspended Agent Loop
   * @returns Agent Loop Entity Array
   */
  async getPausedAgentLoops(): Promise<AgentLoopEntity[]> {
    return this.registry.getPaused();
  }

  /**
   * Getting a Completed Agent Loop
   * @returns Agent Loop Entity Array
   */
  async getCompletedAgentLoops(): Promise<AgentLoopEntity[]> {
    return this.registry.getCompleted();
  }

  /**
   * Getting a Failed Agent Loop
   * @returns Agent Loop Entity Array
   */
  async getFailedAgentLoops(): Promise<AgentLoopEntity[]> {
    return this.registry.getFailed();
  }

  /**
   * Getting Agent Loop Statistics
   * @returns statistics
   */
  async getAgentLoopStatistics(): Promise<{
    total: number;
    byStatus: Record<AgentLoopStatus, number>;
  }> {
    const result = await this.getAll();
    if (!isSuccess(result)) {
      throw new Error(getErrorMessage(result) || "Failed to get agent loop statistics");
    }
    const entities = getData(result) || [];

    const byStatus: Record<AgentLoopStatus, number> = {
      [AgentLoopStatus.CREATED]: 0,
      [AgentLoopStatus.RUNNING]: 0,
      [AgentLoopStatus.PAUSED]: 0,
      [AgentLoopStatus.COMPLETED]: 0,
      [AgentLoopStatus.FAILED]: 0,
      [AgentLoopStatus.CANCELLED]: 0,
    };

    for (const entity of entities) {
      const status = entity.getStatus();
      byStatus[status]++;
    }

    return {
      total: entities.length,
      byStatus,
    };
  }

  /**
   * Clearing a Completed Agent Loop
   * @returns Number of instances cleaned up
   */
  async cleanupCompletedAgentLoops(): Promise<number> {
    return this.registry.cleanupTerminated();
  }

  /**
   * Checks if the Agent Loop exists
   * @param id Instance ID
   * @returns if it exists
   */
  async hasAgentLoop(id: ID): Promise<boolean> {
    return this.registry.has(id);
  }

  /**
   * Get the number of Agent Loops
   * Number of @returns instances
   */
  async getAgentLoopCount(): Promise<number> {
    return this.registry.size();
  }

  /**
   * Get the underlying AgentLoopRegistry instance
   * @returns AgentLoopRegistry instance
   */
  getRegistry(): AgentLoopRegistry {
    return this.registry;
  }

  // ============================================================================
  // Iteration History Methods
  // ============================================================================

  /**
   * Get iteration history for an agent loop
   * @param agentLoopId Agent loop ID
   * @returns Iteration details in chronological order
   */
  async getIterationHistory(agentLoopId: ID): Promise<IterationDetail[]> {
    const entity = await this.registry.get(agentLoopId);
    if (!entity) {
      return [];
    }

    return entity.state.iterationHistory.map(record => ({
      iteration: record.iteration,
      startTime: record.startTime,
      endTime: record.endTime ?? 0,
      duration: record.endTime ? record.endTime - record.startTime : -1,
      toolCallCount: record.toolCalls.length,
      toolCalls: record.toolCalls,
      responseContent: record.responseContent,
    }));
  }

  /**
   * Get iteration history summary for an agent loop
   * @param agentLoopId Agent loop ID
   * @returns Iteration summary or null if agent loop not found
   */
  async getIterationHistorySummary(agentLoopId: ID): Promise<IterationHistorySummary | null> {
    const entity = await this.registry.get(agentLoopId);
    if (!entity) {
      return null;
    }

    const history = entity.state.iterationHistory;
    const completedIterations = history.filter(r => r.endTime != null);

    let totalDuration = 0;
    let totalToolCalls = 0;

    for (const record of history) {
      totalToolCalls += record.toolCalls.length;
      if (record.endTime) {
        totalDuration += record.endTime - record.startTime;
      }
    }

    return {
      totalIterations: history.length,
      totalToolCalls,
      totalDuration,
      averageDuration: completedIterations.length > 0
        ? Math.round(totalDuration / completedIterations.length)
        : 0,
      status: entity.getStatus(),
    };
  }
}

/**
 * Iteration detail for agent loop history
 */
export interface IterationDetail {
  /** Iteration number (1-based) */
  iteration: number;
  /** Start timestamp */
  startTime: number;
  /** End timestamp (0 if still in progress) */
  endTime: number;
  /** Duration in milliseconds (-1 if still in progress) */
  duration: number;
  /** Number of tool calls made during this iteration */
  toolCallCount: number;
  /** Tool calls made during this iteration */
  toolCalls: ToolCallRecord[];
  /** LLM response content */
  responseContent?: string;
}

/**
 * Agent loop iteration history summary
 */
export interface IterationHistorySummary {
  /** Total number of iterations */
  totalIterations: number;
  /** Total tool calls across all iterations */
  totalToolCalls: number;
  /** Total elapsed time across completed iterations (ms) */
  totalDuration: number;
  /** Average duration per iteration (ms) */
  averageDuration: number;
  /** Agent loop status */
  status: AgentLoopStatus;
}
