/**
 * Agent Loop Runtime Configuration
 *
 * This type represents the runtime configuration used during Agent Loop execution.
 * It includes both declarative settings (profileId, maxIterations) and imperative
 * functions (transformContext, convertToLlm) that cannot be serialized to config files.
 *
 * For file-based configuration (TOML/JSON), use AgentLoopConfigFile from SDK's config module.
 * The SDK provides transformToAgentLoopConfig() to convert file configs to runtime configs.
 *
 * Key differences from AgentLoopConfigFile:
 * - Contains executable functions (TransformContextFn, ConvertToLlmFn)
 * - Used directly by AgentLoopEntity and AgentLoopCoordinator
 * - Cannot be loaded from configuration files due to function types
 */

import type { ID } from "../common.js";
import type { Message, LLMMessage } from "../message/index.js";
import type { AgentHook } from "./hooks.js";

/**
 * Message Transformation Pipeline
 *
 * Inspired by pi-agent-core's transformContext and convertToLlm pattern.
 * Allows customization of message processing before LLM calls.
 */

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

/**
 * Agent Loop Runtime Configuration
 *
 * This type represents the runtime configuration used to create and customize AgentLoopEntity instances.
 * It defines the behavior and capabilities of an agent loop, including both declarative settings
 * (profileId, maxIterations) and imperative callbacks (transformContext, convertToLlm).
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
 *
 * ## Serialization Strategy
 *
 * - ❌ **NOT serialized to checkpoints**: Functions (transformContext, convertToLlm) can't serialize
 * - ✅ **Only AgentLoopState is serialized**: Contains iteration history, tool calls, etc.
 * - ✅ **Config re-provided on restore**: Application provides config when calling fromCheckpoint()
 *
 * ## Comparison with AgentLoopConfigFile
 *
 * | Aspect | AgentLoopConfigFile | AgentLoopConfig |
 * |--------|-------------------|-----------------|
 * | Location | SDK (sdk/api/shared/config) | packages/types |
 * | Purpose | File-based config (TOML/JSON) | Runtime config with callbacks |
 * | Functions | ❌ No functions | ✅ Contains TransformContextFn, ConvertToLlmFn |
 * | Serializable | ✅ Yes | ❌ No (due to functions) |
 * | Usage | Loaded from files, transformed to AgentLoopConfig | Used directly by AgentLoopFactory |
 *
 * ## Design Rationale
 *
 * Unlike Workflow which separates WorkflowExecution (data) from WorkflowExecutionEntity (wrapper),
 * Agent Loop keeps config and state together in AgentLoopEntity because:
 * 1. Simpler execution model (linear iteration vs graph traversal)
 * 2. Lighter runtime data (iteration count vs node results + variable scopes)
 * 3. No need for separate serializable data object
 *
 * @see AgentLoopConfigFile - File-based configuration format (SDK)
 * @see AgentLoopEntity - Runtime execution instance
 * @see AgentLoopState - Execution state manager (serializable)
 * @see AgentLoopFactory - Factory for creating AgentLoopEntity instances
 */
export interface AgentLoopConfig {
  /** LLM Profile ID */
  profileId?: ID;
  /** System prompt words (direct strings) */
  systemPrompt?: string;
  /** System prompt word template ID (has higher priority than systemPrompt) */
  systemPromptTemplateId?: string;
  /** System prompt word template variables */
  systemPromptTemplateVariables?: Record<string, unknown>;
  /** Initial user message */
  initialUserMessage?: string;
  /** Maximum number of iterations (-1 indicates no limit) */
  maxIterations?: number;
  /** Initial message list */
  initialMessages?: Message[];
  /** List of allowed tools (array of tool IDs) */
  tools?: string[];
  /** Whether to output in streaming format */
  stream?: boolean;
  /** Whether to create a checkpoint at the end */
  createCheckpointOnEnd?: boolean;
  /** Whether to create checkpoints when an error occurs */
  createCheckpointOnError?: boolean;
  /** Hook configuration list */
  hooks?: AgentHook[];

  // ========== Message Transformation Pipeline (NEW) ==========

  /**
   * Transform context before LLM call
   *
   * Optional function to transform the message context before each LLM call.
   * Use for message compression, history pruning, or context injection.
   *
   * @example
   * ```typescript
   * transformContext: async (messages) => {
   *   // Keep only last 20 messages
   *   return messages.slice(-20);
   * }
   * ```
   */
  transformContext?: TransformContextFn;

  /**
   * Convert messages to LLM format
   *
   * Optional function to convert messages to LLM-compatible format.
   * Use for filtering UI-only messages or converting custom types.
   *
   * @example
   * ```typescript
   * convertToLlm: (messages) => {
   *   return messages.filter(m => m.role !== 'notification');
   * }
   * ```
   */
  convertToLlm?: ConvertToLlmFn;
}
