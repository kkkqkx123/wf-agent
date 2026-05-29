/**
 * HumanRelay Type Definition
 * Define the core business types associated with Human Relay
 *
 * HumanRelay is a special type of LLM Provider that allows human intervention in the LLM conversation process
 */

import type { ID, Metadata } from "../common.js";
import type { LLMMessage } from "../message/index.js";
import type { LLMToolCall } from "../message/message.js";

/**
 * HumanRelay Request Type
 */
export interface HumanRelayRequest {
  /** Request ID */
  requestId: ID;
  /** Array of messages (with dialog history) */
  messages: LLMMessage[];
  /** Prompt message to the user (used by the application layer for display) */
  prompt: string;
  /** Request timeout in milliseconds */
  timeout: number;
  /** Session ID (used by application layer) */
  sessionId?: string;
  /** Additional operational information */
  metadata?: Metadata;
}

/**
 * HumanRelay Response Type
 *
 * For testing/mock scenarios, the `toolCalls` field can be populated to simulate
 * LLM responses that include function/tool calls. This allows agent loop E2E tests
 * to verify multi-iteration and tool execution flows without a real LLM API.
 */
export interface HumanRelayResponse {
  /** Request ID */
  requestId: ID;
  /** Manually entered message content */
  content: string;
  /** response time stamp */
  timestamp: number;
  /**
   * Mock tool calls to include in the LLM result (for testing only).
   * When set, the HumanRelayClient will pass these through to the LLMResult,
   * enabling agent loop multi-iteration and tool execution tests.
   */
  toolCalls?: LLMToolCall[];
}

/**
 * HumanRelay implementation results
 */
export interface HumanRelayExecutionResult {
  /** Request ID */
  requestId: ID;
  /** Manually entered messages */
  message: LLMMessage;
  /** Execution time (milliseconds) */
  executionTime: number;
}

/**
 * HumanRelay Processor Interface
 * Interface that must be implemented by the application layer to process manual inputs
 */
export interface HumanRelayHandler {
  /**
   * Processing HumanRelay Request
   * @param request HumanRelay Request
   * @returns HumanRelay Response
   */
  handle(request: HumanRelayRequest): Promise<HumanRelayResponse>;
}
