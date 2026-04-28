/**
 * Base Signal Type Definitions
 * Define the base type of workflow execution interrupt signal
 */

import type { ID } from "../../common.js";
import type { ThreadInterruptedException } from "../../errors/index.js";

/**
 * Workflow Execution Abort Signal
 * Extends the standard AbortSignal to add execution and node context information
 */
export interface WorkflowExecutionAbortSignal extends Omit<AbortSignal, "reason"> {
  /** Execution ID */
  executionId: ID;
  /** Node ID */
  nodeId: ID;
  /** Reason for interruption */
  reason: ThreadInterruptedException;
}
