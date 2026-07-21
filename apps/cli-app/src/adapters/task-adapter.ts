/**
 * Task Adapter
 * Encapsulates SDK API calls related to task management
 */

import { BaseAdapter } from "./base-adapter.js";
import { CLINotFoundError } from "../types/cli-types.js";

/**
 * Task filter options
 */
export interface TaskFilter {
  ids?: string[];
  status?: string;
  executionId?: string;
  instanceId?: string;
}

/**
 * Task statistics
 */
export interface TaskStats {
  total: number;
  queued: number;
  running: number;
  completed: number;
  failed: number;
  cancelled: number;
  timeout: number;
}

/**
 * Task Adapter
 */
export class TaskAdapter extends BaseAdapter {
  /**
   * Get the task API from the SDK factory
   */
  private getTaskAPI() {
    return this.sdk.getFactory().createTaskAPI();
  }

  /**
   * List all tasks with optional filter
   */
  async listTasks(filter?: TaskFilter): Promise<any[]> {
    return this.executeWithErrorHandling(async () => {
      const api = this.getTaskAPI();
      if (filter) {
        return await api.getAll(filter as any);
      }
      return await api.getAll();
    }, "List tasks");
  }

  /**
   * Get task details by ID
   */
  async getTask(id: string): Promise<any> {
    return this.executeWithErrorHandling(async () => {
      const api = this.getTaskAPI();
      const task = await api.get(id);

      if (!task) {
        throw new CLINotFoundError(`Task not found: ${id}`, "Task", id);
      }

      return task;
    }, "Get task");
  }

  /**
   * Cancel a task
   */
  async cancelTask(taskId: string): Promise<boolean> {
    return this.executeWithErrorHandling(async () => {
      const api = this.getTaskAPI();
      const result = await api.cancelTask(taskId);
      if (result) {
        this.logOperation(`Task cancelled: ${taskId}`);
      } else {
        this.logOperationFailure(`Failed to cancel task: ${taskId}`);
      }
      return result;
    }, "Cancel task");
  }

  /**
   * Get task statistics
   */
  async getTaskStats(): Promise<TaskStats> {
    return this.executeWithErrorHandling(async () => {
      const api = this.getTaskAPI();
      return await api.getTaskStats();
    }, "Get task statistics");
  }

  /**
   * Clean up completed/failed/cancelled/timeout tasks
   * @param retentionTime Retention time in milliseconds (default: 1 hour)
   */
  async cleanupTasks(retentionTime?: number): Promise<number> {
    return this.executeWithErrorHandling(async () => {
      const api = this.getTaskAPI();
      const count = await api.cleanupTasks(retentionTime);
      this.logOperation(`Cleaned up ${count} tasks`);
      return count;
    }, "Clean up tasks");
  }
}