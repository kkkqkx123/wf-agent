/**
 * Fork/Merge Node Configuration Type Definition
 */

import type { ID } from '../../common.js';

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
  /** 分叉策略(串行、并行) */
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
 */
export interface JoinNodeConfig {
  /**
   * Array of forked path IDs, which must be identical to the paired FORK nodes
   */
  forkPathIds: ID[];
  /** 连接策略(ALL_COMPLETED、ANY_COMPLETED、ALL_FAILED、ANY_FAILED、SUCCESS_COUNT_THRESHOLD) */
  joinStrategy: 'ALL_COMPLETED' | 'ANY_COMPLETED' | 'ALL_FAILED' | 'ANY_FAILED' | 'SUCCESS_COUNT_THRESHOLD';
  /** Number of successes threshold (used when joinStrategy is SUCCESS_COUNT_THRESHOLD) */
  threshold?: number;
  /** Wait timeout in seconds. 0 means no timeout, always wait; >0 means the maximum number of seconds to wait. Default 0 (no timeout) */
  timeout?: number;
  /** Main execution path ID, must be a value in forkPathIds (required) */
  mainPathId: ID;
}