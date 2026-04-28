/**
 * Threaded Storage Type Definitions
 * Define metadata, query options related to thread persistent storage
 */

import type { ID, Timestamp, Version } from "../common.js";
import type { ThreadStatus, ThreadType } from "../thread/status.js";

/**
 * Threads store metadata
 * Metadata information for indexing and querying
 */
export interface ThreadStorageMetadata {
  /** Thread ID */
  threadId: ID;
  /** Workflow ID */
  workflowId: ID;
  /** Workflow version */
  workflowVersion: Version;
  /** thread state */
  status: ThreadStatus;
  /** Thread type */
  threadType?: ThreadType;
  /** Current Node ID */
  currentNodeId?: ID;
  /** Parent thread ID (child thread scenario) */
  parentThreadId?: ID;
  /** Starting time */
  startTime: Timestamp;
  /** end time */
  endTime?: Timestamp;
  /** Tagged arrays (for categorization and retrieval) */
  tags?: string[];
  /** Customizing metadata fields */
  customFields?: Record<string, unknown>;
}

/**
 * Threaded list query options
 * Support for multi-dimensional filtering and paging
 */
export interface ThreadListOptions {
  /** Filter by Workflow ID */
  workflowId?: ID;
  /** Filter by thread state (supports single or multiple states) */
  status?: ThreadStatus | ThreadStatus[];
  /** Filter by thread type */
  threadType?: ThreadType;
  /** Filter by Parent Thread ID */
  parentThreadId?: ID;
  /** Start time range - start */
  startTimeFrom?: Timestamp;
  /** Start time range - End */
  startTimeTo?: Timestamp;
  /** End time range - start */
  endTimeFrom?: Timestamp;
  /** End Time Range - End */
  endTimeTo?: Timestamp;
  /** Filter by tag (match any tag) */
  tags?: string[];
  /** Maximum number of returns (paged) */
  limit?: number;
  /** Offset (paging) */
  offset?: number;
  /** Sort Fields */
  sortBy?: "startTime" | "endTime" | "updatedAt";
  /** sorting direction */
  sortOrder?: "asc" | "desc";
}

/**
 * Thread information (with ID and metadata)
 * For list presentation and statistics
 */
export interface ThreadInfo {
  /** Thread ID */
  threadId: string;
  /** Thread metadata */
  metadata: ThreadStorageMetadata;
}

/**
 * Thread Statistics
 */
export interface ThreadStats {
  /** aggregate */
  total: number;
  /** Number of states */
  byStatus: Record<ThreadStatus, number>;
  /** Number of each type */
  byType: Record<ThreadType, number>;
  /** Statistics grouped by workflow */
  byWorkflow: Record<ID, number>;
}
