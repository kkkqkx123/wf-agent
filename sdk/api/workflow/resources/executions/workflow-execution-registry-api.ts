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
import { CrudResourceAPI } from "../../../shared/resources/generic-resource-api.js";
import type { APIDependencyManager } from "../../../shared/core/sdk-dependencies.js";
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
  executionType?: "MAIN" | "FORK_JOIN" | "TRIGGERED_SUBWORKFLOW";
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
export class WorkflowExecutionRegistryAPI extends CrudResourceAPI<
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
   * @param id execution id
   */
  protected async deleteResource(id: string): Promise<void> {
    this.dependencies.getWorkflowExecutionRegistry().delete(id);
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

    return executionEntities.map(executionEntity => {
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
      id: execution.id,
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
      TIMEOUT: 0,
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
   * Get the underlying WorkflowExecutionRegistry instance
   * @returns WorkflowExecutionRegistry instance
   */
  getRegistry() {
    return this.dependencies.getWorkflowExecutionRegistry();
  }
}
