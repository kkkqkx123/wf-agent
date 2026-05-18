/**
 * Fork/Merge Node Configuration Type Definition
 */

import type { ID } from '../../common.js';
import type { WorkflowVariableOutput, WorkflowMessageOutput } from '../../workflow/boundary-config.js';

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
 * Forked Node Configuration
 */
export interface ForkNodeConfig {
  /** An array of forked paths, each containing pathId and childNodeId. */
  forkPaths: ForkPath[];
  /** Fork strategy (serial、parallel) */
  forkStrategy: 'serial' | 'parallel';
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
}