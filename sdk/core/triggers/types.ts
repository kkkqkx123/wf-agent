/**
 * Definition of a generic Trigger type
 *
 * Provides a basic Trigger type that can be reused by both the Graph and Agent modules.
 */

import type { Metadata } from "@wf-agent/types";

/**
 * General Trigger Conditions (Basic Interface)
 */
export interface BaseTriggerCondition {
  /** Event Type */
  eventType: string;
  /** Custom event name (optional) */
  eventName?: string;
  /** Conditional Metadata */
  metadata?: Metadata;
}

/**
 * Generic Trigger Actions (Basic Interface)
 */
export interface BaseTriggerAction {
  /** Action Type */
  type: string;
  /** Action parameters */
  parameters: Record<string, unknown>;
  /** Action Metadata */
  metadata?: Metadata;
}

/**
 * General Trigger Definition (Basic Interface)
 */
export interface BaseTriggerDefinition {
  /** Trigger ID */
  id: string;
  /** Trigger Name */
  name: string;
  /** Trigger Description */
  description?: string;
  /** Trigger conditions */
  condition: BaseTriggerCondition;
  /** Trigger an action */
  action: BaseTriggerAction;
  /** Whether to enable */
  enabled?: boolean;
  /** Maximum number of triggers (0 indicates no limit) */
  maxTriggers?: number;
  /** Triggered count */
  triggerCount?: number;
  /** Trigger Metadata */
  metadata?: Metadata;
}

/**
 * Trigger execution result
 */
export interface TriggerExecutionResult {
  /** Trigger ID */
  triggerId: string;
  /** Did it succeed? */
  success: boolean;
  /** Actions performed */
  action: BaseTriggerAction;
  /** Execution time */
  executionTime: number;
  /** Result data */
  result?: unknown;
  /** Error message */
  error?: string | Error;
}

/**
 * Trigger Status
 */
export type TriggerStatus =
  | "idle" // leisure time
  | "active" // brighten up
  | "triggered" // triggered
  | "disabled" // disabled
  | "expired"; // Expired (maximum number of triggers reached)

/**
 * Event Data (Basic Interface)
 */
export interface BaseEventData {
  /** Event Type */
  type: string;
  /** Event Name (optional) */
  eventName?: string;
  /** Event data */
  data?: unknown;
  /** timestamp */
  timestamp: number;
  /** Source ID */
  sourceId?: string;
}

/**
 * Trigger handler function type
 */
export type TriggerHandler<TTrigger extends BaseTriggerDefinition = BaseTriggerDefinition> = (
  trigger: TTrigger,
  eventData: BaseEventData,
) => Promise<TriggerExecutionResult>;

/**
 * Trigger Matcher Function Type
 */
export type TriggerMatcher = (condition: BaseTriggerCondition, event: BaseEventData) => boolean;
