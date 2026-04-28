/**
 * Thread history type definition
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
  /** execution status (computing) */
  status: "PENDING" | "RUNNING" | "COMPLETED" | "FAILED" | "SKIPPED" | "CANCELLED";
  /** Implementation step number */
  step: number;
  /** error message */
  error?: unknown;
  /** Execution time (milliseconds) */
  executionTime?: Timestamp;
  /** Starting time */
  startTime?: Timestamp;
  /** end time */
  endTime?: Timestamp;
  /** timestamp */
  timestamp?: Timestamp;
}

/**
 * Execution History Entry Type
 */
export interface ExecutionHistoryEntry {
  /** Implementation step number */
  step: number;
  /** Node ID */
  nodeId: ID;
  /** Node type */
  nodeType: string;
  /** execution status (computing) */
  status: "PENDING" | "RUNNING" | "COMPLETED" | "FAILED" | "SKIPPED" | "CANCELLED";
  /** timestamp */
  timestamp: Timestamp;
  /** Implementation data (for tracing and debugging) */
  data?: unknown;
  /** error message */
  error?: unknown;
}
