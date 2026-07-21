/**
 * WorkflowExecutionRegistryAPI - Workflow Execution Management API
 * Encapsulates WorkflowExecutionRegistry to provide workflow execution query and management.
 * Refactored version: Inherit GenericResourceAPI to improve code reusability and consistency.
 */

import type {
  WorkflowExecution,
  WorkflowExecutionResult,
  WorkflowExecutionStatus,
} from "@wf-agent/types";
import { SimplifiedCrudResourceAPI } from "../../shared/resources/generic-resource-api.js";
import type { APIDependencyManager } from "../../shared/core/sdk-dependencies.js";
import type { Timestamp } from "@wf-agent/types";

/**
 * Workflow Execution Filter
 */
export interface WorkflowExecutionFilter {
  /** Execution ID List */
  ids?: string[];
  /** Workflow ID */
  workflowId?: string;
  /** Execution state */
  status?: WorkflowExecutionStatus;
  /** Execution type */
  executionType?: "MAIN" | "FORK_JOIN" | "TRIGGERED_SUBWORKFLOW";
  /** Creation timeframe */
  createdAtRange?: { start?: Timestamp; end?: Timestamp };
  /** Updated timeframe */
  updatedAtRange?: { start?: Timestamp; end?: Timestamp };
}

/**
 * Workflow Execution Summary
 */
export interface WorkflowExecutionSummary {
  /** Execution ID */
  id: string;
  /** Workflow ID */
  workflowId: string;
  /** Workflow name */
  workflowName: string;
  /** Execution state */
  status: WorkflowExecutionStatus;
  /** Execution type */
  executionType?: "MAIN" | "FORK_JOIN" | "TRIGGERED_SUBWORKFLOW" | "SUBGRAPH";
  /** Current Node ID */
  currentNodeId: string;
  /** Starting time */
  startTime: Timestamp;
  /** End time */
  endTime?: Timestamp;
  /** Execution time (milliseconds) */
  executionTime?: number;
  /** Number of errors */
  errorCount: number;
}

/**
 * WorkflowExecutionRegistryAPI - Workflow Execution Management API
 *
 * Refactoring Notes:
 * - Inherits GenericResourceAPI to reuse generic CRUD operations.
 * - Implement all abstract methods to adapt to WorkflowExecutionRegistry.
 * - Add enhancements such as caching, logging, validation, etc.
 */
export class WorkflowExecutionRegistryAPI extends SimplifiedCrudResourceAPI<
  WorkflowExecution,
  string,
  WorkflowExecutionFilter
