/**
 * Workflow Execution Related Type Definitions
 */

import type { ID, Timestamp } from "../common.js";
import type { WorkflowExecutionStatus } from "./status.js";
import type { NodeExecutionResult } from "./history.js";

/**
 * Workflow Execution Option Type
 */
export interface WorkflowExecutionOptions {
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
  /** Error callback */
  onError?: (error: unknown) => void | Promise<void>;
}

/**
 * Thread Options (deprecated, use WorkflowExecutionOptions)
 * @deprecated Use WorkflowExecutionOptions instead
 */
export type ThreadOptions = WorkflowExecutionOptions;

/**
 * Workflow execution result type
 *
 * Design principle:
 * - Use status field to indicate execution status instead of redundant success field.
 * - Errors are stored in the errors array, with error counts provided in the metadata.
 * - Caller determines success/failure by status
 */
export interface WorkflowExecutionResult {
  /** Execution ID */
  executionId: ID;
  /** @deprecated Use executionId instead. Kept for backward compatibility */
  id: ID;
  /** Output data */
  output: Record<string, unknown>;
  /** Execution time (milliseconds) */
  executionTime: Timestamp;
  /** Array of node execution results */
  nodeResults: NodeExecutionResult[];
  /** Execution metadata */
  metadata: WorkflowExecutionResultMetadata;
}

/**
 * Workflow execution result metadata
 */
export interface WorkflowExecutionResultMetadata {
  /** Execution state */
  status: WorkflowExecutionStatus;
  /** Start time */
  startTime: Timestamp;
  /** End time */
  endTime: Timestamp;
  /** Execution time (milliseconds) */
  executionTime: Timestamp;
  /** Number of nodes */
  nodeCount: number;
  /** Number of errors */
  errorCount: number;
}
