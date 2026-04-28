/**
 * Trigger State Type Definition
 */

import type { ID, Timestamp } from "../common.js";

/**
 * trigger state
 */
export type TriggerStatus =
  /** enabled */
  | "enabled"
  /** disabled */
  | "disabled"
  /** triggered */
  | "triggered";

/**
 * Trigger Runtime State Interface
 * Contains only runtime state, not trigger definitions
 * Used to save the runtime state of triggers in the state manager and checkpoints.
 */
export interface TriggerRuntimeState {
  /** Trigger ID */
  triggerId: ID;
  /** Thread ID */
  threadId: ID;
  /** Workflow ID */
  workflowId: ID;
  /** trigger state */
  status: TriggerStatus;
  /** Number of triggers */
  triggerCount: number;
  /** Last updated */
  updatedAt: Timestamp;
}
