/**
 * Agent Hook Type Definition
 *
 * Defines runtime hook types for Agent Loop execution.
 * Part of the agent-execution package for runtime-related types.
 *
 * Based on the sdk/core/hooks generic framework, defines Agent-specific Hook types.
 * Refer to NodeHook design pattern.
 */

import type { Condition } from "../graph/condition.js";

/**
 * Agent Hook Type
 *
 * Defines the trigger point for a Hook during Agent execution.
 */
export type AgentHookType =
  /** Triggered before the start of each iteration */
  | "BEFORE_ITERATION"
  /** Triggered at the end of each iteration */
  | "AFTER_ITERATION"
  /** Triggered before each tool call */
  | "BEFORE_TOOL_CALL"
  /** Triggered after each tool call completes */
  | "AFTER_TOOL_CALL"
  /** Triggered before each LLM call */
  | "BEFORE_LLM_CALL"
  /** Triggered after each LLM call completes */
  | "AFTER_LLM_CALL";

/**
 * Agent Hook Configuration (Runtime)
 *
 * Runtime hook configuration with parsed Condition objects.
 * Used during agent execution for event triggering.
 *
 * @see AgentHookStatic - Static hook definition for file-based configuration
 */
export interface AgentHook {
  /** Hook type defining when the hook triggers */
  hookType: AgentHookType;
  /** Trigger condition expression (optional) */
  condition?: Condition;
  /** Name of the event to emit when triggered */
  eventName: string;
  /** Event payload data (optional) */
  eventPayload?: Record<string, unknown>;
  /** Whether the hook is enabled (default: true) */
  enabled?: boolean;
  /** Priority weight (higher number = higher priority) */
  weight?: number;
  /** Whether to create a checkpoint when triggered */
  createCheckpoint?: boolean;
  /** Checkpoint description */
  checkpointDescription?: string;
}
