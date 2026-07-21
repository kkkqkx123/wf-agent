/**
 * AgentLoopRegistryAPI - Agent Loop Registry Management API (Read-Only Query Registry)
 *
 * This is a **read-only query registry** focused on execution analytics.
 * It provides query methods for iteration history, timeline, variable history,
 * context evolution, and execution statistics.
 *
 * Responsibilities:
 * - Query and retrieve Agent Loop instances from the underlying registry
 * - Provide execution analytics: iteration history, timeline, variable history, context evolution
 * - Provide statistical information and status queries
 * - Refer to the design pattern of WorkflowExecutionRegistryAPI.
 *
 * Note: createResource and updateResource are not supported (throw errors).
 * Agent Loops are created through AgentLoopEntity, not through this API.
 */

import { SimplifiedCrudResourceAPI } from "../../shared/resources/generic-resource-api.js";
import type { AgentLoopRegistry } from "../../../agent/registry/agent-loop-registry.js";
import type { AgentLoopEntity } from "../../../agent/entities/agent-loop-entity.js";
import { AgentLoopStatus, type ID } from "@wf-agent/types";
import type { ToolCallRecord } from "@wf-agent/types";
import type { APIDependencyManager } from "@sdk/api/shared/core/sdk-dependencies.js";

/**
 * Agent Loop Filter
 */
export interface AgentLoopFilter {
  /** ID List */
  ids?: ID[];
  /** State Filtering */
  status?: AgentLoopStatus;
  /** Profile ID filter */
  profileId?: string;
  /** Tag filter */
  tags?: string[];
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
  /** Profile ID */
  profileId?: string;
}

/**
 * AgentLoopRegistryAPI - Agent Loop Registry Management API (Read-Only Query Registry)
 *
 * This is a **read-only query registry** focused on execution analytics.
 * It provides query methods for iteration history, timeline, variable history,
 * context evolution, and execution statistics.
 *
 * Core Responsibilities:
 * - Query and retrieve Agent Loop instances from the underlying registry
 * - Provide execution analytics: iteration history, timeline, variable history, context evolution
 * - Provide statistical information and status queries
 * - Refer to the design pattern of WorkflowExecutionRegistryAPI.
 *
 * Note: createResource and updateResource are not supported and throw errors.
 * Agent Loops are created through AgentLoopEntity, not through this API.
 */
export class AgentLoopRegistryAPI extends SimplifiedCrudResourceAPI<AgentLoopEntity, ID, AgentLoopFilter> {
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
   * Create Agent Loop - Not supported
   *
   * @deprecated Agent Loop creation is not supported via this read-only query registry.
   * Agent Loops are created through AgentLoopEntity.
   * @param resource Agent Loop entity
   */
  protected async createResource(_resource: AgentLoopEntity): Promise<void> {
    throw new Error(
      "Agent Loop creation via API is not supported. Agent Loops are created through AgentLoopEntity.",
    );
  }

