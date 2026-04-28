/**
 * Agent Loop configuration type definition
 *
 * Enhanced with message transformation pipeline (inspired by pi-agent-core)
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
 * Agent Loop Configuration
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
