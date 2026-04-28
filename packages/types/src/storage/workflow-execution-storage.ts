/**
 * Workflow Execution Storage Type Definitions
 * Define metadata, query options related to workflow execution persistent storage
 */

import type { ID, Timestamp, Version } from "../common.js";
import type { WorkflowExecutionStatus, WorkflowExecutionType } from "../workflow-execution/status.js";

/**
 * Workflow Execution Storage Metadata
 * Metadata information for indexing and querying
 */
export interface WorkflowExecutionStorageMetadata {
  /** Workflow Execution ID */
  executionId: ID;
  /** Workflow ID */
  workflowId: ID;
  /** Workflow version */
  workflowVersion: Version;
  /** execution state */
  status: WorkflowExecutionStatus;
  /** Execution type */
  executionType?: WorkflowExecutionType;
  /** Current Node ID */
  currentNodeId?: ID;
  /** Parent execution ID (child execution scenario) */
  parentExecutionId?: ID;
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
 * Workflow Execution List Query Options
 * Support for multi-dimensional filtering and paging
 */
export interface WorkflowExecutionListOptions {
  /** Filter by Workflow ID */
  workflowId?: ID;
  /** Filter by execution state (supports single or multiple states) */
  status?: WorkflowExecutionStatus | WorkflowExecutionStatus[];
  /** Filter by execution type */
  executionType?: WorkflowExecutionType;
  /** Filter by Parent Execution ID */
  parentExecutionId?: ID;
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
 * Workflow Execution Information (with ID and metadata)
 * For list presentation and statistics
 */
export interface WorkflowExecutionInfo {
  /** Workflow Execution ID */
  executionId: string;
  /** Execution metadata */
  metadata: WorkflowExecutionStorageMetadata;
}

/**
 * Workflow Execution Statistics
 */
export interface WorkflowExecutionStats {
  /** aggregate */
  total: number;
  /** Number of states */
  byStatus: Record<WorkflowExecutionStatus, number>;
  /** Number of each type */
  byType: Record<WorkflowExecutionType, number>;
  /** Statistics grouped by workflow */
  byWorkflow: Record<ID, number>;
}