> {
  private dependencies: APIDependencyManager;

  /**
   * Create a WorkflowExecutionRegistryAPI instance
   * @param dependencies API dependencies
   */
  constructor(dependencies: APIDependencyManager) {
    super();
    this.dependencies = dependencies;
  }

  /**
   * Get a single workflow execution
   * @param id execution id
   * @returns WorkflowExecution instance, or null if it doesn't exist.
   */
  protected async getResource(id: string): Promise<WorkflowExecution | null> {
    const executionEntity = this.dependencies.getWorkflowExecutionRegistry().get(id);
    if (!executionEntity) {
      return null;
    }
    return executionEntity.getWorkflowExecutionData();
  }

  /**
   * Get all workflow executions
   * @returns Array of workflow execution instances
   */
  protected async getAllResources(): Promise<WorkflowExecution[]> {
    return this.dependencies
      .getWorkflowExecutionRegistry()
      .getAll()
      .map(executionEntity => executionEntity.getWorkflowExecutionData());
  }

  /**
   * Create workflow execution - Not supported, executions are created via WorkflowExecutionEntity
   * @param resource WorkflowExecution resource
   */
  protected async createResource(_resource: WorkflowExecution): Promise<void> {
    throw new Error(
      "Workflow execution creation via API is not supported. Executions are created through WorkflowExecutionEntity.",
    );
  }

  /**
   * Update workflow execution - Not supported, execution state is managed via WorkflowExecutionEntity
   * @param id Execution ID
   * @param updates Partial updates
   */
  protected async updateResource(_id: string, _updates: Partial<WorkflowExecution>): Promise<void> {
    throw new Error(
      "Workflow execution update via API is not supported. Execution state is managed through WorkflowExecutionEntity.",
    );
  }

  /**
   * Deleting workflow executions
   * First cleanup the entity resources, then delete from the registry.
   * @param id execution id
   */
  protected async deleteResource(id: string): Promise<void> {
    const registry = this.dependencies.getWorkflowExecutionRegistry();
    const entity = registry.get(id);
    if (entity && typeof entity.cleanup === "function") {
      entity.cleanup();
    }
    registry.delete(id);
  }

  /**
   * Applying Filter Criteria
   * @param resources array of workflow executions
   * @param filter Filtering conditions
   * @returns Filtered workflow execution array
   */
  protected override applyFilter(
    resources: WorkflowExecution[],
    filter: WorkflowExecutionFilter,
  ): WorkflowExecution[] {
    return resources.filter(execution => {
      if (filter.ids && !filter.ids.some(id => execution.id.includes(id))) {
        return false;
      }
      if (filter.workflowId && execution.workflowId !== filter.workflowId) {
        return false;
      }
      return true;
    });
  }

  /**
   * Get a list of workflow execution summaries
   * @param filter filter criteria
   * @returns array of workflow execution summaries
   */
  async getExecutionSummaries(
    filter?: WorkflowExecutionFilter,
  ): Promise<WorkflowExecutionSummary[]> {
    const executionEntities = this.dependencies.getWorkflowExecutionRegistry().getAll();

    // Apply filtering if filter is provided
    let filteredEntities = executionEntities;
    if (filter) {
      const executions = executionEntities.map(entity => entity.getWorkflowExecutionData());
      const filteredExecutions = this.applyFilter(executions, filter);

      // Map back to entities
      filteredEntities = executionEntities.filter(entity =>
        filteredExecutions.some(exec => exec.id === entity.getExecutionId()),
      );
    }

    return filteredEntities.map(executionEntity => {
      const execution = executionEntity.getWorkflowExecutionData();
      const startTime = executionEntity.getStartTime();
      const endTime = executionEntity.getEndTime();

      return {
        id: execution.id,
        workflowId: execution.workflowId,
        workflowName: "",
        status: executionEntity.getStatus(),
        executionType: execution.executionType,
        currentNodeId: execution.currentNodeId,
        startTime: startTime || 0,
        endTime: endTime || undefined,
        executionTime: startTime && endTime ? endTime - startTime : undefined,
        errorCount: execution.errors.length,
      };
    });
  }

  /**
   * Get workflow execution status
   * @param executionId executionId
   * @returns execution status, or null if it does not exist
   */
  async getExecutionStatus(executionId: string): Promise<WorkflowExecutionStatus | null> {
    const executionEntity = this.dependencies.getWorkflowExecutionRegistry().get(executionId);
    if (!executionEntity) {
      return null;
    }
    return executionEntity.getStatus();
  }

  /**
   * Get the workflow execution result
   * @param executionId executionId
   * @returns The result of the workflow execution, or null if it doesn't exist or hasn't completed.
   */
  async getExecutionResult(executionId: string): Promise<WorkflowExecutionResult | null> {
    const executionEntity = this.dependencies.getWorkflowExecutionRegistry().get(executionId);
    if (!executionEntity) {
      return null;
    }

    const execution = executionEntity.getWorkflowExecutionData();
    const status = executionEntity.getStatus();
    const startTime = executionEntity.getStartTime();
    const endTime = executionEntity.getEndTime();

    // Only completed, failed or canceled executions have results
    if (status !== "COMPLETED" && status !== "FAILED" && status !== "CANCELLED") {
      return null;
    }

    return {
      executionId: execution.id,
      output: execution.output,
      executionTime: startTime && endTime ? endTime - startTime : 0,
      nodeResults: execution.nodeResults,
      metadata: {
        status,
        startTime: startTime || 0,
        endTime: endTime || 0,
        executionTime: startTime && endTime ? endTime - startTime : 0,
        nodeCount: execution.nodeResults.length,
        errorCount: execution.errors.length,
      },
    };
  }

  // ============================================================================
  // State Query Methods
  // ============================================================================

  /**
   * Get running workflow executions
   * @returns Array of running workflow executions
   */
  async getRunningExecutions(): Promise<WorkflowExecution[]> {
    return this.getExecutionsByStatus("RUNNING");
  }

  /**
   * Get paused workflow executions
   * @returns Array of paused workflow executions
   */
  async getPausedExecutions(): Promise<WorkflowExecution[]> {
    return this.getExecutionsByStatus("PAUSED");
  }

  /**
   * Get completed workflow executions
   * @returns Array of completed workflow executions
   */
  async getCompletedExecutions(): Promise<WorkflowExecution[]> {
    return this.getExecutionsByStatus("COMPLETED");
  }

  /**
   * Get failed workflow executions
   * @returns Array of failed workflow executions
   */
  async getFailedExecutions(): Promise<WorkflowExecution[]> {
    return this.getExecutionsByStatus("FAILED");
  }

  /**
   * Get workflow executions by status
   * @param status Status to filter by
   * @returns Array of matching workflow executions
   */
  private getExecutionsByStatus(status: WorkflowExecutionStatus): WorkflowExecution[] {
    const registry = this.dependencies.getWorkflowExecutionRegistry();
    return registry
      .getAll()
      .filter((entity) => entity.getStatus() === status)
      .map((entity) => entity.getWorkflowExecutionData());
  }

  /**
   * Getting Workflow Execution Statistics
   * @returns Statistics
   */
  async getExecutionStatistics(): Promise<{
    total: number;
    byStatus: Record<WorkflowExecutionStatus, number>;
    byWorkflow: Record<string, number>;
  }> {
    const executionEntities = this.dependencies.getWorkflowExecutionRegistry().getAll();
    const byStatus: Record<WorkflowExecutionStatus, number> = {
      CREATED: 0,
      RUNNING: 0,
      PAUSED: 0,
      COMPLETED: 0,
      FAILED: 0,
      CANCELLED: 0,
      STOPPED: 0,
    };
    const byWorkflow: Record<string, number> = {};

    for (const executionEntity of executionEntities) {
      const execution = executionEntity.getWorkflowExecutionData();
      const status = executionEntity.getStatus();

      // Statistics by Status
      byStatus[status]++;

      // Statistics by workflow
      const workflowId = execution.workflowId;
      byWorkflow[workflowId] = (byWorkflow[workflowId] || 0) + 1;
    }

    return {
      total: executionEntities.length,
      byStatus,
      byWorkflow,
    };
  }

  /**
   * Cleanup completed/failed/cancelled workflow executions
   * Delegates to WorkflowExecutionRegistry.cleanupTerminated()
   * @returns Number of instances cleaned up
   */
  async cleanupCompletedExecutions(): Promise<number> {
    return this.dependencies.getWorkflowExecutionRegistry().cleanupTerminated();
  }

  /**
   * Get the underlying WorkflowExecutionRegistry instance
   * @returns WorkflowExecutionRegistry instance
   */
  getRegistry() {
    return this.dependencies.getWorkflowExecutionRegistry();
  }

  // ============================================================================
  // Timeline & Execution Path Queries
  // ============================================================================

  /**
   * Get execution timeline for a workflow execution
   *
   * Builds a chronological timeline of node execution events including
   * start, completion, failures, and cancellations.
   *
   * @param executionId - Workflow execution ID
   * @returns Array of timeline entries sorted by timestamp
   */
  async getExecutionTimeline(executionId: string): Promise<WorkflowExecutionTimelineEntry[]> {
    const registry = this.dependencies.getWorkflowExecutionRegistry();
    const entity = registry.get(executionId);
    if (!entity) {
      return [];
    }

    const timeline: WorkflowExecutionTimelineEntry[] = [];
    const startTime = entity.getStartTime();
    const endTime = entity.getEndTime();
    const nodeResults = entity.getNodeResults();

    // Add start event
    if (startTime) {
      timeline.push({
        id: `${executionId}:start`,
        timestamp: startTime,
        type: 'execution_start',
        description: 'Workflow execution started',
      });
    }

    // Add node execution events
    for (const result of nodeResults) {
      if (result.startTime) {
        timeline.push({
          id: `${executionId}:node:${result.nodeId}:start`,
          timestamp: result.startTime,
          type: 'node_start',
          description: `Node ${result.nodeId} (${result.nodeType}) started`,
          nodeId: result.nodeId,
          nodeType: result.nodeType,
        });
      }

      if (result.endTime) {
        const statusType = result.status === 'COMPLETED' ? 'node_completed'
          : result.status === 'FAILED' ? 'node_failed'
          : result.status === 'SKIPPED' ? 'node_skipped'
          : result.status === 'CANCELLED' ? 'node_cancelled'
          : 'node_end';

        timeline.push({
          id: `${executionId}:node:${result.nodeId}:end`,
          timestamp: result.endTime,
          type: statusType as WorkflowExecutionTimelineEntryType,
          description: `Node ${result.nodeId} ${result.status.toLowerCase()} (${result.executionTime}ms)`,
          nodeId: result.nodeId,
          nodeType: result.nodeType,
          duration: result.executionTime,
        });
      }
    }

    // Add end event
    if (endTime) {
      const statusType = entity.getStatus() === 'COMPLETED' ? 'execution_completed'
        : entity.getStatus() === 'FAILED' ? 'execution_failed'
        : entity.getStatus() === 'CANCELLED' ? 'execution_cancelled'
        : 'execution_end';

      timeline.push({
        id: `${executionId}:end`,
        timestamp: endTime,
        type: statusType as WorkflowExecutionTimelineEntryType,
        description: `Workflow execution ${entity.getStatus().toLowerCase()}`,
        duration: endTime - (startTime || 0),
      });
    }

    return timeline.sort((a, b) => a.timestamp - b.timestamp);
  }

  /**
   * Get execution path for a workflow execution
   *
   * Builds the ordered execution path showing which nodes were executed,
   * their status, and timing information.
   *
   * @param executionId - Workflow execution ID
   * @returns Execution path summary, or null if execution not found
   */
  async getExecutionPath(executionId: string): Promise<WorkflowExecutionPath | null> {
    const registry = this.dependencies.getWorkflowExecutionRegistry();
    const entity = registry.get(executionId);
    if (!entity) {
      return null;
    }

    const nodeResults = entity.getNodeResults();
    const startTime = entity.getStartTime();
    const endTime = entity.getEndTime();

    const pathSteps: WorkflowExecutionPathStep[] = nodeResults.map(result => ({
      nodeId: result.nodeId,
      nodeType: result.nodeType,
      status: result.status,
      step: result.step,
      startTime: result.startTime,
      endTime: result.endTime,
      executionTime: result.executionTime,
      error: result.error ?? undefined,
    }));

    return {
      executionId,
      status: entity.getStatus(),
      totalNodes: nodeResults.length,
      steps: pathSteps,
      totalDuration: endTime && startTime ? endTime - startTime : undefined,
    };
  }
}

