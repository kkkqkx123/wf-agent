/**
 * Workflow Storage Type Definitions
 * Defining metadata, query options, and versioning related to workflow persistent storage
 */

import type { ID, Timestamp, Version } from "../common.js";

/**
 * Workflow Storage Metadata
 * Metadata information for indexing and querying
 */
export interface WorkflowStorageMetadata {
  /** Workflow ID */
  workflowId: ID;
  /** Workflow name */
  name: string;
  /** Workflow version */
  version: Version;
  /** descriptive */
  description?: string;
  /** author */
  author?: string;
  /** categorization */
  category?: string;
  /** tagged array */
  tags?: string[];
  /** Creation time */
  createdAt: Timestamp;
  /** update time */
  updatedAt: Timestamp;
  /** Number of nodes */
  nodeCount: number;
  /** algebraic quantity of a side */
  edgeCount: number;
  /** Enable or disable */
  enabled?: boolean;
  /** Customizing metadata fields */
  customFields?: Record<string, unknown>;
}

/**
 * Workflow list query options
 * Support for multi-dimensional filtering and paging
 */
export interface WorkflowListOptions {
  /** Fuzzy search by name */
  name?: string;
  /** Filter by Author */
  author?: string;
  /** Filter by Category */
  category?: string;
  /** Filter by tag (match any tag) */
  tags?: string[];
  /** Filter by Enabled Status */
  enabled?: boolean;
  /** Creation timeframe - start */
  createdAtFrom?: Timestamp;
  /** Creation Timeframe - End */
  createdAtTo?: Timestamp;
  /** Update timeframe - start */
  updatedAtFrom?: Timestamp;
  /** Update timeframe - end */
  updatedAtTo?: Timestamp;
  /** Maximum number of returns (paged) */
  limit?: number;
  /** Offset (paging) */
  offset?: number;
  /** Sort Fields */
  sortBy?: "name" | "createdAt" | "updatedAt";
  /** sorting direction */
  sortOrder?: "asc" | "desc";
}

/**
 * Workflow information (with IDs and metadata)
 * For list presentation
 */
export interface WorkflowInfo {
  /** Workflow ID */
  workflowId: string;
  /** Workflow metadata */
  metadata: WorkflowStorageMetadata;
}

/**
 * Workflow version information
 */
export interface WorkflowVersionInfo {
  /** version number */
  version: Version;
  /** Creation time */
  createdAt: Timestamp;
  /** founder */
  createdBy?: string;
  /** Explanation of changes */
  changeNote?: string;
  /** Is the current version */
  isCurrent?: boolean;
}

/**
 * Workflow version list options
 */
export interface WorkflowVersionListOptions {
  /** Maximum number of returns */
  limit?: number;
  /** shift */
  offset?: number;
}

/**
 * Workflow statistics
 */
export interface WorkflowStats {
  /** aggregate */
  total: number;
  /** Enablement Quantity */
  enabled: number;
  /** Number of disablements */
  disabled: number;
  /** Statistics by classification */
  byCategory: Record<string, number>;
  /** By author */
  byAuthor: Record<string, number>;
}
