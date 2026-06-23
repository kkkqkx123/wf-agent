/**
 * Agent Trigger Execution Types
 *
 * Defines triggers that can interrupt or control agent execution during loop iterations.
 * Triggers are similar to hooks but focus on state-based conditions and immediate actions.
 *
 * Key differences from AgentHookStatic:
 * - Contains executable Condition objects instead of string expressions
 * - Used directly by AgentLoopEntity for trigger matching
 * - Cannot be loaded from configuration files due to Condition object types
 */

import type { TriggerActionType } from "../trigger/config.js";
import type { Condition } from "../condition.js";

/**
 * Agent Trigger Runtime Configuration
 *
 * Used during execution with parsed Condition objects.
 * Extends BaseTriggerAction with agent-specific action types.
 */
export interface AgentTrigger {
  /** Trigger ID */
  id: string;

  /** Trigger name */
  name?: string;

  /** Trigger description */
  description?: string;

  /** Enable or disable */
  enabled?: boolean;

  /** Maximum number of times this trigger can fire */
  maxTriggers?: number;

  /** Trigger condition (with parsed Condition object) */
  condition: {
    eventType: string;
    eventName?: string;
    condition?: Condition;
  };

  /** Action to take when triggered */
  action: {
    type: TriggerActionType;
    parameters: Record<string, unknown>;
  };

  /** Priority (higher priority triggers execute first) */
  priority?: number;
}
