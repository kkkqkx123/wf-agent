/**
 * Token utility functions
 *
 * Provide stateless token calculation and validation capabilities
 * Use TokenEstimator for fast token counting (CJK/Latin classification)
 * All functions are pure functions and do not maintain any state
 */

import type { LLMMessage, LLMUsage } from "@wf-agent/types";
import { encodeText, encodeObject } from "../../../utils/token-encoder.js";

/**
 * Estimate the token usage of messages (using TokenEstimator)
 *
 * Counting scope:
 * - content (array of strings or objects)
 * - thinking (thought content, only for the assistant role)
 * - toolCalls (structure of tool calls)
 * - Metadata overhead (4 tokens per message)
 *
 * @param messages: Array of messages
 * @returns: Number of tokens
 */
export function estimateTokens(messages: LLMMessage[]): number {
  let totalTokens = 0;

  for (const message of messages) {
    // Count content
    if (typeof message.content === "string") {
      totalTokens += encodeText(message.content);
    } else if (Array.isArray(message.content)) {
      // Processing array content (commonly used in multimodal content)
      for (const item of message.content) {
        if (typeof item === "string") {
          totalTokens += encodeText(item);
        } else if (typeof item === "object" && item !== null) {
          totalTokens += encodeObject(item);
        }
      }
    }

    // Counting Thinking (Content from Extended Thinking)
    if (message.thinking) {
      totalTokens += encodeText(message.thinking);
    }

    // Counting tool calls (structure of tool invocation)
    if (message.toolCalls && message.toolCalls.length > 0) {
      totalTokens += encodeObject(message.toolCalls);
    }

    // Metadata overhead (about 4 tokens per message)
    totalTokens += 4;
  }

  return totalTokens;
}

/**
 * Get Token usage statistics (prefer using API statistics; otherwise, use local estimates)
 *
 * @param usage: Token usage statistics returned by the API
 * @param messages: Array of messages (used for local estimates)
 * @returns: Number of Tokens
 */
export function getTokenUsage(usage: LLMUsage | null, messages: LLMMessage[]): number {
  // "Prioritize using APIs for statistics."
  if (usage) {
    return usage.totalTokens;
  }

  // Using local estimation methods
  return estimateTokens(messages);
}

/**
 * Check if the number of Tokens used has exceeded the limit.
 *
 * @param tokensUsed: The number of Tokens that have been used
 * @param tokenLimit: The token limit threshold
 * @returns: Whether the limit has been exceeded
 */
export function isTokenLimitExceeded(tokensUsed: number, tokenLimit: number): boolean {
  return tokensUsed > tokenLimit;
}
