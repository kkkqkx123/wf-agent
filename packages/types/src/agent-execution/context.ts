/**
 * Agent Loop Execution Context Types
 *
 * This module contains execution context and configuration types for Agent Loop:
 * - Runtime configuration (with executable functions)
 * - Execution options (per-run settings)
 * - Execution context (runtime state access)
 *
 * Part of the agent-execution package for runtime-related types.
 *
 * ## Architecture Context
 *
 * This is NOT the execution instance data. The actual execution state is managed by:
 * - `AgentLoopState`: Tracks iteration count, history, streaming state (serializable)
 * - `AgentLoopEntity`: Wraps config + state + runtime managers (conversation, variables)
 *
 * ## Key Characteristics
 *
 * 1. **Immutable Configuration**: Once created, this config should not be modified
 * 2. **Contains Functions**: Includes callback functions that cannot be serialized to JSON/TOML
 * 3. **Re-provided on Restore**: When restoring from checkpoint, this config is re-injected
 * 4. **Used by Factory**: AgentLoopFactory.create() uses this to build AgentLoopEntity
 */

import type { ID } from "../common.js";
import type { Message } from "../message/index.js";
import type { AgentHook } from "./hooks.js";
import type { AgentLoopStatus, AgentLoopResult } from "./types.js";
import type { AgentToolConfig } from "../agent/tool-config.js";
import type { DynamicContextConfig } from "../dynamic-context.js";
import type { AgentTrigger } from "./triggers.js";

// =============================================================================
// Dynamic Prompt Injection (Two-Layer Design)
// =============================================================================

/**
 * Dynamic prompt context input
 *
 * Runtime information for generating dynamic prompts.
 * Provides context needed to generate fresh system and user-level prompts.
 */
export interface DynamicPromptContext {
  /** Current timestamp (ms) for time-based context */
  timestamp?: number;
  /** Number of messages in conversation */
  messageCount?: number;
  /** Current iteration number (for agent loops) */
  currentIteration?: number;
  /** Execution ID */
  executionId?: string;
  /** Additional runtime data */
  metadata?: Record<string, unknown>;
  /** Abort signal for cancellation */
  signal?: AbortSignal;
}

/**
 * Dynamic prompt injection result
 *
 * Contains two types of dynamic prompts:
 *
 * 1. **staticSystem**: Injected into system message (stable, enables KV caching)
 *    - Current time, timezone
 *    - Available tools documentation
 *    - Base system constraints
 *    - Rarely changes during execution → good for caching
 *
 * 2. **dynamicUserContext**: Appended to last user message (variable, no caching)
 *    - TODO lists, task status
 *    - Pinned files content
 *    - Real-time state changes
 *    - Frequently changes → kept out of system message to preserve cache
 *
 * This separation ensures KV cache hits on stable content while allowing
 * frequent updates to user-level context without cache invalidation.
 */
export interface DynamicPromptInjection {
  /** Static system prompt (merged into system message, stable) */
  staticSystem?: string;
  /** Dynamic user context (appended to last user message, variable) */
  dynamicUserContext?: string;
}

/**
 * Transform context function (dynamic prompt generator)
 *
 * Called before each LLM call to generate dynamic prompts.
 *
 * Returns two parts:
 * 1. staticSystem: Stable info (time, tools) → system message → cached ✓
 * 2. dynamicUserContext: Variable info (TODOs, state) → last user message → not cached
 *
 * This design maximizes KV cache hits while allowing frequent updates.
 *
 * @param context Runtime context for generating prompts
 * @returns Dynamic prompt injection with system and user-level parts
 */
export type TransformContextFn = (
  context: DynamicPromptContext,
) => Promise<DynamicPromptInjection>;

// =============================================================================
// Runtime Configuration
// =============================================================================

/**
 * Agent Loop Runtime Configuration
 *
 * This type represents the runtime configuration used to create and customize
 * AgentLoopEntity instances. It defines the behavior and capabilities of an
 * agent loop, including both declarative settings (profileId, maxIterations)
 * and imperative callbacks (transformContext, convertToLlm).
 *
 * Key differences from AgentLoopDefinition:
 * - Contains executable functions (TransformContextFn, ConvertToLlmFn)
 * - Used directly by AgentLoopEntity and AgentLoopCoordinator
 * - Cannot be loaded from configuration files due to function types
 * - Hooks use AgentHook with Condition objects instead of string expressions
 */
export interface AgentLoopRuntimeConfig {
  /** Agent Config ID - unique identifier for the agent configuration */
  agentConfigId?: ID;

  /** LLM Profile ID */
  profileId?: ID;

  /** System prompt (direct string) */
  systemPrompt?: string;

