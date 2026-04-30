/**
 * General Hook Type Definition
 *
 * Provides a basic type of Hook that can be reused by both the Graph and Agent modules.
 * Specific modules implement Hooks for specific scenarios by extending these types.
 */

import type { Condition } from "@wf-agent/types";

/**
 * General Hook Definition (Basic Interface)
 *
 * All Hook definitions should extend this interface.
 * The Graph module uses NodeHook, and the Agent module uses AgentHook.
 */
export interface BaseHookDefinition {
  /** Hook type identifier */
  hookType: string;
  /** Trigger condition expression (optional) */
  condition?: Condition;
  /** Event name, used for identification and logging */
  eventName: string;
  /** Event payload template, which supports variable substitution. */
  eventPayload?: Record<string, unknown>;
  /** Whether to enable (default is true) */
  enabled?: boolean;
  /** Weight: The larger the number, the higher the priority, and the earlier it will be executed. */
  weight?: number;
}

/**
 * General Hook Execution Context (Basic Interface)
 *
 * Specific modules need to extend this interface to add context data specific to the scenario.
 * For example:
 * - Graph: Add execution, node, result, etc.
 * - Agent: Add messageHistory, toolCall, iteration, etc.
 */
export interface BaseHookContext {
  /** Execute the ID (for tracking purposes). */
  executionId?: string;
  /** Custom Data (for extension use) */
  [key: string]: unknown;
}

/**
 * Results of the General Hook Execution
 */
export interface HookExecutionResult {
  /** Whether it was successful */
  success: boolean;
  /** Event Name */
  eventName: string;
  /** Execution time (in milliseconds) */
  executionTime: number;
  /** Result data */
  data?: Record<string, unknown>;
  /** Error message */
  error?: Error;
}

/**
 * Hook Executor Configuration
 */
export interface HookExecutorConfig {
  /** Whether to execute in parallel (default is true) */
  parallel?: boolean;
  /** Whether to continue executing subsequent Hooks in the event of an error (default is true) */
  continueOnError?: boolean;
  /** Whether to log a warning when the condition evaluation fails (default is true) */
  warnOnConditionFailure?: boolean;
}

/**
 * Hook processor function types
 *
 * Used to handle specific logic during the execution of hooks, such as:
 * - Creating checkpoints
 * - Sending events
 * - Executing custom processing functions
 */
export type HookHandler<TContext extends BaseHookContext = BaseHookContext> = (
  context: TContext,
  hook: BaseHookDefinition,
  eventData?: Record<string, unknown>,
) => Promise<void>;

/**
 * Event emission function type
 */
export type EventEmitter<TEvent = unknown> = (event: TEvent) => Promise<void>;

/**
 * Context Builder Function Type
 *
 * Used to convert the context of a specific scenario into a generic evaluation context
 */
export type ContextBuilder<TContext extends BaseHookContext = BaseHookContext> = (
  context: TContext,
) => Record<string, unknown>;