  /**
   * Update Agent Loop - Not supported
   *
   * @deprecated Agent Loop update is not supported via this read-only query registry.
   * Agent Loop state is managed through AgentLoopEntity.
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
      if (filter.profileId && entity.config.profileId !== filter.profileId) {
        return false;
      }
      if (filter.tags && filter.tags.length > 0) {
        const entityTags = entity.config.tags || [];
        if (!filter.tags.some(tag => entityTags.includes(tag))) {
          return false;
        }
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
    const entities = await this.getAll(filter);

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
        profileId: entity.config.profileId,
      };
    });
  }

  /**
   * Get a single Agent Loop summary by ID
   * @param agentLoopId Agent Loop ID
   * @returns Agent Loop summary, or null if not found
   */
  async getAgentLoopSummary(agentLoopId: ID): Promise<AgentLoopSummary | null> {
    const entity = await this.get(agentLoopId);
    if (!entity) {
      return null;
    }

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
      profileId: entity.config.profileId,
    };
  }

  /**
   * List agent loops by status
   * @param status Status to filter by
   * @returns Array of matching agent loop entities
   */
  async listByStatus(status: AgentLoopStatus): Promise<AgentLoopEntity[]> {
    return this.registry.getByStatus(status);
  }

  /**
   * Update agent loop status
   * Delegates to the entity's state management methods for lifecycle transitions.
   * @param agentLoopId Agent Loop ID
   * @param status New status to set
   */
  async updateStatus(agentLoopId: ID, status: AgentLoopStatus): Promise<void> {
    const entity = await this.registry.get(agentLoopId);
    if (!entity) {
      throw new Error(`Agent Loop not found: ${agentLoopId}`);
    }
    // Use the entity's state management methods for proper lifecycle transitions
    switch (status) {
      case AgentLoopStatus.RUNNING:
        entity.resume();
        break;
      case AgentLoopStatus.PAUSED:
        entity.pause();
        break;
      case AgentLoopStatus.CANCELLED:
      case AgentLoopStatus.STOPPED:
        entity.stop();
        break;
      default:
        // For other status changes, directly set the status
        entity.state.status = status;
        break;
    }
  }

  /**
   * Get Agent Loop Status
   * @param id instance id
   * @returns the state, or null if it doesn't exist.
   */
  async getAgentLoopStatus(id: ID): Promise<AgentLoopStatus | null> {
    const entity = await this.get(id);
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
    const entities = await this.getAll();

    const byStatus: Record<AgentLoopStatus, number> = {
      [AgentLoopStatus.CREATED]: 0,
      [AgentLoopStatus.RUNNING]: 0,
      [AgentLoopStatus.PAUSED]: 0,
      [AgentLoopStatus.COMPLETED]: 0,
      [AgentLoopStatus.FAILED]: 0,
      [AgentLoopStatus.CANCELLED]: 0,
      [AgentLoopStatus.STOPPED]: 0,
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
  //
  // @deprecated Use {@link AgentLoopIterationAPI} instead for iteration-level analysis.
  // The methods below are kept for backward compatibility but delegate to the
  // more specific AgentLoopIterationAPI where possible.
  // ============================================================================

  /**
   * Get iteration history for an agent loop
   *
   * @deprecated Use {@link AgentLoopIterationAPI#getExtendedHistorySummary} for enhanced iteration analysis.
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
   *
   * @deprecated Use {@link AgentLoopIterationAPI#getExtendedHistorySummary} for enhanced iteration analysis.
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
      averageDuration:
        completedIterations.length > 0 ? Math.round(totalDuration / completedIterations.length) : 0,
      status: entity.getStatus(),
    };
  }

  // ============================================================================
  // Execution History APIs
  //
  // @deprecated Use {@link AgentExecutionStateAPI} for execution timeline and state tracking.
  // The methods below are kept for backward compatibility.
  // ============================================================================

  /**
   * Get execution timeline for an agent loop
   *
   * @deprecated Use {@link AgentExecutionStateAPI#getExecutionTimeline} instead.
   * @param agentLoopId Agent loop ID
   * @returns Timeline entries sorted by timestamp
   */
  async getExecutionTimeline(agentLoopId: ID): Promise<ExecutionTimelineEntry[]> {
    const entity = await this.registry.get(agentLoopId);
    if (!entity) {
      return [];
    }

    const timeline: ExecutionTimelineEntry[] = [];
    const state = entity.state;

    // Add start event
    if (state.startTime) {
      timeline.push({
        id: `${agentLoopId}:start`,
        timestamp: state.startTime,
        type: 'execution_start',
        description: 'Agent loop execution started',
        iteration: 0,
      });
    }

    // Add iteration events
    for (const record of state.iterationHistory) {
      timeline.push({
        id: `${agentLoopId}:iteration:${record.iteration}:start`,
        timestamp: record.startTime,
        type: 'iteration_start',
        description: `Iteration ${record.iteration} started`,
        iteration: record.iteration,
      });

      if (record.endTime) {
        timeline.push({
          id: `${agentLoopId}:iteration:${record.iteration}:end`,
          timestamp: record.endTime,
          type: 'iteration_end',
          description: `Iteration ${record.iteration} completed (${record.endTime - record.startTime}ms)`,
          iteration: record.iteration,
          duration: record.endTime - record.startTime,
        });
      }
    }

    // Add error events
    for (const errorRecord of state.getErrorRecords()) {
      timeline.push({
        id: errorRecord.id,
        timestamp: errorRecord.timestamp,
        type: 'error',
        description: `Error: ${errorRecord.message}`,
        iteration: errorRecord.iteration,
        errorType: errorRecord.errorType,
        errorSeverity: errorRecord.severity,
      });
    }

    // Add interruption events
    for (const interruptRecord of state.getInterruptionHistory()) {
      const typeMap: Record<string, ExecutionTimelineEntryType> = {
        'PAUSE': 'interruption_pause',
        'STOP': 'interruption_stop',
      };

      const descriptionMap: Record<string, string> = {
        'PAUSE': 'paused',
        'STOP': 'stopped',
      };
      const typeDescription = descriptionMap[interruptRecord.type] || 'paused';

      timeline.push({
        id: interruptRecord.id,
        timestamp: interruptRecord.timestamp,
        type: (typeMap[interruptRecord.type] || 'interruption_pause') as ExecutionTimelineEntryType,
        description: `Execution ${typeDescription}: ${interruptRecord.reason}`,
        iteration: interruptRecord.iteration,
      });
    }

    // Add end event
    if (state.endTime) {
      const statusMap: Record<string, ExecutionTimelineEntryType> = {
        [AgentLoopStatus.COMPLETED]: 'execution_completed',
        [AgentLoopStatus.FAILED]: 'execution_failed',
        [AgentLoopStatus.CANCELLED]: 'execution_cancelled',
        [AgentLoopStatus.STOPPED]: 'execution_stopped',
      };
      timeline.push({
        id: `${agentLoopId}:end`,
        timestamp: state.endTime,
        type: (statusMap[entity.getStatus()] || 'execution_end') as ExecutionTimelineEntryType,
        description: `Agent loop execution ${entity.getStatus().toLowerCase()}`,
        duration: state.endTime - (state.startTime || 0),
      });
    }

    return timeline.sort((a, b) => a.timestamp - b.timestamp);
  }

  /**
   * Get variable history - track how a specific variable changed throughout execution
   *
   * @deprecated Use {@link AgentVariableResourceAPI} for variable management.
   * @param agentLoopId Agent loop ID
   * @param variableName Variable name to track
   * @returns Variable snapshots in chronological order
   */
  async getVariableHistory(agentLoopId: ID, variableName: string): Promise<VariableHistoryEntry[]> {
    const entity = await this.registry.get(agentLoopId);
    if (!entity) {
      return [];
    }

    const history: VariableHistoryEntry[] = [];
    const snapshots = entity.state.getVariableSnapshots();

    for (let i = 0; i < snapshots.length; i++) {
      const snapshot = snapshots[i];
      if (!snapshot) {
        continue;
      }

      if (!(variableName in snapshot.variables)) {
        continue;
      }

      const varData = snapshot.variables[variableName]!;
      const previousSnapshot = i > 0 ? snapshots[i - 1] : null;
      const previousVarData = previousSnapshot?.variables[variableName];

      history.push({
        timestamp: snapshot.timestamp,
        name: variableName,
        value: varData.value,
        iteration: snapshot.iteration,
        change: {
          from: previousVarData?.value,
          to: varData.value,
        },
      });
    }

    return history;
  }

  /**
   * Get context evolution - track how execution context evolved
   *
   * @deprecated Use {@link AgentExecutionStateAPI} for state tracking.
   * @param agentLoopId Agent loop ID
   * @returns Context snapshots at key points
   */
  async getContextEvolution(agentLoopId: ID): Promise<ContextEvolutionEntry[]> {
    const entity = await this.registry.get(agentLoopId);
    if (!entity) {
      return [];
    }

    const evolution: ContextEvolutionEntry[] = [];
    const state = entity.state;

    evolution.push({
      timestamp: state.startTime || 0,
      iteration: 0,
      status: AgentLoopStatus.RUNNING,
      description: 'Execution started',
    });

    for (const record of state.iterationHistory) {
      evolution.push({
        timestamp: record.startTime,
        iteration: record.iteration,
        status: AgentLoopStatus.RUNNING,
        description: `Iteration ${record.iteration} started, tool calls: ${record.toolCalls.length}`,
        toolCalls: record.toolCalls.length,
      });
    }

    if (state.endTime) {
      evolution.push({
        timestamp: state.endTime,
        iteration: state.currentIteration,
        status: entity.getStatus(),
        description: `Execution ${entity.getStatus().toLowerCase()}`,
      });
    }

    return evolution;
  }

  /**
   * Get execution statistics - aggregated metrics across all agent loops
   *
   * @deprecated Use {@link AgentExecutionRegistryAPI#getExecutionStatistics} instead.
   * @returns Statistics including success rate, avg duration, etc.
   */
  async getExecutionStatistics(): Promise<AgentExecutionStatistics> {
    const entities = await this.getAll();
    const now = Date.now();

    let totalDuration = 0;
    let completedCount = 0;
    let failedCount = 0;
    let cancelledCount = 0;
    let totalIterations = 0;
    let totalToolCalls = 0;

    for (const entity of entities) {
      const state = entity.state;
      const status = entity.getStatus();

      if (status === AgentLoopStatus.COMPLETED) {
        completedCount++;
      } else if (status === AgentLoopStatus.FAILED) {
        failedCount++;
      } else if (status === AgentLoopStatus.CANCELLED) {
        cancelledCount++;
      }

      if (state.startTime && state.endTime) {
        totalDuration += state.endTime - state.startTime;
      } else if (state.startTime && status === AgentLoopStatus.RUNNING) {
        totalDuration += now - state.startTime;
      }

      totalIterations += state.currentIteration;
      totalToolCalls += state.toolCallCount;
    }

    const total = entities.length;
    const avgDuration = completedCount > 0 ? Math.round(totalDuration / completedCount) : 0;
    const successRate = total > 0 ? (completedCount / total) * 100 : 0;

    return {
      total,
      completed: completedCount,
      failed: failedCount,
      cancelled: cancelledCount,
      successRate: Math.round(successRate * 100) / 100,
      avgDuration,
      totalIterations,
      avgIterationsPerExecution: total > 0 ? Math.round(totalIterations / total) : 0,
      totalToolCalls,
      avgToolCallsPerExecution: total > 0 ? Math.round(totalToolCalls / total) : 0,
    };
  }

  /**
   * Get execution path - track which iterations and branches were taken
   *
   * @deprecated Use {@link AgentLoopIterationAPI#analyzePaths} for enhanced path analysis.
   * @param agentLoopId Agent loop ID
   * @returns Execution path description
   */
  async getExecutionPath(agentLoopId: ID): Promise<ExecutionPath | null> {
    const entity = await this.registry.get(agentLoopId);
    if (!entity) {
      return null;
    }

    const state = entity.state;
    const iterations: ExecutionPathIteration[] = [];

    for (const record of state.iterationHistory) {
      iterations.push({
        iteration: record.iteration,
        toolCalls: record.toolCalls.map(tc => ({
          name: tc.name,
          status: tc.error ? 'failed' : (tc.result !== undefined ? 'completed' : 'pending'),
          startTime: tc.startTime,
          endTime: tc.endTime,
        })),
        duration: record.endTime ? record.endTime - record.startTime : undefined,
      });
    }

    return {
      executionId: agentLoopId,
      status: entity.getStatus(),
      totalIterations: state.currentIteration,
      iterations,
      totalDuration: state.endTime && state.startTime ? state.endTime - state.startTime : undefined,
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

// ============================================================================
// New Types for Execution History APIs (P0)
// ============================================================================

/**
 * Execution timeline entry types
 */
export type ExecutionTimelineEntryType =
  | 'execution_start'
  | 'execution_end'
  | 'execution_completed'
  | 'execution_failed'
  | 'execution_cancelled'
  | 'execution_stopped'
  | 'execution_timeout'
  | 'iteration_start'
  | 'iteration_end'
  | 'error'
  | 'interruption_pause'
  | 'interruption_resume'
  | 'interruption_stop'
  | 'interruption_timeout';

/**
 * Execution timeline entry
 */
export interface ExecutionTimelineEntry {
  /** Unique entry ID */
  id: string;
  /** Event timestamp */
  timestamp: number;
  /** Event type */
  type: ExecutionTimelineEntryType;
  /** Human-readable description */
  description: string;
  /** Iteration number (if applicable) */
  iteration?: number;
  /** Duration of event (if applicable) */
  duration?: number;
  /** Error type (if type is 'error') */
  errorType?: string;
  /** Error severity (if type is 'error') */
  errorSeverity?: string;
}

/**
 * Variable history entry
 */
export interface VariableHistoryEntry {
  /** Timestamp when variable changed */
  timestamp: number;
  /** Variable name */
  name: string;
  /** Variable value */
  value: unknown;
  /** Iteration number */
  iteration: number;
  /** How the variable changed */
  change: {
    from: unknown;
    to: unknown;
  };
}

/**
 * Context evolution entry
 */
export interface ContextEvolutionEntry {
  /** Timestamp of context change */
  timestamp: number;
  /** Current iteration */
  iteration: number;
  /** Agent loop status at this point */
  status: AgentLoopStatus;
  /** Description of the context change */
  description: string;
  /** Tool calls made (if applicable) */
  toolCalls?: number;
}

/**
 * Agent execution statistics
 */
export interface AgentExecutionStatistics {
  /** Total number of agent loops executed */
  total: number;
  /** Number of completed executions */
  completed: number;
  /** Number of failed executions */
  failed: number;
  /** Number of cancelled executions */
  cancelled: number;
  /** Success rate (completed / total * 100) */
  successRate: number;
  /** Average execution duration in milliseconds */
  avgDuration: number;
  /** Total iterations across all executions */
  totalIterations: number;
  /** Average iterations per execution */
  avgIterationsPerExecution: number;
  /** Total tool calls across all executions */
  totalToolCalls: number;
  /** Average tool calls per execution */
  avgToolCallsPerExecution: number;
}

/**
 * Tool call in execution path
 */
export interface ToolCallInPath {
  /** Tool name */
  name: string;
  /** Tool call status */
  status: string;
  /** Start time */
  startTime: number;
  /** End time (if completed) */
  endTime?: number;
}

/**
 * Iteration in execution path
 */
export interface ExecutionPathIteration {
  /** Iteration number */
  iteration: number;
  /** Tool calls made in this iteration */
  toolCalls: ToolCallInPath[];
  /** Duration of iteration in milliseconds */
  duration?: number;
}

/**
 * Execution path - complete execution flow
 */
export interface ExecutionPath {
  /** Execution ID */
  executionId: ID;
  /** Final status */
  status: AgentLoopStatus;
  /** Total iterations */
  totalIterations: number;
  /** Iteration-by-iteration breakdown */
  iterations: ExecutionPathIteration[];
  /** Total execution duration */
  totalDuration?: number;
}
