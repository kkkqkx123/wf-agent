/**
 * Workflow Execution Context Type Definition
 */

import type { ID } from "../common.js";

/**
 * FORK/JOIN Context
 * Execution relationship management for FORK/JOIN scenarios
 */
export interface ForkJoinContext {
  /** Fork operation ID */
  forkId: string;
  /** Fork path ID (for identifying the main execution during Join) */
  forkPathId: string;
}

/**
 * Triggered Sub-Workflow Context
 * Execution relationship management for Triggered sub-workflow scenarios
 */
export interface TriggeredSubworkflowContext {
  /** Parent Execution ID */
  parentExecutionId: ID;
  /** Array of child execution IDs */
  childExecutionIds: ID[];
  /** Triggered subworkflow ID */
  triggeredSubworkflowId: ID;
}
