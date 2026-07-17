/**
 * Trigger Definition Type
 */

import type { ID, Timestamp, Metadata } from "../common.js";
import type { TriggerStatus } from "./state.js";
import type { TriggerCondition, TriggerAction } from "./config.js";

/**
 * Trigger Definition Interface (compile-time only)
 *
 * Contains only the fields that are specified at definition time.
 * Used for workflow definitions, templates, and configuration.
 * Runtime fields (status, triggerCount, etc.) are excluded.
 */
export interface TriggerDefinition {
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

  /**
   * Enable or not (used when defining)
   * - Default true
   * - Maps to status when converted to a Trigger.
   */
  enabled?: boolean;
}

/**
 * Trigger Interface (definition + runtime fields)
 *
 * Design Notes:
 * - Used both at definition time (Workflow definition) and at runtime.
 * - Definition time use: provide id, name, condition, action, enabled and other basic fields.
 * - Runtime use: add status, triggerCount, createdAt, updatedAt and other runtime fields.
 */
export interface Trigger extends TriggerDefinition {
  // ==========================================================================
  // Runtime fields (may not be provided at definition time, automatically populated at runtime)
  // ==========================================================================

  /**
   * Trigger status (runtime)
   * - When defined: can be set indirectly via the enabled field
   * - Runtime: maintained by the system
   */
  status?: TriggerStatus;

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
 * At definition time, this contains only compile-time fields.
 */
export type WorkflowTrigger = TriggerDefinition;