  /** System prompt template ID (has higher priority than systemPrompt) */
  systemPromptTemplateId?: string;

  /** System prompt template variables */
  systemPromptTemplateVariables?: Record<string, unknown>;

  /** Initial user message */
  initialUserMessage?: string;

  /** Maximum iterations (-1 means unlimited) */
  maxIterations?: number;

  /** Initial message list */
  initialMessages?: Message[];

  /**
   * Tool configuration for Agent Loop
   * 
   * Specifies which tools are available during agent loop execution.
   * Uses simplified AgentToolConfig (no initial/dynamic concepts needed).
   */
  availableTools?: AgentToolConfig;

  /** Streaming output or not */
  stream?: boolean;

  /** Whether to create checkpoint at the end */
  createCheckpointOnEnd?: boolean;

  /** Whether to create checkpoint on error */
  createCheckpointOnError?: boolean;

  /** Hook configuration list (with parsed Condition objects) */
  hooks?: AgentHook[];

  /** Trigger configuration list (with parsed Condition objects) */
  triggers?: AgentTrigger[];

  /** Tags for categorization and filtering */
  tags?: string[];

  // ========== Dynamic System Prompt Generation ==========

  /**
   * Generate dynamic system prompt before LLM call
   *
   * Optional function to generate dynamic system prompts before each LLM call.
   * The returned prompt is merged with the base system prompt.
   * Does NOT modify the message history.
   *
   * Use for injecting:
   * - Current time and timezone
   * - Available tools documentation
   * - Environment-specific context
   * - Runtime state information
   */
  transformContext?: TransformContextFn;

  /**
   * Dynamic context configuration
   *
   * Specifies what dynamic content to include during agent execution.
   * These options control which dynamic contexts are injected:
   * - Current time and timezone
   * - TODO lists and pinned files
   * - Environment information
   * - Workspace files
   * - Skills and workflows
   *
   * If not specified, reasonable defaults will be used.
   * CLI options can override these settings.
   */
  dynamicContextConfig?: DynamicContextConfig;
}

// =============================================================================
// Execution Options
// =============================================================================

/**
 * Agent Loop Execution Options
 *
 * Options for starting or resuming an Agent Loop execution.
 * These are per-execution settings, not configuration.
 *
 * Separated from AgentLoopRuntimeConfig because:
 * - RuntimeConfig is about agent behavior/capabilities
 * - ExecutionOptions is about a specific execution run
 */
export interface AgentLoopExecutionOptions {
  /** Initial messages to start the conversation */
  initialMessages?: Message[];

  /** Initial variables for this execution */
  initialVariables?: Record<string, unknown>;

  /** Parent Workflow Execution ID (when executed as Graph node) */
  parentExecutionId?: ID;

  /** Node ID in parent workflow (when executed as Graph node) */
  nodeId?: ID;

  /** Whether to stream output */
  stream?: boolean;

  /** Abort signal for cancellation */
  signal?: AbortSignal;

  /** Custom context data */
  contextData?: Record<string, unknown>;
}

// =============================================================================
// Execution Context
// =============================================================================

/**
 * Agent Loop Execution Context
 *
 * Runtime context available during execution.
 * Provides access to execution state and utilities.
 */
export interface AgentLoopExecutionContext {
  /** Execution ID */
  executionId: ID;

  /** Agent Loop ID */
  agentLoopId: ID;

  /** Current iteration number */
  currentIteration: number;

  /** Total tool calls so far */
  toolCallCount: number;

  /** Whether currently streaming */
  isStreaming: boolean;

  /** Parent execution ID (if any) */
  parentExecutionId?: ID;

  /** Node ID (if any) */
  nodeId?: ID;
}

// =============================================================================
// Run Options & Extended Result
// =============================================================================

/**
 * Agent Loop Run Options
 *
 * High-level options for running an agent loop.
 * Combines configuration and execution options.
 */
export interface AgentLoopRunOptions {
  /** Runtime configuration (or ID to load) */
  configId?: ID;

  /** Execution options */
  execution?: AgentLoopExecutionOptions;

  /** Whether to create checkpoint on completion */
  checkpointOnCompletion?: boolean;

  /** Whether to resume from checkpoint */
  resumeFromCheckpoint?: ID;
}

/**
 * Agent Loop Execution Result (Extended)
 *
 * Extended result with additional execution metadata.
 */
export interface AgentLoopExecutionResult extends AgentLoopResult {
  /** Execution ID */
  executionId: ID;

  /** Final status */
  status: AgentLoopStatus;

  /** Execution duration in milliseconds */
  duration?: number;

  /** Final messages */
  finalMessages?: Message[];

  /** Checkpoint ID (if created) */
  checkpointId?: ID;
}
