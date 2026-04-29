/**
 * Trigger Definition Type
 */

import type { ID, Timestamp, Metadata } from "../common.js";
import type { TriggerStatus } from "./state.js";
import type { TriggerCondition, TriggerAction } from "./config.js";

/**
 * Trigger Definition Interface
 *
 * Design Notes:
 * - Used both at definition time (Workflow definition) and at runtime.
 * - Definition time use: provide id, name, condition, action, enabled and other basic fields.
 * - Runtime use: add status, triggerCount, createdAt, updatedAt and other runtime fields.
 */
export interface Trigger {
  /** Trigger Unique Identifier */
  id: ID;
  /** Trigger Name */
  name: string;
  /** Trigger Description */
  description?: string;
  /** trigger condition */
  condition: TriggerCondition;
  /** trigger action */
  action: TriggerAction;
  /** Limit on the number of triggers (0 means no limit) */
  maxTriggers?: number;
  /** Trigger Metadata */
  metadata?: Metadata;
  /** Whether to create a checkpoint when triggered */
  createCheckpoint?: boolean;
  /** Checkpoint Description */
  checkpointDescription?: string;

  // ==========================================================================
  // Runtime fields (may not be provided at definition time, automatically populated at runtime)
  // ==========================================================================

  /**
   * Trigger status (runtime)
   * - When defined: can be set indirectly via the enabled field
   * - Runtime: maintained by the system
   */
  status?: TriggerStatus;

  /**
   * Enable or not (used when defining)
   * - Default true
   * - Maps to status when converted to a Trigger.
   */
  enabled?: boolean;

  /** Associated workflow ID (runtime) */
  workflowId?: ID;
  /** Associated execution ID (runtime) */
  executionId?: ID;
  /** Number of times triggered (runtime) */
  triggerCount?: number;
  /** Creation time (runtime) */
  createdAt?: Timestamp;
  /** Update time (runtime) */
  updatedAt?: Timestamp;
}

/**
 * Workflow Trigger Type Alias
 * Used to denote a fully defined trigger in a workflow (as opposed to a TriggerReference)
 */
export type WorkflowTrigger = Trigger;
