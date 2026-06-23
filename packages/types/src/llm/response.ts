/**
 * LLM Response Type Definition
 */

import type { ID, Timestamp, Metadata } from "../common.js";
import type { LLMMessage, LLMToolCall } from "../message/index.js";
import type { LLMUsage } from "./usage.js";

/**
 * LLM response result types (integrating choices and finishReason)
 */
export interface LLMResult {
  /** Response ID */
  id: ID;
  /** Model name */
  model: string;
  /** Response Content Text */
  content: string;
  /** The complete LLMMessage object */
  message: LLMMessage;
  /** Tool Call Array */
  toolCalls?: LLMToolCall[];
  /** Token usage */
  usage?: LLMUsage;
  /** Reason for completion */
  finishReason: string;
  /** Response time (milliseconds) */
  duration: Timestamp;
  /** response metadata */
  metadata?: Metadata;
  /**
   * Thinking/reasoning content
   *
   * Used to store the thought process or reasoning content of a model
   * - OpenAI: reasoning_content (DeepSeek R1, o1 and other reasoning models)
   * - Anthropic: thinking (Claude extended thinking)
   * - Gemini: thoughts (Gemini thinking models)
   */
  reasoningContent?: string;
  /**
   * Thinking/reasoning about the number of Token
   *
   * Used to differentiate between thinking token and regular token billing
   */
  reasoningTokens?: number;
  /**
   * Streaming statistical information
   *
   * Records performance data during streaming generation
   */
  streamStats?: {
    /** Total number of received streaming blocks */
    chunkCount: number;
    /** Time from request to first packet arrival (milliseconds) */
    timeToFirstChunk: number;
    /** Time from first packet to last packet (milliseconds) */
    streamDuration: number;
    /** Total time (in milliseconds) from request sent to end of stream */
    totalDuration: number;
  };
}
