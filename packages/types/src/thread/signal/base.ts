/**
 * Defining the Base Signal Types
 * Define the base type of thread interrupt signal
 */

import type { ID } from "../../common.js";
import type { ThreadInterruptedException } from "../../errors/index.js";

/**
 * Thread Abort Signal
 * Extends the standard AbortSignal to add thread and node context information
 */
export interface ThreadAbortSignal extends Omit<AbortSignal, "reason"> {
  /** Thread ID */
  threadId: ID;
  /** Node ID */
  nodeId: ID;
  /** Reason for interruption */
  reason: ThreadInterruptedException;
}
