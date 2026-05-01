/**
 * Task Storage Adapter Interface Definition
 * Define a uniform interface for task persistence operations
 */

import type {
  TaskStorageMetadata,
  TaskListOptions,
  TaskStats,
  TaskStatsOptions,
} from "@wf-agent/types";
import type { BaseStorageAdapter } from "./base-storage-adapter.js";

/**
 * Task Store Adapter Interface
 *
 * Unified interface for defining task persistence operations
 * - Inherits from BaseStorageAdapter and provides standard CRUD operations.
 * - packages/storage provides an implementation of TaskStorageAdapter based on this interface.
 * - The application layer can use TaskStorageAdapter directly or implement this interface by itself.
 */
export interface TaskStorageAdapter extends BaseStorageAdapter<
  TaskStorageMetadata,
  TaskListOptions
> {
  /**
   * Getting Task Statistics
   * @param options Statistics options
   * @returns Task statistics
   */
  getTaskStats(options?: TaskStatsOptions): Promise<TaskStats>;

  /**
   * Cleaning up expired tasks
   * @param retentionTime retention time in milliseconds
   * @returns Number of tasks cleared
   */
  cleanupTasks(retentionTime: number): Promise<number>;
}
