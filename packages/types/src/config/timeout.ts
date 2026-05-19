/**
 * Timeout Configuration Type Definitions
 * 
 * Defines types for timeout configuration across the SDK.
 * These timeouts control internal operation waiting periods.
 */

/**
 * Timeout configuration for different SDK operations
 * All values are in milliseconds
 */
export interface TimeoutConfig {
  /** Waiting for a single workflow execution to complete (default: 30000) */
  workflowExecutionCompletion?: number;
  
  /** Waiting for workflow execution to pause (default: 5000) */
  workflowExecutionPause?: number;
  
  /** Waiting for workflow execution cancellation (default: 10000) */
  workflowExecutionCancel?: number;
  
  /** Waiting for workflow execution to resume (default: 5000) */
  workflowExecutionResume?: number;
  
  /** Waiting for child execution completion (default: 30000) */
  childExecutionWait?: number;
  
  /** Cascade cancel of all child executions (default: 30000) */
  cascadeCancel?: number;
  
  /** Waiting for node completion (default: 30000) */
  nodeCompletion?: number;
  
  /** Waiting for node failure (default: 30000) */
  nodeFailed?: number;
  
  /** SYNC node waiting for branch completion (default: 60000) */
  syncBranchWait?: number;
  
  /** JOIN node waiting for multiple branches (default: 60000) */
  joinCompletion?: number;
  
  /** Waiting for any lifecycle event (default: 5000) */
  lifecycleEvent?: number;
  
  /** Default timeout for polling-based waiting (default: 30000) */
  pollingWait?: number;
  
  /** Polling interval in milliseconds (default: 100) */
  pollingInterval?: number;
  
  /** Default timeout when no specific timeout is configured (default: 30000) */
  default?: number;
  
  /** Maximum allowed timeout in milliseconds (default: 300000 / 5 minutes) */
  maxAllowed?: number;
}
