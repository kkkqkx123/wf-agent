/**
 * Agent Hook Type Definition
 *
 * Based on the sdk/core/hooks generic framework, define Agent-specific Hook types.
 * Refer to NodeHook design pattern.
 */

import type { Condition } from "../graph/condition.js";

/**
 * Agent Hook Type
 *
 * Defines the trigger point for a Hook during Agent execution.
 */
export type AgentHookType =
  /** Triggered before the start of the iteration */
  | "BEFORE_ITERATION"
  /** Triggered at the end of an iteration */
  | "AFTER_ITERATION"
  /** Triggered before the start of the tool call */
  | "BEFORE_TOOL_CALL"
  /** Triggered at the end of a tool call */
  | "AFTER_TOOL_CALL"
  /** Triggered before the start of an LLM call */
  | "BEFORE_LLM_CALL"
  /** Triggered at the end of an LLM call */
  | "AFTER_LLM_CALL";

/**
 * Agent Hook Configuration
 *
 * Extends BaseHookDefinition to add Agent-specific properties.
 */
export interface AgentHook {
  /** Hook Type */
  hookType: AgentHookType;
  /** Trigger condition expression (optional) */
  condition?: Condition;
  /** Name of the custom event to trigger */
  eventName: string;
  /** Event load generation logic (optional) */
  eventPayload?: Record<string, unknown>;
  /** Enable or not (default true) */
  enabled?: boolean;
  /** Weighting (the higher the number the higher the priority) */
  weight?: number;
  /** Hook Whether to create checkpoints when triggered */
  createCheckpoint?: boolean;
  /** Checkpoint Description */
  checkpointDescription?: string;
}
