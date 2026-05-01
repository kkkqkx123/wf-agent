/**
 * JSON File Task Storage Implementation
 * Task persistence storage based on JSON file system with metadata-data separation
 */

import * as path from "path";
import type {
  TaskStorageMetadata,
  TaskListOptions,
  TaskStats,
  TaskStatsOptions,
  TaskStatus,
} from "@wf-agent/types";
import type { TaskStorageAdapter } from "../types/adapter/task-adapter.js";
import { BaseJsonStorage, BaseJsonStorageConfig } from "./base-json-storage.js";

/**
 * JSON File Task Storage
 * Implements the TaskStorageAdapter interface
 */
export class JsonTaskStorage
  extends BaseJsonStorage<TaskStorageMetadata>
  implements TaskStorageAdapter
{
  constructor(config: BaseJsonStorageConfig) {
    super(config);
  }

  /**
   * Get metadata directory path for tasks
   */
  protected override getMetadataDir(): string {
    return path.join(this.config.baseDir, "metadata", "task");
  }

  /**
   * Get data directory path for tasks
   */
  protected override getDataDir(): string {
    return path.join(this.config.baseDir, "data", "task");
  }

  /**
   * List task IDs
   */
  async list(options?: TaskListOptions): Promise<string[]> {
    this.ensureInitialized();

    let ids = this.getAllIds();

    if (options) {
      ids = ids.filter(id => {
        const entry = this["metadataIndex"].get(id);
        if (!entry) return false;

        const metadata = entry.metadata;

        if (options.executionId && metadata.executionId !== options.executionId) {
          return false;
        }

        if (options.workflowId && metadata.workflowId !== options.workflowId) {
          return false;
        }

        if (options.status) {
          if (Array.isArray(options.status)) {
            if (!options.status.includes(metadata.status)) {
              return false;
            }
          } else if (metadata.status !== options.status) {
            return false;
          }
        }

        if (options.submitTimeFrom && metadata.submitTime < options.submitTimeFrom) {
          return false;
        }

        if (options.submitTimeTo && metadata.submitTime > options.submitTimeTo) {
          return false;
        }

        if (
          options.startTimeFrom &&
          (metadata.startTime === undefined || metadata.startTime < options.startTimeFrom)
        ) {
          return false;
        }

        if (
          options.startTimeTo &&
          (metadata.startTime === undefined || metadata.startTime > options.startTimeTo)
        ) {
          return false;
        }

        if (
          options.completeTimeFrom &&
          (metadata.completeTime === undefined || metadata.completeTime < options.completeTimeFrom)
        ) {
          return false;
        }

        if (
          options.completeTimeTo &&
          (metadata.completeTime === undefined || metadata.completeTime > options.completeTimeTo)
        ) {
          return false;
        }

        if (options.tags && options.tags.length > 0) {
          if (!metadata.tags || !options.tags.some(tag => metadata.tags!.includes(tag))) {
            return false;
          }
        }

        return true;
      });
    }

    const sortBy = options?.sortBy ?? "submitTime";
    const sortOrder = options?.sortOrder ?? "desc";

    ids.sort((a, b) => {
      const metaA = this["metadataIndex"].get(a)?.metadata;
      const metaB = this["metadataIndex"].get(b)?.metadata;

      let valueA: number;
      let valueB: number;

      switch (sortBy) {
        case "submitTime":
          valueA = metaA?.submitTime ?? 0;
          valueB = metaB?.submitTime ?? 0;
          break;
        case "startTime":
          valueA = metaA?.startTime ?? 0;
          valueB = metaB?.startTime ?? 0;
          break;
        case "completeTime":
        default:
          valueA = metaA?.completeTime ?? 0;
          valueB = metaB?.completeTime ?? 0;
          break;
      }

      return sortOrder === "asc" ? valueA - valueB : valueB - valueA;
    });

    const offset = options?.offset ?? 0;
    const limit = options?.limit ?? ids.length;

    return ids.slice(offset, offset + limit);
  }

  /**
   * Get task statistics
   * Only uses metadata index, no need to load data files
   */
  async getTaskStats(options?: TaskStatsOptions): Promise<TaskStats> {
    this.ensureInitialized();

    let entries = Array.from(this["metadataIndex"].values());

    // Apply filters
    if (options?.workflowId) {
      entries = entries.filter(e => e.metadata.workflowId === options.workflowId);
    }

    if (options?.timeFrom) {
      entries = entries.filter(e => e.metadata.submitTime >= options.timeFrom!);
    }

    if (options?.timeTo) {
      entries = entries.filter(e => e.metadata.submitTime <= options.timeTo!);
    }

    // Statistics
    const total = entries.length;
    const byStatus: Record<TaskStatus, number> = {
      QUEUED: 0,
      RUNNING: 0,
      COMPLETED: 0,
      FAILED: 0,
      CANCELLED: 0,
      TIMEOUT: 0,
    };

    const byWorkflow: Record<string, number> = {};
    const executionTimes: number[] = [];

    for (const entry of entries) {
      const meta = entry.metadata;

      // Status statistics
      byStatus[meta.status]++;

      // Workflow statistics
      if (!byWorkflow[meta.workflowId]) {
        byWorkflow[meta.workflowId] = 0;
      }
      byWorkflow[meta.workflowId]!++;

      // Execution time statistics
      if (meta.status === "COMPLETED" && meta.startTime && meta.completeTime) {
        executionTimes.push(meta.completeTime - meta.startTime);
      }
    }

    // Calculate execution time statistics
    let avgExecutionTime: number | undefined;
    let maxExecutionTime: number | undefined;
    let minExecutionTime: number | undefined;

    if (executionTimes.length > 0) {
      avgExecutionTime = executionTimes.reduce((a, b) => a + b, 0) / executionTimes.length;
      maxExecutionTime = Math.max(...executionTimes);
      minExecutionTime = Math.min(...executionTimes);
    }

    const completed = byStatus.COMPLETED;
    const timeoutCount = byStatus.TIMEOUT;

    return {
      total,
      byStatus,
      byWorkflow,
      avgExecutionTime,
      maxExecutionTime,
      minExecutionTime,
      successRate: total > 0 ? completed / total : undefined,
      timeoutRate: total > 0 ? timeoutCount / total : undefined,
    };
  }

  /**
   * Clean up expired tasks
   */
  async cleanupTasks(retentionTime: number): Promise<number> {
    this.ensureInitialized();

    const cutoffTime = Date.now() - retentionTime;
    let cleanedCount = 0;

    for (const [id, entry] of this["metadataIndex"]) {
      const meta = entry.metadata;
      if (
        (meta.status === "COMPLETED" ||
          meta.status === "FAILED" ||
          meta.status === "CANCELLED" ||
          meta.status === "TIMEOUT") &&
        meta.completeTime &&
        meta.completeTime < cutoffTime
      ) {
        await this.delete(id);
        cleanedCount++;
      }
    }

    return cleanedCount;
  }
}
