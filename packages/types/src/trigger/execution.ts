/**
 * Trigger execution related type definitions
 */

import type { ID, Timestamp, Metadata } from "../common.js";
import type { TriggerAction } from "./config.js";
import type { Trigger } from "./definition.js";

/**
 * Trigger Execution Result Interface
 */
export interface TriggerExecutionResult {
  /** Trigger ID */
  triggerId: ID;
  /** Successful implementation */
  success: boolean;
  /** Actions performed */
  action: TriggerAction;
  /** execution time */
  executionTime: Timestamp;
  /** Implementation results data */
  result?: unknown;
  /** Error message (if failed) */
  error?: unknown;
  /** Implementation metadata */
  metadata?: Metadata;
}

/**
 * Converts Trigger at definition time to Trigger at runtime.
 * Supplement runtime fields (status, triggerCount, createdAt, updatedAt, workflowId)
 * @param trigger Trigger definition
 * @param workflowId workflowId
 * @returns Runtime Trigger instance
 */
export function convertToTrigger(trigger: Trigger, workflowId: ID): Trigger {
  return {
    ...trigger,
    status: trigger.enabled !== false ? "enabled" : "disabled",
    workflowId: workflowId,
    triggerCount: trigger.triggerCount ?? 0,
    createdAt: trigger.createdAt ?? Date.now(),
    updatedAt: trigger.updatedAt ?? Date.now(),
  };
}
