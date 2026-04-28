/**
 * Thread Context Type Definition
 */

import type { ID } from "../common.js";

/**
 * FORK/JOIN Context
 * Thread relationship management for FORK/JOIN scenarios
 */
export interface ForkJoinContext {
  /** Fork operation ID */
  forkId: string;
  /** Fork path ID (for identifying the main thread during Join) */
  forkPathId: string;
}

/**
 * Triggered Sub-Workflow Context
 * Thread relationship management for Triggered sub-workflow scenarios
 */
export interface TriggeredSubworkflowContext {
  /** Parent Thread ID */
  parentThreadId: ID;
  /** Array of subthread IDs */
  childThreadIds: ID[];
  /** Triggered sub workflow ID */
  triggeredSubworkflowId: ID;
}
