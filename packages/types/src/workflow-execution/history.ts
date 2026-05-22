/**
 * Workflow Execution History Type Definition
 */

import type { ID, Timestamp } from "../common.js";

/**
 * Node execution result type
 * 
 * output field uses Record<string, unknown> to enforce string-keyed data path access.
 * This aligns with the expression evaluator's path resolution mechanism
 * where output fields are accessed via dot-notation paths like "output.fieldName".
 * Using unknown would allow any value but break path-based reference semantics.
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
  /**
   * Node execution output data
   * 
   * Uses Record<string, unknown> to enforce string-keyed data path IDs.
   * All node outputs must be structured as key-value objects to support
   * expression path resolution (e.g., "output.content", "output.status").
   * 
   * Raw/scalar outputs from nodes like SCRIPT should be wrapped as
   * { result: <rawValue> } to maintain path-based access consistency.
   */
  output?: Record<string, unknown>;
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
