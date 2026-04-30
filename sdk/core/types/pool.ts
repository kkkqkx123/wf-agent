/**
 * Pool Types - Worker and Execution Pool Definitions
 *
 * Type definitions for execution pool and worker management.
 *
 * Design Principles:
 * - Simple and clear type definitions
 * - Support pool configuration and statistics
 * - Provide worker status tracking
 */

/**
 * Executor Status (using const enum for better performance)
 */
export const enum WorkerStatus {
  /** Free and available for new assignments */
  IDLE = "IDLE",
  /** Busy. On a mission. */
  BUSY = "BUSY",
  /** Closing... */
  SHUTTING_DOWN = "SHUTTING_DOWN",
}

/**
 * Queue Statistics Interface
 */
export interface QueueStats {
  /** Length of the queue to be executed */
  pendingCount: number;
  /** Number of tasks in progress */
  runningCount: number;
  /** Number of tasks completed */
  completedCount: number;
  /** Number of failed tasks */
  failedCount: number;
  /** Cancel the number of tasks */
  cancelledCount: number;
}

/**
 * Workflow Execution Pool Statistics Interface
 */
export interface PoolStats {
  /** Total number of executors */
  totalExecutors: number;
  /** Number of idle executors */
  idleExecutors: number;
  /** Number of busy executors */
  busyExecutors: number;
  /** Minimum number of executors */
  minExecutors: number;
  /** Maximum number of executors */
  maxExecutors: number;
}

/**
 * Executor Packaging Interface (for internal use)
 */
export interface ExecutorWrapper {
  /** Executor ID */
  executorId: string;
  /** Executor instance */
  executor: unknown;
  /** Executor Status */
  status: WorkerStatus;
  /** Current task ID (if in progress) */
  currentTaskId?: string;
  /** Last used time */
  lastUsedTime: number;
  /** Idle timeout timer */
  idleTimer?: NodeJS.Timeout;
}

/**
 * Execution Pool Configuration Interface
 *
 * Used to configure the behavior of ExecutionPool
 */
export interface ExecutionPoolConfig {
  /** Minimum number of executors */
  minExecutors?: number;
  /** Maximum number of executors */
  maxExecutors?: number;
  /** Idle timeout period (in milliseconds). After this time, the idle executor will be reclaimed. */
  idleTimeout?: number;
  /** Default timeout period (in milliseconds) */
  defaultTimeout?: number;
}


