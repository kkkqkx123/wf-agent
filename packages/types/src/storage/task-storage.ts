/**
 * Task Storage Type Definitions
 * Defining metadata, query options, and statistics related to task persistent storage
 */

import type { ID, Timestamp } from "../common.js";

/**
 * mission status
 */
export type TaskStatus =
  /** queuing up */
  | "QUEUED"
  /** running */
  | "RUNNING"
  /** done */
  | "COMPLETED"
  /** failed */
  | "FAILED"
  /** Cancelled */
  | "CANCELLED"
  /** overtime pay */
  | "TIMEOUT";

/**
 * Task Store Metadata
 * Metadata information for indexing and querying
 */
export interface TaskStorageMetadata {
  /** Task ID */
  taskId: ID;
  /** Execution ID */
  executionId: ID;
  /** Workflow ID */
  workflowId: ID;
  /** mission status */
  status: TaskStatus;
  /** Submission time */
  submitTime: Timestamp;
  /** Starting time */
  startTime?: Timestamp;
  /** Completion time */
  completeTime?: Timestamp;
  /** Timeout time (milliseconds) */
  timeout?: number;
  /** error message */
  error?: string;
  /** error stack */
  errorStack?: string;
  /** tagged array */
  tags?: string[];
  /** Customizing metadata fields */
  customFields?: Record<string, unknown>;
}

/**
 * Task list query options
 * Support for multi-dimensional filtering and paging
 */
export interface TaskListOptions {
  /** Filter by execution ID */
  executionId?: ID;
  /** Filter by Workflow ID */
  workflowId?: ID;
  /** Filter by task status (single or multiple statuses supported) */
  status?: TaskStatus | TaskStatus[];
  /** Submission timeframe - start */
  submitTimeFrom?: Timestamp;
  /** Submission timeframe - end */
  submitTimeTo?: Timestamp;
  /** Start time range - start */
  startTimeFrom?: Timestamp;
  /** Start time range - End */
  startTimeTo?: Timestamp;
  /** Completion timeframe - start */
  completeTimeFrom?: Timestamp;
  /** Completion timeframe - end */
  completeTimeTo?: Timestamp;
  /** Filter by tag */
  tags?: string[];
  /** Maximum number of returns (paged) */
  limit?: number;
  /** Offset (paging) */
  offset?: number;
  /** Sort Fields */
  sortBy?: "submitTime" | "startTime" | "completeTime";
  /** sorting direction */
  sortOrder?: "asc" | "desc";
}

/**
 * Task information (with ID and metadata)
 * For list presentation
 */
export interface TaskInfo {
  /** Task ID */
  taskId: string;
  /** Task metadata */
  metadata: TaskStorageMetadata;
}

/**
 * Task statistics options
 */
export interface TaskStatsOptions {
  /** Filter by Workflow ID */
  workflowId?: ID;
  /** Time range - start */
  timeFrom?: Timestamp;
  /** Timeframe - End */
  timeTo?: Timestamp;
}

/**
 * Statistical information on missions
 */
export interface TaskStats {
  /** aggregate */
  total: number;
  /** Number of states */
  byStatus: Record<TaskStatus, number>;
  /** Statistics grouped by workflow */
  byWorkflow: Record<ID, number>;
  /** Average execution time (milliseconds) */
  avgExecutionTime?: number;
  /** Maximum execution time (milliseconds) */
  maxExecutionTime?: number;
  /** Minimum execution time (milliseconds) */
  minExecutionTime?: number;
  /** success rate */
  successRate?: number;
  /** overtime rate */
  timeoutRate?: number;
}

/**
 * Task execution time statistics
 */
export interface TaskExecutionTimeStats {
  /** Average implementation time */
  avgTime: number;
  /** Maximum execution time */
  maxTime: number;
  /** Minimum execution time */
  minTime: number;
  /** Median implementation time */
  medianTime?: number;
  /** P95 execution time */
  p95Time?: number;
  /** P99 execution time */
  p99Time?: number;
}
