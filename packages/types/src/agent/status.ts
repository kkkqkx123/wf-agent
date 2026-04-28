/**
 * Agent Loop Status Enumeration Definitions
 */

/**
 * Agent Loop Execution Status Enumeration
 */
export enum AgentLoopStatus {
  /** Created, not started */
  CREATED = "CREATED",
  /** under implementation */
  RUNNING = "RUNNING",
  /** Suspended */
  PAUSED = "PAUSED",
  /** done */
  COMPLETED = "COMPLETED",
  /** failure of execution */
  FAILED = "FAILED",
  /** Cancelled */
  CANCELLED = "CANCELLED",
}
