/**
 * Thread execution related type definitions
 */

import type { ID, Timestamp } from "../common.js";
import type { WorkflowExecutionStatus } from "./status.js";
import type { NodeExecutionResult } from "./history.js";

/**
 * Thread Execution Option Type
 */
export interface ThreadOptions {
  /** Input Data Objects */
  input?: Record<string, unknown>;
  /** Maximum number of execution steps */
  maxSteps?: number;
  /** Timeout time (milliseconds) */
  timeout?: number;
  /** Whether to enable checkpoints */
  enableCheckpoints?: boolean;
  /** Token Limit Threshold */
  tokenLimit?: number;
  /** Node execution completion callback */
  onNodeExecuted?: (result: NodeExecutionResult) => void | Promise<void>;
  /** Tool Callbacks */
  onToolCalled?: (toolId: ID, parameters: Record<string, unknown>) => void | Promise<void>;
  /** error callback */
  onError?: (error: unknown) => void | Promise<void>;
}

/**
 * Thread execution result type
 *
 * Design principle:
 * - Use status field to indicate execution status instead of redundant success field.
 * - Errors are stored in the errors array, with error counts provided in the metadata.
 * - Caller determines success/failure by status
 */
export interface ThreadResult {
  /** Thread ID */
  threadId: ID;
  /** output data */
  output: Record<string, unknown>;
  /** Execution time (milliseconds) */
  executionTime: Timestamp;
  /** Array of node execution results */
  nodeResults: NodeExecutionResult[];
  /** Implementation metadata */
  metadata: ThreadResultMetadata;
}

/**
 * Thread execution result metadata
 */
export interface ThreadResultMetadata {
  /** thread state */
  status: WorkflowExecutionStatus;
  /** Starting time */
  startTime: Timestamp;
  /** end time */
  endTime: Timestamp;
  /** Execution time (milliseconds) */
  executionTime: Timestamp;
  /** Number of nodes */
  nodeCount: number;
  /** Number of errors */
  errorCount: number;
}
