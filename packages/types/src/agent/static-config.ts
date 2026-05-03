/**
 * Agent Loop Static Configuration Types
 *
 * These types represent file-based configuration (TOML/JSON) for Agent Loop.
 * They are fully serializable and do not contain any executable functions.
 *
 * Design Principles:
 * - All fields are static and serializable
 * - Conditions use string expressions instead of Condition objects
 * - Used by SDK's config parser to load agent configurations from files
 * - Transformed to runtime types by SDK processors
 */

import type { AgentHookType } from "../agent-execution/hooks.js";

/**
 * Agent Hook Static Configuration
 *
 * Used in file-based configuration (TOML/JSON).
 * Differs from AgentHook in that condition is a string expression.
 *
 * @see AgentHook - Runtime hook with parsed Condition object
 */
export interface AgentHookStatic {
  /** Hook type */
  hookType: AgentHookType;
  /** Trigger condition expression (optional, as string for file format) */
  condition?: string;
  /** Name of the custom event to trigger */
  eventName: string;
  /** Event payload (optional) */
  eventPayload?: Record<string, unknown>;
  /** Enable or not (default true) */
  enabled?: boolean;
  /** Weight (higher number = higher priority) */
  weight?: number;
  /** Whether to create checkpoint when triggered */
  createCheckpoint?: boolean;
  /** Checkpoint description */
  checkpointDescription?: string;
}

/**
 * Agent Trigger Action Configuration
 *
 * Defines what action to take when a trigger fires.
 */
export interface AgentTriggerAction {
  /** Action type */
  type: "pause" | "stop" | "checkpoint" | "custom";
  /** Action configuration (optional) */
  config?: Record<string, unknown>;
}

/**
 * Agent Trigger Static Configuration
 *
 * Defines triggers that can interrupt or control agent execution.
 * Note: Triggers are designed for future extension.
 *
 * @example
 * ```toml
 * [[triggers]]
 * id = "pause-on-condition"
 * type = "condition"
 * condition = "iteration > 10"
 * enabled = true
 * [triggers.action]
 * type = "pause"
 * ```
 */
export interface AgentTriggerStatic {
  /** Trigger ID */
  id: string;
  /** Trigger type */
  type: "event" | "condition" | "schedule";
  /** Trigger condition expression (for condition type) */
  condition?: string;
  /** Event name (for event type) */
  eventName?: string;
  /** Enable or disable */
  enabled?: boolean;
  /** Action to take when triggered */
  action: AgentTriggerAction;
}

/**
 * Agent Checkpoint Configuration
 *
 * Defines checkpoint creation policies for Agent Loop.
 */
export interface AgentCheckpointConfig {
  /** Whether to create checkpoint at the end */
  createOnEnd?: boolean;
  /** Whether to create checkpoint on error */
  createOnError?: boolean;
  /** Whether to create checkpoint after each iteration */
  createOnIteration?: boolean;
}

/**
 * Agent Loop Metadata
 *
 * Contains descriptive and organizational information about an agent configuration.
 */
export interface AgentLoopMetadata {
  /** Author name */
  author?: string;
  /** Tags for categorization */
  tags?: string[];
  /** Custom properties */
  [key: string]: unknown;
}
