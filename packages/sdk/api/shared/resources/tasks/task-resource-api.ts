/**
 * TaskResourceAPI - Task Resource Management API
 * Provides query and management for execution tasks
 *
 * Responsibilities:
 * - Query tasks by ID, status, execution ID
 * - Manage task lifecycle (cancel, cleanup)
 * - Retrieve task statistics
 * - Wraps TaskRegistry from the SDK core layer
 */

import { SimplifiedCrudResourceAPI } from "../generic-resource-api.js";
import type { TaskStatus, TaskInfo } from "../../../../shared/types/index.js";
import type { APIDependencyManager } from "../../core/sdk-dependencies.js";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";

const logger = createContextualLogger({ operation: "TaskResourceAPI" });

/**
 * Task filter options
 */
export interface TaskFilter {
  /** Task ID list */
  ids?: string[];
  /** Task status filter */
  status?: TaskStatus;
  /** Execution ID */
  executionId?: string;
  /** Instance ID (Agent or WorkflowExecution) */
  instanceId?: string;
}

/**
 * Task summary information
 */
export interface TaskSummary {
  /** Task ID */
  id: string;
  /** Instance type */
  instanceType: string;
  /** Task status */
  status: TaskStatus;
  /** Submission time */
  submitTime: number;
  /** Start time */
  startTime?: number;
  /** Completion time */
  completeTime?: number;
  /** Timeout (ms) */
  timeout?: number;
}

/**
 * Task statistics
 */
export interface TaskStats {
  /** Total number of tasks */
  total: number;
  /** Number of queued tasks */
  queued: number;
  /** Number of running tasks */
  running: number;
  /** Number of completed tasks */
  completed: number;
  /** Number of failed tasks */
  failed: number;
  /** Number of cancelled tasks */
  cancelled: number;
  /** Number of timed out tasks */
  timeout: number;
}

/**
 * TaskResourceAPI - Task Resource Management API
 */
export class TaskResourceAPI extends SimplifiedCrudResourceAPI<TaskInfo, string, TaskFilter> {
  private registry: import("../../../../shared/registry/task-registry.js").TaskRegistry;

  constructor(deps: APIDependencyManager) {
    super();
    this.registry = deps.getTaskRegistry();
    logger.info("TaskResourceAPI initialized");
  }

  // ============================================================================
  // Implement abstract methods
  // ============================================================================

  /**
   * Get a single task by ID
   * @param id Task ID
   * @returns TaskInfo or null if not found
   */
  protected async getResource(id: string): Promise<TaskInfo | null> {
    return this.registry.get(id);
  }

  /**
   * Get all tasks with loaded instances
   * @returns Array of TaskInfo
   */
  protected async getAllResources(): Promise<TaskInfo[]> {
    return this.registry.getAll();
  }

  /**
   * Create a task - not supported (tasks are created during execution)
   */
  protected async createResource(_resource: TaskInfo): Promise<void> {
    throw new Error(
      "Task creation via API is not supported. Tasks are created during workflow/agent execution.",
    );
  }

  /**
   * Update a task - not supported (task status is managed by the execution engine)
   */
  protected async updateResource(_id: string, _updates: Partial<TaskInfo>): Promise<void> {
    throw new Error(
      "Task update via API is not supported. Task status is managed by the execution engine.",
    );
  }

  /**
   * Delete a task
   * @param id Task ID
   */
  protected async deleteResource(id: string): Promise<void> {
    await this.registry.delete(id);
    logger.info("Task deleted", { taskId: id });
  }

  /**
   * Apply filter conditions to tasks
   */
  protected override applyFilter(tasks: TaskInfo[], filter: TaskFilter): TaskInfo[] {
    return tasks.filter(task => {
      if (filter.ids && !filter.ids.includes(task.id)) {
        return false;
      }
      if (filter.status && task.status !== filter.status) {
        return false;
      }
      if (filter.executionId && task.instance.id !== filter.executionId) {
        return false;
      }
      if (filter.instanceId && task.instance.id !== filter.instanceId) {
        return false;
      }
      return true;
    });
  }

  /**
   * Clear all tasks
   */
  protected override async clearResources(): Promise<void> {
    await this.registry.clear();
    logger.info("All tasks cleared");
  }

  // ============================================================================
  // Task-specific methods
  // ============================================================================

  /**
   * Get tasks by status
   * @param status Task status to filter by
   * @returns Array of tasks with the given status
   */
  async getByStatus(status: TaskStatus): Promise<TaskInfo[]> {
    return this.registry.getByStatus(status);
  }

  /**
   * Get a task by execution ID
   * @param executionId Execution ID
   * @returns TaskInfo or null if not found
   */
  async getByExecutionId(executionId: string): Promise<TaskInfo | null> {
    return this.registry.getByExecutionId(executionId);
  }

  /**
   * Get a task by instance ID (Agent or WorkflowExecution)
   * @param instanceId Instance ID
   * @returns TaskInfo or null if not found
   */
  async getByInstanceId(instanceId: string): Promise<TaskInfo | null> {
    return this.registry.getByInstanceId(instanceId);
  }

  /**
   * Cancel a task
   * @param taskId Task ID to cancel
   * @returns Whether the cancellation was successful
   */
  async cancelTask(taskId: string): Promise<boolean> {
    const result = await this.registry.cancelTask(taskId);
    if (result) {
      logger.info("Task cancelled", { taskId });
    }
    return result;
  }

  /**
   * Get task statistics
   * @returns Task statistics
   */
  async getTaskStats(): Promise<{
    total: number;
    queued: number;
    running: number;
    completed: number;
    failed: number;
    cancelled: number;
    timeout: number;
  }> {
    return this.registry.getStats();
  }

  /**
   * Clean up completed/failed/cancelled/timeout tasks
   * @param retentionTime Retention time in milliseconds (default: 1 hour)
   * @returns Number of tasks cleaned up
   */
  async cleanupTasks(retentionTime: number = 60 * 60 * 1000): Promise<number> {
    const count = await this.registry.cleanup(retentionTime);
    if (count > 0) {
      logger.info("Tasks cleaned up", { count, retentionTime });
    }
    return count;
  }

  /**
   * Check if a task exists
   * @param taskId Task ID
   * @returns Whether the task exists
   */
  async exists(taskId: string): Promise<boolean> {
    return this.registry.has(taskId);
  }
}
