/**
 * In-Memory Task Storage Adapter
 * Fast, isolated task storage for testing
 */

import type {
  TaskStorageMetadata,
  TaskListOptions,
  TaskStats,
  TaskStatsOptions,
} from "@wf-agent/types";
import type { TaskStorageAdapter } from "../types/adapter/task-adapter.js";
import { BaseMemoryStorage, type MemoryStorageConfig } from "./base-memory-storage.js";

/**
 * Memory-based task storage implementation
 * Implements TaskStorageAdapter interface with in-memory storage
 */
export class MemoryTaskStorage
  extends BaseMemoryStorage<TaskStorageMetadata, TaskListOptions>
  implements TaskStorageAdapter
{
  constructor(config: MemoryStorageConfig = {}) {
    super(config);
  }

  /**
   * List task IDs with filtering support
   */
  override async list(options?: TaskListOptions): Promise<string[]> {
    this.ensureInitialized();
    await this.simulateLatency();

    let ids = Array.from(this.store.keys());

    // Apply filters if provided
    if (options) {
      if (options.executionId) {
        ids = ids.filter(id => {
          const entry = this.store.get(id);
          return entry?.metadata.executionId === options.executionId;
        });
      }

      if (options.workflowId) {
        ids = ids.filter(id => {
          const entry = this.store.get(id);
          return entry?.metadata.workflowId === options.workflowId;
        });
      }

      if (options.status) {
        const statuses = Array.isArray(options.status) ? options.status : [options.status];
        ids = ids.filter(id => {
          const entry = this.store.get(id);
          return entry && statuses.includes(entry.metadata.status);
        });
      }

      if (options.submitTimeFrom) {
        ids = ids.filter(id => {
          const entry = this.store.get(id);
          return entry && entry.metadata.submitTime >= options.submitTimeFrom!;
        });
      }

      if (options.submitTimeTo) {
        ids = ids.filter(id => {
          const entry = this.store.get(id);
          return entry && entry.metadata.submitTime <= options.submitTimeTo!;
        });
      }

      if (options.startTimeFrom) {
        ids = ids.filter(id => {
          const entry = this.store.get(id);
          return entry && entry.metadata.startTime && entry.metadata.startTime >= options.startTimeFrom!;
        });
      }

      if (options.startTimeTo) {
        ids = ids.filter(id => {
          const entry = this.store.get(id);
          return entry && entry.metadata.startTime && entry.metadata.startTime <= options.startTimeTo!;
        });
      }

      if (options.completeTimeFrom) {
        ids = ids.filter(id => {
          const entry = this.store.get(id);
          return entry && entry.metadata.completeTime && entry.metadata.completeTime >= options.completeTimeFrom!;
        });
      }

      if (options.completeTimeTo) {
        ids = ids.filter(id => {
          const entry = this.store.get(id);
          return entry && entry.metadata.completeTime && entry.metadata.completeTime <= options.completeTimeTo!;
        });
      }

      if (options.tags && options.tags.length > 0) {
        ids = ids.filter(id => {
          const entry = this.store.get(id);
          const metadataTags = entry?.metadata.tags || [];
          return options.tags!.some(tag => metadataTags.includes(tag));
        });
      }
    }

    // Apply pagination
    if (options?.offset !== undefined || options?.limit !== undefined) {
      const offset = options.offset ?? 0;
      const limit = options.limit ?? ids.length;
      ids = ids.slice(offset, offset + limit);
    }

    return ids;
  }

  /**
   * Get task statistics
   */
  async getTaskStats(options?: TaskStatsOptions): Promise<TaskStats> {
    this.ensureInitialized();
    await this.simulateLatency();

    let entries = Array.from(this.store.values());

    // Apply filters if provided
    if (options) {
      if (options.workflowId) {
        entries = entries.filter(e => e.metadata.workflowId === options.workflowId);
      }

      if (options.timeFrom) {
        entries = entries.filter(e => e.metadata.submitTime >= options.timeFrom!);
      }

      if (options.timeTo) {
        entries = entries.filter(e => e.metadata.submitTime <= options.timeTo!);
      }
    }

    // Calculate statistics
    const byStatus: Record<string, number> = {};
    const byWorkflow: Record<string, number> = {};
    let totalExecutionTime = 0;
    let executionTimeCount = 0;
    let maxExecutionTime = 0;

    for (const entry of entries) {
      const status = entry.metadata.status;
      byStatus[status] = (byStatus[status] || 0) + 1;

      const workflowId = entry.metadata.workflowId;
      byWorkflow[workflowId] = (byWorkflow[workflowId] || 0) + 1;

      // Calculate execution time if available
      if (entry.metadata.startTime && entry.metadata.completeTime) {
        const execTime = entry.metadata.completeTime - entry.metadata.startTime;
        totalExecutionTime += execTime;
        executionTimeCount++;
        maxExecutionTime = Math.max(maxExecutionTime, execTime);
      }
    }

    const avgExecutionTime = executionTimeCount > 0 ? totalExecutionTime / executionTimeCount : undefined;

    return {
      total: entries.length,
      byStatus: byStatus as Record<any, number>,
      byWorkflow,
      avgExecutionTime,
      maxExecutionTime,
    };
  }

  /**
   * Cleanup expired tasks
   */
  async cleanupTasks(retentionTime: number): Promise<number> {
    this.ensureInitialized();
    await this.simulateLatency();

    const cutoffTime = Date.now() - retentionTime;
    const idsToDelete: string[] = [];

    for (const [id, entry] of this.store.entries()) {
      // Delete if completed/cancelled/failed and older than retention time
      const terminalStatuses = ["COMPLETED", "FAILED", "CANCELLED"];
      if (
        terminalStatuses.includes(entry.metadata.status) &&
        entry.metadata.completeTime &&
        entry.metadata.completeTime < cutoffTime
      ) {
        idsToDelete.push(id);
      }
    }

    // Delete expired tasks
    for (const id of idsToDelete) {
      this.store.delete(id);
    }

    return idsToDelete.length;
  }
}
