/**
 * ThreadRegistryAPI - Thread Management API
 * Encapsulate ThreadRegistry to provide thread query and management.
 * Refactored version: Inherit GenericResourceAPI to improve code reusability and consistency.
 */

import type { ThreadRegistry } from "../../../../workflow/stores/thread-registry.js";
import type { Thread, WorkflowExecutionResult, WorkflowExecutionStatus } from "@wf-agent/types";
import { CrudResourceAPI } from "../../../shared/resources/generic-resource-api.js";
import { getErrorMessage, isSuccess, getData } from "../../../shared/types/execution-result.js";
import type { APIDependencyManager } from "../../../shared/core/sdk-dependencies.js";
import type { Timestamp } from "@wf-agent/types";
// SingletonRegistry has been removed and is no longer needed for this import!

/**
 * Thread Filter
 */
export interface WorkflowExecutionFilter {
  /** Thread ID List */
  ids?: string[];
  /** Workflow ID */
  workflowId?: string;
  /** thread state */
  status?: WorkflowExecutionStatus;
  /** Thread type */
  threadType?: "MAIN" | "FORK_JOIN" | "TRIGGERED_SUBWORKFLOW";
  /** Creation timeframe */
  createdAtRange?: { start?: Timestamp; end?: Timestamp };
  /** Updated timeframe */
  updatedAtRange?: { start?: Timestamp; end?: Timestamp };
}

/**
 * Thread Summary
 */
export interface WorkflowExecutionSummary {
  /** Thread ID */
  id: string;
  /** Workflow ID */
  workflowId: string;
  /** Workflow name */
  workflowName: string;
  /** thread state */
  status: WorkflowExecutionStatus;
  /** Thread type */
  threadType?: "MAIN" | "FORK_JOIN" | "TRIGGERED_SUBWORKFLOW";
  /** Current Node ID */
  currentNodeId: string;
  /** Starting time */
  startTime: Timestamp;
  /** end time */
  endTime?: Timestamp;
  /** Execution time (milliseconds) */
  executionTime?: number;
  /** Number of errors */
  errorCount: number;
}

/**
 * ThreadRegistryAPI - Thread Management API
 * Use the global thread registry singleton by default
 *
 * Refactoring Notes:
 * - Inherits GenericResourceAPI to reuse generic CRUD operations.
 * - Implement all abstract methods to adapt to ThreadRegistry.
 * - Implement all abstract methods to adapt to ThreadRegistry.
 * - Add enhancements such as caching, logging, validation, etc.
 */
export class ThreadRegistryAPI extends CrudResourceAPI<Thread, string, WorkflowExecutionFilter> {
  private dependencies: APIDependencyManager;

  /**
   * Create a ThreadRegistryAPI instance
   * @param dependencies API dependencies
   */
  constructor(dependencies: APIDependencyManager) {
    super();
    this.dependencies = dependencies;
  }

  /**
   * Get a single thread
   * @param id thread id
   * @returns Thread instance, or null if it doesn't exist.
   */
  protected async getResource(id: string): Promise<Thread | null> {
    const threadEntity = this.dependencies.getThreadRegistry().get(id);
    if (!threadEntity) {
      return null;
    }
    return threadEntity.getThread();
  }

  /**
   * Get all threads
   * @returns Array of thread instances
   */
  protected async getAllResources(): Promise<Thread[]> {
    return this.dependencies
      .getThreadRegistry()
      .getAll()
      .map(threadEntity => threadEntity.getThread());
  }

  /**
   * Create thread - Not supported, threads are created via ThreadEntity
   * @param resource Thread resource
   */
  protected async createResource(_resource: Thread): Promise<void> {
    throw new Error(
      "Thread creation via API is not supported. Threads are created through ThreadEntity.",
    );
  }

  /**
   * Update thread - Not supported, thread state is managed via ThreadEntity
   * @param id Thread ID
   * @param updates Partial updates
   */
  protected async updateResource(_id: string, _updates: Partial<Thread>): Promise<void> {
    throw new Error(
      "Thread update via API is not supported. Thread state is managed through ThreadEntity.",
    );
  }

  /**
   * Deleting threads
   * @param id thread id
   */
  protected async deleteResource(id: string): Promise<void> {
    this.dependencies.getThreadRegistry().delete(id);
  }

