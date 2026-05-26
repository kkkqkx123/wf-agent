/**
 * HumanRelay Type Definition
 * Define the core business types associated with Human Relay
 *
 * HumanRelay is a special type of LLM Provider that allows human intervention in the LLM conversation process
 */

import type { ID, Metadata } from "../common.js";
import type { LLMMessage } from "../message/index.js";

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
 */
export interface HumanRelayResponse {
  /** Request ID */
  requestId: ID;
  /** Manually entered message content */
  content: string;
  /** response time stamp */
  timestamp: number;
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