/**
 * Workflow execution timeline entry type
 */
export type WorkflowExecutionTimelineEntryType =
  | 'execution_start'
  | 'execution_completed'
  | 'execution_failed'
  | 'execution_cancelled'
  | 'execution_end'
  | 'node_start'
  | 'node_completed'
  | 'node_failed'
  | 'node_skipped'
  | 'node_cancelled'
  | 'node_end';

/**
 * Workflow execution timeline entry
 */
export interface WorkflowExecutionTimelineEntry {
  /** Unique entry ID */
  id: string;
  /** Event timestamp */
  timestamp: number;
  /** Event type */
  type: WorkflowExecutionTimelineEntryType;
  /** Human-readable description */
  description: string;
  /** Node ID (if applicable) */
  nodeId?: string;
  /** Node type (if applicable) */
  nodeType?: string;
  /** Duration of event (if applicable) */
  duration?: number;
}

/**
 * Workflow execution path step
 */
export interface WorkflowExecutionPathStep {
  /** Node ID */
  nodeId: string;
  /** Node type */
  nodeType: string;
  /** Execution status */
  status: string;
  /** Execution step number */
  step: number;
  /** Start time */
  startTime?: number;
  /** End time */
  endTime?: number;
  /** Execution duration (ms) */
  executionTime?: number;
  /** Error information (if failed) */
  error?: unknown;
}

/**
 * Workflow execution path summary
 */
export interface WorkflowExecutionPath {
  /** Execution ID */
  executionId: string;
  /** Execution status */
  status: string;
  /** Total number of nodes executed */
  totalNodes: number;
  /** Ordered list of execution steps */
  steps: WorkflowExecutionPathStep[];
  /** Total duration (ms) */
  totalDuration?: number;
}
