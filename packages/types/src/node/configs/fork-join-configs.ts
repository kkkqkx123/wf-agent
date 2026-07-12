/**
 * Fork/Merge Node Configuration Type Definition
 */

import type { ID } from '../../common.js';
import type { WorkflowVariableOutput, WorkflowMessageOutput, WorkflowDataOutput } from '../../workflow/boundary-config.js';

/**
 * Forked Path Configuration
 * Each forked path contains the path ID and corresponding child node IDs
 */
export interface ForkPath {
  /**
   * Forked Path IDs
   * Each path ID is unique within the workflow definition
   * Graph build phase converts to globally unique IDs
   */
  pathId: ID;
  /** Subnode ID, the starting node of the path */
  childNodeId: string;
}

/**
 * Fork Node Output
 * - launchedBranches: Array<{ pathId: ID, childNodeId: ID, strategy: 'serial' | 'parallel' }>
 */
export interface ForkNodeOutput {
  launchedBranches: Array<{
    pathId: ID;
    childNodeId: ID;
    strategy: 'serial' | 'parallel';
  }>;
}

/**
 * Forked Node Configuration
 */
export interface ForkNodeConfig {
  /** An array of forked paths, each containing pathId and childNodeId. */
  forkPaths: ForkPath[];
  /** Fork strategy (serial、parallel) */
  forkStrategy: 'serial' | 'parallel';
  /**
   * Failure handling strategy for fork branches.
   * - 'fail-fast': Terminate immediately if any branch fails (default).
   * - 'continue-on-error': Continue executing remaining branches even if some fail.
   * - 'fail-on-threshold': Fail only if the number of failed branches exceeds maxFailedBranches.
   */
  failureStrategy?: 'fail-fast' | 'continue-on-error' | 'fail-on-threshold';
  /** Maximum number of failed branches allowed (only used when failureStrategy is 'fail-on-threshold'). Default: 0 */
  maxFailedBranches?: number;
  /**
   * Retry policy for failed FORK branches (Task #7)
   * When a branch fails, automatically retry up to maxRetries times with exponential backoff.
   * If not specified, failed branches are not retried.
   */
  retryPolicy?: import('../../execution/failure-policy.js').RetryPolicy;
  /**
   * Timeout for each child branch execution in milliseconds (Task #9)
   * If a branch exceeds this timeout, it is immediately cancelled and marked as failed.
   * If not specified, no timeout is enforced.
   *
   * Problem #8 Fix: This is now perExecutionTimeout (per single attempt)
   * Use totalBranchTimeout for cumulative time across all retries
   */
  childExecutionTimeout?: number;
  /**
   * Total timeout for a branch across all retry attempts in milliseconds (Problem #8)
   * Limits the total wall-clock time a branch can take, including retries
   * If a branch exceeds this timeout across all retry attempts, it is marked as failed
   * If not specified, only childExecutionTimeout applies
   *
   * Example: childExecutionTimeout=5000, totalBranchTimeout=30000
   * - Single attempt max: 5 seconds
   * - All retries combined max: 30 seconds
   */
  totalBranchTimeout?: number;
}

/**
 * Join Node Output
 * - completedBranches: ID[] - IDs of branches that completed successfully
 * - failedBranches: Array<{pathId, error}> - Information about branches that failed (Task #1: Preserve error details)
 * - skippedBranches: ID[] - IDs of branches that were skipped
 * - strategy: string - The join strategy used
 * - aggregatedOutput?: unknown - The aggregated result from completed branches
 */
export interface JoinNodeOutput {
  completedBranches: ID[];
  failedBranches: Array<{ pathId: ID; error?: string }>;
  skippedBranches: ID[];
  strategy: string;
  aggregatedOutput?: unknown;
}

/**
 * Connection Node Configuration
 *
 * Description:
 * - Subexecution IDs are dynamically determined at runtime, generated during FORK node execution and stored in the execution context.
 * The JOIN node reads it from the execution context when it executes and is not defined in the node configuration.
 * - timeout is the maximum time (in seconds) to wait for the completion of the child execution.
 * When timeout = 0, no timeout is set, and the node waits until the condition is met;
 * When timeout > 0, it means wait for the maximum number of seconds, and throws TimeoutError if timeout is exceeded.
 * - forkPathIds must be identical to the paired FORK nodes (including order)
 * - mainPathId specifies the main execution path, must be one of the values in forkPathIds
 * - variableOutputs allows explicit variable export from branches to parent workflow
 * - messageOutputs defines explicit message context outputs from branches using boundary-config pattern
 */
export interface JoinNodeConfig {
  /**
   * Array of forked path IDs, which must be identical to the paired FORK nodes
   */
  forkPathIds: ID[];
  /** Join strategy (ALL_COMPLETED、ANY_COMPLETED、ALL_FAILED、ANY_FAILED、SUCCESS_COUNT_THRESHOLD) */
  joinStrategy: 'ALL_COMPLETED' | 'ANY_COMPLETED' | 'ALL_FAILED' | 'ANY_FAILED' | 'SUCCESS_COUNT_THRESHOLD';
  /** Number of successes threshold (used when joinStrategy is SUCCESS_COUNT_THRESHOLD) */
  threshold?: number;
  /** Wait timeout in seconds. 0 means no timeout, always wait; >0 means the maximum number of seconds to wait. Default 0 (no timeout) */
  timeout?: number;
  /** Main execution path ID, must be a value in forkPathIds (required) */
  mainPathId: ID;
  /** 
   * Variable outputs mapping - explicitly defines which variables to export from branches to parent workflow
   * Each mapping specifies source branch path and variable to export
   */
  variableOutputs?: WorkflowVariableOutput[];
  /** 
   * Message context outputs - explicitly defines which message contexts to export from branches to parent workflow
   * Uses the same boundary-config pattern as START/END nodes for consistency
   * Each output maps an internal context from a specific branch to an external context in the parent
   */
  messageOutputs?: Array<WorkflowMessageOutput & { sourcePathId: ID }>;

  /**
   * Data outputs - maps internal variables from the aggregated branch outputs
   * to keys in the parent workflow execution output.
   *
   * After branches complete and their outputs are collected, these mappings
   * export specified variables to the parent's execution output under the
   * defined output keys.
   *
   * Example:
   *   Branch completed with variable "result_summary"
   *   dataOutputs: [{ internalName: "result_summary", outputKey: "finalResult" }]
   *   Result: Parent output.finalResult gets the result_summary value
   */
  dataOutputs?: WorkflowDataOutput[];
}