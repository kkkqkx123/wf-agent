/**
 * Default timeout configurations for different operations
 * 
 * Centralized timeout constants to ensure consistency across the SDK.
 * All timeout values are in milliseconds.
 */

/**
 * Default timeout values categorized by operation type
 */
export const DEFAULT_TIMEOUTS = {
  // ========================================
  // Workflow execution waiting
  // ========================================
  
  /**
   * Waiting for a single workflow execution to complete
   * Used in: waitForWorkflowExecutionCompleted()
   */
  WORKFLOW_EXECUTION_COMPLETION: 30000,  // 30 seconds
  
  /**
   * Waiting for workflow execution to pause
   * Used in: waitForWorkflowExecutionPaused()
   */
  WORKFLOW_EXECUTION_PAUSE: 5000,        // 5 seconds
  
  /**
   * Waiting for workflow execution cancellation
   * Used in: cascade cancel operations
   */
  WORKFLOW_EXECUTION_CANCEL: 10000,      // 10 seconds
  
  /**
   * Waiting for workflow execution to resume
   * Used in: waitForWorkflowExecutionResumed()
   */
  WORKFLOW_EXECUTION_RESUME: 5000,       // 5 seconds
  
  // ========================================
  // Child execution operations
  // ========================================
  
  /**
   * Waiting for child execution completion
   * Used in: waitForChildExecutionsCompletion()
   */
  CHILD_EXECUTION_WAIT: 30000,           // 30 seconds
  
  /**
   * Cascade cancel of all child executions
   * Used in: cascadeCancel()
   */
  CASCADE_CANCEL: 30000,                 // 30 seconds
  
  // ========================================
  // Node execution
  // ========================================
  
  /**
   * Waiting for node completion
   * Used in: waitForNodeCompleted()
   */
  NODE_COMPLETION: 30000,                // 30 seconds
  
  /**
   * Waiting for node failure
   * Used in: waitForNodeFailed()
   */
  NODE_FAILED: 30000,                    // 30 seconds
  
  // ========================================
  // Sync/Join operations
  // ========================================
  
  /**
   * SYNC node waiting for branch completion
   * Used in: SyncBarrier.waitForBranchCompletion()
   */
  SYNC_BRANCH_WAIT: 60000,               // 60 seconds
  
  /**
   * JOIN node waiting for multiple branches
   * Used in: join() operation
   */
  JOIN_COMPLETION: 60000,                // 60 seconds
  
  // ========================================
  // Lifecycle events
  // ========================================
  
  /**
   * Waiting for any lifecycle event
   * Used in: waitForAnyLifecycleEvent()
   */
  LIFECYCLE_EVENT: 5000,                 // 5 seconds
  
  // ========================================
  // Polling operations
  // ========================================
  
  /**
   * Default timeout for polling-based waiting
   * Used in: waitForCompletionByPolling()
   */
  POLLING_WAIT: 30000,                   // 30 seconds
  
  /**
   * Polling interval (how often to check status)
   */
  POLLING_INTERVAL: 100,                 // 100 milliseconds
  
  // ========================================
  // Fallback
  // ========================================
  
  /**
   * Default timeout when no specific timeout is configured
   */
  DEFAULT: 30000,                        // 30 seconds
  
  /**
   * Maximum allowed timeout (5 minutes)
   * Operations requesting longer timeouts will trigger a warning
   */
  MAX_ALLOWED: 300000,                   // 5 minutes
  
  /**
   * Wait forever constant (used as special value)
   * When timeout equals this value, wait indefinitely
   */
  WAIT_FOREVER: -1,
} as const;

/**
 * Timeout configuration type
 */
export type TimeoutConfig = typeof DEFAULT_TIMEOUTS;

/**
 * Timeout key type
 */
export type TimeoutKey = keyof TimeoutConfig;

/**
 * Validate timeout value
 * 
 * @param timeout Timeout value in milliseconds
 * @param context Description of the operation for error messages
 * @throws Error if timeout is invalid
 */
export function validateTimeout(timeout: number, context: string): void {
  if (timeout < 0 && timeout !== DEFAULT_TIMEOUTS.WAIT_FOREVER) {
    throw new Error(
      `Invalid timeout for ${context}: ${timeout}ms (must be non-negative or WAIT_FOREVER)`
    );
  }
  
  if (timeout > DEFAULT_TIMEOUTS.MAX_ALLOWED) {
    console.warn(
      `Very long timeout for ${context}: ${timeout}ms (>${DEFAULT_TIMEOUTS.MAX_ALLOWED / 1000}s). ` +
      `Consider if this is intentional.`
    );
  }
}

/**
 * Get default timeout for a specific operation
 * 
 * @param key Timeout configuration key
 * @returns Default timeout value in milliseconds
 * 
 * @example
 * ```typescript
 * const timeout = getDefaultTimeout('WORKFLOW_EXECUTION_COMPLETION');
 * // Returns 30000
 * ```
 */
export function getDefaultTimeout(key: TimeoutKey): number {
  return DEFAULT_TIMEOUTS[key];
}

/**
 * Check if a timeout value means "wait forever"
 * 
 * @param timeout Timeout value to check
 * @returns true if timeout means wait forever
 */
export function isWaitForever(timeout: number): boolean {
  return timeout === DEFAULT_TIMEOUTS.WAIT_FOREVER;
}

/**
 * Convert timeout to actual value (handles WAIT_FOREVER)
 * 
 * @param timeout Timeout value (may be WAIT_FOREVER)
 * @returns Actual timeout in milliseconds, or undefined for WAIT_FOREVER
 */
export function toActualTimeout(timeout: number): number | undefined {
  return isWaitForever(timeout) ? undefined : timeout;
}
