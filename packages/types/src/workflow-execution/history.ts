/**
 * Workflow Execution History Type Definition
 */

import type { ID, Timestamp } from "../common.js";

/**
 * Node execution result type
 */
export interface NodeExecutionResult {
  /** Node ID */
  nodeId: ID;
  /** Node type */
  nodeType: string;
  /** Execution status */
  status: "PENDING" | "RUNNING" | "COMPLETED" | "FAILED" | "SKIPPED" | "CANCELLED";
  /** Execution step number */
  step: number;
  /** Error message */
  error?: unknown;
  /** Execution time (milliseconds) */
  executionTime?: Timestamp;
  /** Start time */
  startTime?: Timestamp;
  /** End time */
  endTime?: Timestamp;
  /** Timestamp */
  timestamp?: Timestamp;
}

/**
 * Execution History Entry Type
 */
export interface ExecutionHistoryEntry {
  /** Execution step number */
  step: number;
  /** Node ID */
  nodeId: ID;
  /** Node type */
  nodeType: string;
  /** Execution status */
  status: "PENDING" | "RUNNING" | "COMPLETED" | "FAILED" | "SKIPPED" | "CANCELLED";
  /** Timestamp */
  timestamp: Timestamp;
  /** Execution data (for tracing and debugging) */
  data?: unknown;
  /** Error message */
  error?: unknown;
}
