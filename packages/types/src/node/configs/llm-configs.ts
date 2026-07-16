/**
 * LLM Node Configuration Type Definition
 */

import type { ID } from '../../common.js';
import type { ToolCallFormatConfig } from '../../llm/tool-call-format.js';

/**
 * LLM Node Output
 * - content: string - The LLM response text content
 * - toolCalls?: Array<{ id: string, name: string, arguments: unknown }> - Any tool calls made by the LLM
 */
export interface LLMNodeOutput {
  content: string;
  toolCalls?: Array<{
    id: string;
    name: string;
    arguments: unknown;
  }>;
}

/**
 * LLM Node Configuration
 *
 * Simplified configuration using a single named message context.
 * Replaces the complex template system with direct context reference.
 */
export interface LLMNodeConfig {
  /** Referenced LLM Profile ID */
  profileId: ID;

  /**
   * Message context ID to pull messages from.
   *
   * Defaults to 'current' if not specified.
   * Can be a built-in ID (e.g., 'current') or a custom ID created by Context Processor.
   */
  contextId?: string;

  /**
   * Optional: Whether to append LLM response to specified context
   *
   * Defaults to 'current' if not specified.
   */
  outputContext?: string;

  /** Optional parameter override (overrides parameters in Profile) */
  parameters?: Record<string, unknown>;

  /** Maximum number of tool calls returned by a single LLM call (default 3, error thrown if exceeded) */
  maxToolCallsPerRequest?: number;

  /**
   * Tool call format configuration.
   * Declares the expected tool call protocol for this LLM node.
   * If set, must be compatible with the referenced LLMProfile.toolCallFormat.
   * If not set, inherits from LLMProfile.toolCallFormat.
   */
  toolCallFormat?: ToolCallFormatConfig;
}