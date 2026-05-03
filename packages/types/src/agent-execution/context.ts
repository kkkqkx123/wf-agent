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
import type { Message, LLMMessage } from "../message/index.js";
import type { AgentHook } from "./hooks.js";
import type { AgentLoopStatus, AgentLoopResult } from "./types.js";

// =============================================================================
// Message Transformation Pipeline
// =============================================================================

/**
 * Transform context function
 *
 * Called before each LLM call to transform the message context.
 * Use cases:
 * - Message compression/summarization
 * - History pruning (remove old messages)
 * - Context injection (add system context)
 * - Message filtering
 *
 * @param messages Current message context
 * @param signal Abort signal for cancellation
 * @returns Transformed message context
 */
export type TransformContextFn = (
  messages: LLMMessage[],
  signal?: AbortSignal,
) => Promise<LLMMessage[]>;

/**
 * Convert to LLM format function
 *
 * Called to convert AgentMessage[] to LLM Message[].
 * Use cases:
 * - Filter out UI-only messages
 * - Convert custom message types to LLM format
 * - Add provider-specific formatting
 *
 * @param messages Messages to convert
 * @returns Messages in LLM format
 */
export type ConvertToLlmFn = (messages: LLMMessage[]) => LLMMessage[];

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

  /** List of allowed tools (array of tool IDs) */
  tools?: string[];

  /** Streaming output or not */
  stream?: boolean;

  /** Whether to create checkpoint at the end */
  createCheckpointOnEnd?: boolean;

  /** Whether to create checkpoint on error */
  createCheckpointOnError?: boolean;

  /** Hook configuration list (with parsed Condition objects) */
  hooks?: AgentHook[];

  // ========== Message Transformation Pipeline ==========

  /**
   * Transform context before LLM call
   *
   * Optional function to transform the message context before each LLM call.
   * Use for message compression, history pruning, or context injection.
   */
  transformContext?: TransformContextFn;

  /**
   * Convert messages to LLM format
   *
   * Optional function to convert messages to LLM-compatible format.
   * Use for filtering UI-only messages or converting custom types.
   */
  convertToLlm?: ConvertToLlmFn;
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
