/**
 * Generic Hook Type Definitions
 *
 * Provides generic base Hook types that can be reused by both the Workflow (Graph)
 * and Agent modules. Specific modules implement Hooks for specific scenarios by
 * extending these generic types with a concrete hookType union.
 *
 * Architecture:
 * - BaseHookConfig<THookType>: Runtime hook configuration (uses Condition objects)
 * - BaseHookStaticConfig<THookType>: File-based static hook configuration (uses string expressions)
 *
 * @see NodeHook in node/hooks.ts - Workflow node hook (extends BaseHookConfig<HookType>)
 * @see AgentHook in agent-execution/hooks.ts - Agent loop hook (extends BaseHookConfig<AgentHookType>)
 * @see AgentHookStatic in agent/static-config.ts - File-based agent hook (extends BaseHookStaticConfig<AgentHookType>)
 */

import type { Condition } from "./condition.js";

/**
 * Generic Hook Configuration (Runtime)
 *
 * All hook definitions should extend this interface with a concrete hookType union.
 * Used during runtime execution with parsed Condition objects.
 *
 * @typeParam THookType - The specific hook type union (e.g. "BEFORE_EXECUTE" | "AFTER_EXECUTE")
 */
export interface BaseHookConfig<THookType extends string = string> {
  /** Hook type identifier (specific to the module) */
  hookType: THookType;
  /** Trigger condition expression (optional) */
  condition?: Condition;
  /** Event name, used for identification, routing, and logging */
  eventName: string;
  /** Event payload template, supports variable substitution */
  eventPayload?: Record<string, unknown>;
  /** Whether the hook is enabled (default: true) */
  enabled?: boolean;
  /** Priority weight (higher number = higher priority, executed earlier) */
  weight?: number;
  /** Whether to create a checkpoint when this hook is triggered */
  createCheckpoint?: boolean;
  /** Description for the checkpoint (used when createCheckpoint is true) */
  checkpointDescription?: string;
}

/**
 * Generic Hook Static Configuration (File-based)
 *
 * Used in file-based configuration (TOML/JSON).
 * Differs from BaseHookConfig in that condition is a string expression
 * for full serializability.
 *
 * @typeParam THookType - The specific hook type union
 */
export interface BaseHookStaticConfig<THookType extends string = string> {
  /** Hook type identifier */
  hookType: THookType;
  /** Trigger condition expression as string (for file format serialization) */
  condition?: string;
  /** Event name */
  eventName: string;
  /** Event payload (optional) */
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