  /**
   * Applying Filter Criteria
   * @param resources array of threads
   * @param filter Filtering conditions
   * @returns Filtered thread array
   */
  protected override applyFilter(resources: Thread[], filter: WorkflowExecutionFilter): Thread[] {
    return resources.filter(thread => {
      if (filter.ids && !filter.ids.some(id => thread.id.includes(id))) {
        return false;
      }
      if (filter.workflowId && thread.workflowId !== filter.workflowId) {
        return false;
      }
      // Note: status filtering requires ThreadEntity, skip for now
      // if (filter.status && thread.status !== filter.status) {
      //   return false;
      // }
      // Note: createdAtRange filtering requires ThreadEntity, skip for now
      // if (filter.createdAtRange) {
      //   if (filter.createdAtRange.start && thread.startTime < filter.createdAtRange.start) {
      //     return false;
      //   }
      //   if (filter.createdAtRange.end && thread.startTime > filter.createdAtRange.end) {
      //     return false;
      //   }
      // }
      return true;
    });
  }

  /**
   * Get a list of thread summaries
   * @param filter filter criteria
   * @returns array of thread summaries
   */
  async getThreadSummaries(filter?: WorkflowExecutionFilter): Promise<WorkflowExecutionSummary[]> {
    const threadEntities = this.dependencies.getThreadRegistry().getAll();

    return threadEntities.map(threadEntity => {
      const thread = threadEntity.getThread();
      const startTime = threadEntity.getStartTime();
      const endTime = threadEntity.getEndTime();

      return {
        id: thread.id,
        workflowId: thread.workflowId,
        workflowName: "", // TODO: Get workflow name from workflow registry
        status: threadEntity.getStatus(),
        threadType: thread.threadType,
        currentNodeId: thread.currentNodeId,
        startTime: startTime || 0,
        endTime: endTime || undefined,
        executionTime: startTime && endTime ? endTime - startTime : undefined,
        errorCount: thread.errors.length,
      };
    });
  }

  /**
   * Get thread status
   * @param threadId threadId
   * @returns thread status, or null if it does not exist
   */
  async getThreadStatus(threadId: string): Promise<WorkflowExecutionStatus | null> {
    const threadEntity = this.dependencies.getThreadRegistry().get(threadId);
    if (!threadEntity) {
      return null;
    }
    return threadEntity.getStatus();
  }

  /**
   * Get the thread execution result
   * @param threadId threadId
   * @returns The result of the thread execution, or null if it doesn't exist or hasn't completed.
   */
  async getThreadResult(threadId: string): Promise<WorkflowExecutionResult | null> {
    const threadEntity = this.dependencies.getThreadRegistry().get(threadId);
    if (!threadEntity) {
      return null;
    }

    const thread = threadEntity.getThread();
    const status = threadEntity.getStatus();
    const startTime = threadEntity.getStartTime();
    const endTime = threadEntity.getEndTime();

    // Only completed, failed or canceled threads have results
    if (status !== "COMPLETED" && status !== "FAILED" && status !== "CANCELLED") {
      return null;
    }

    return {
      threadId: thread.id,
      output: thread.output,
      executionTime: startTime && endTime ? endTime - startTime : 0,
      nodeResults: thread.nodeResults,
      metadata: {
        status,
        startTime: startTime || 0,
        endTime: endTime || 0,
        executionTime: startTime && endTime ? endTime - startTime : 0,
        nodeCount: thread.nodeResults.length,
        errorCount: thread.errors.length,
      },
    };
  }

  /**
   * Getting Thread Statistics
   * @returns Statistics
   */
  async getThreadStatistics(): Promise<{
    total: number;
    byStatus: Record<WorkflowExecutionStatus, number>;
    byWorkflow: Record<string, number>;
  }> {
    const threadEntities = this.dependencies.getThreadRegistry().getAll();
    const byStatus: Record<WorkflowExecutionStatus, number> = {
      CREATED: 0,
      RUNNING: 0,
      PAUSED: 0,
      COMPLETED: 0,
      FAILED: 0,
      CANCELLED: 0,
      TIMEOUT: 0,
    };
    const byWorkflow: Record<string, number> = {};

    for (const threadEntity of threadEntities) {
      const thread = threadEntity.getThread();
      const status = threadEntity.getStatus();

      // Statistics by Status
      byStatus[status]++;

      // Statistics by workflow
      const workflowId = thread.workflowId;
      byWorkflow[workflowId] = (byWorkflow[workflowId] || 0) + 1;
    }

    return {
      total: threadEntities.length,
      byStatus,
      byWorkflow,
    };
  }

  /**
   * Get the underlying ThreadRegistry instance
   * @returns ThreadRegistry instance
   */
  getRegistry(): ThreadRegistry {
    return this.dependencies.getThreadRegistry();
  }
}
