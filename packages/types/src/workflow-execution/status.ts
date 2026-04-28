/**
 * Workflow Execution Status Type Definition
 */

/**
 * Workflow Execution Status
 */
export type WorkflowExecutionStatus =
  /** Created */
  | "CREATED"
  /** Running */
  | "RUNNING"
  /** Paused */
  | "PAUSED"
  /** Completed */
  | "COMPLETED"
  /** Failed */
  | "FAILED"
  /** Cancelled */
  | "CANCELLED"
  /** Timeout */
  | "TIMEOUT";

/**
 * Workflow Execution Type
 */
export type WorkflowExecutionType =
  /** Main execution */
  | "MAIN"
  /** FORK/JOIN child executions */
  | "FORK_JOIN"
  /** Triggered subworkflow execution */
  | "TRIGGERED_SUBWORKFLOW";
