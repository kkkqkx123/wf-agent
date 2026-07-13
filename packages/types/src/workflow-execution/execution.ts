/**
 * Workflow Execution Related Type Definitions
 */

import type { ID, Timestamp } from "../common.js";
import type { WorkflowExecutionStatus } from "./status.js";
import type { NodeExecutionResult } from "./history.js";

/**
 * Retry budget configuration for workflow execution.
 * Controls global retry budget across all nodes and branches.
 */
export interface RetryBudgetOption {
  /** Maximum total retry count across the entire workflow (undefined = unlimited count) */
  maxRetries?: number;
  /** Maximum total time spent on retry delays (ms), 0 = unlimited */
  timeBudgetMs?: number;
}

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
  /** Maximum wall-clock execution time in milliseconds (0 = no limit) */
  maxExecutionTime?: number;
  /** Whether to enable checkpoints */
  enableCheckpoints?: boolean;
  /** Token Limit Threshold */
  tokenLimit?: number;
  /**
   * Default timeout for individual node execution in milliseconds.
   * Can be overridden at the node level.
   * @default 30000
   */
  nodeTimeout?: number;
  /**
   * Maximum pause duration in milliseconds (0 = no limit).
   * When set, the workflow execution will be automatically cancelled if paused longer.
   * @default 0
   */
  maxPauseDuration?: number;
  /** Node execution completion callback */
  onNodeExecuted?: (result: NodeExecutionResult) => void | Promise<void>;

  /**
   * Global default node retry configuration.
   * Individual nodes can override via onFailure/maxRetries/retryDelayMs in their config.
   * @default { maxRetries: 0, retryDelay: 1000, exponentialBackoff: true }
   */
  defaultNodeRetry?: {
    /** Maximum number of retry attempts (0 = no retry). @default 0 */
    maxRetries: number;
    /** Base delay between retries in milliseconds. @default 1000 */
    retryDelay: number;
    /** Whether to use exponential backoff for retry delays. @default true */
    exponentialBackoff: boolean;
  };

  /** Tool Callbacks */
  onToolCalled?: (toolId: ID, parameters: Record<string, unknown>) => void | Promise<void>;
  /** Error callback */
  onError?: (error: unknown) => void | Promise<void>;

  /**
   * Global retry budget configuration.
   * When set, a RetryBudget is created for the entire workflow execution
   * and shared across all node-level retries and FORK branch retries.
   */
  retryBudget?: RetryBudgetOption;

  /**
   * Workflow-level failure strategy.
   * Controls what happens when a node fails (after node-level retries are exhausted):
   * - "fail" (default): Fail the entire workflow immediately.
   * - "continue": Mark the failed node as SKIPPED and continue execution.
   * - "retry": Retry the entire workflow execution from the beginning.
   * @default "fail"
   */
  onFailure?: "retry" | "continue" | "fail";

  /**
   * Maximum number of workflow-level retry attempts.
   * Only applicable when onFailure === "retry".
   * @default 0
   */
  maxRetries?: number;

  /**
   * Base delay between workflow-level retries in milliseconds.
   * Only applicable when onFailure === "retry".
   * @default 1000
   */
  retryDelayMs?: number;

  /**
   * Whether to use exponential backoff for retry delays.
   * Only applicable when onFailure === "retry".
   * @default true
   */
  exponentialBackoff?: boolean;

  /**
   * Fallback output to use when the workflow fails and onFailure === "continue".
   * This output is returned as the workflow result instead of failing.
   */
  fallbackOutput?: {
    /** Fallback output data */
    output?: Record<string, unknown>;
    /** Optional content for result metadata */
    content?: string;
  };
}

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
  /** Output data */
  output: Record<string, unknown>;
  /** Execution time (milliseconds) */
  executionTime: Timestamp;
  /** Array of node execution results */
  nodeResults: NodeExecutionResult[];
  /** Execution metadata */
  metadata: WorkflowExecutionResultMetadata;

  // ============ [Problem #4 Fix] Error Details ============

  /**
   * Errors that occurred during workflow execution.
   * Includes all errors from failed nodes and global workflow failures.
   * Each entry is either a NodeExecutionResult error or the global workflow error.
   */
  errors?: Array<{
    /** Node ID where the error occurred (if node-level) */
    nodeId?: ID;
    /** Error message */
    message: string;
    /** Error type */
    type?: string;
  }>;

  // ============ [Issue 4 Fix] Retry & Timeout Statistics ============

  /**
   * Workflow-level retry count.
   * Number of times the entire workflow execution was retried.
   */
  workflowRetryCount?: number;

  /**
   * Workflow-level retry delay time in milliseconds.
   * Total time spent in delays for workflow-level retries.
   */
  workflowRetryDelayTime?: number;

  /**
   * Total number of retry attempts across all levels (workflow + node).
   */
  totalRetryCount?: number;

  /**
   * Total retry delay time across all levels in milliseconds.
   */
  totalRetryDelayTime?: number;

  /**
   * Number of timeouts that occurred during execution.
   */
  timeoutCount?: number;

  /**
   * Error chain from root cause to final error.
   * Provides full traceability for failure analysis.
   */
  errorChain?: Array<{
    /** Error ID */
    id: string;
    /** Error message */
    message: string;
    /** Error type */
    errorType: string;
    /** Severity level */
    severity: string;
    /** Node ID where error occurred */
    nodeId?: string;
    /** Timestamp when error occurred */
    timestamp: number;
  }>;
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
  /** Interruption type (if interrupted) */
  interruptionType?: "PAUSE" | "STOP";
  /** Node ID where interruption occurred */
  interruptedAtNodeId?: string;
}
