/**
 * Thread state type definition
 */

/**
 * thread state
 */
export type ThreadStatus =
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
 * Thread type
 */
export type ThreadType =
  /** main thread */
  | "MAIN"
  /** FORK/JOIN subthreads */
  | "FORK_JOIN"
  /** Triggered sub workflow thread */
  | "TRIGGERED_SUBWORKFLOW";
