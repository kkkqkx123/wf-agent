/**
 * Thread state type definition
 */

/**
 * Workflow Execution Status
 */
export type WorkflowExecutionStatus =
  /** Created */
  | "CREATED"
  /** running */
  | "RUNNING"
  /** Suspended */
  | "PAUSED"
  /** done */
  | "COMPLETED"
  /** failed */
  | "FAILED"
  /** Cancelled */
  | "CANCELLED"
  /** overtime pay */
  | "TIMEOUT";

/**
 * Workflow Execution Type
 */
export type WorkflowExecutionType =
  /** main thread */
  | "MAIN"
  /** FORK/JOIN subthreads */
  | "FORK_JOIN"
  /** Triggered sub workflow thread */
  | "TRIGGERED_SUBWORKFLOW";

/**
 * Backward compatibility aliases
 * @deprecated Use WorkflowExecutionStatus instead
 */
export type ThreadStatus = WorkflowExecutionStatus;

/**
 * Backward compatibility alias
 * @deprecated Use WorkflowExecutionType instead
 */
export type ThreadType = WorkflowExecutionType;
