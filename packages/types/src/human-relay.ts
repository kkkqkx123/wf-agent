/**
 * HumanRelay Type Definition
 * Define the core business types associated with Human Relay
 *
 * HumanRelay is a special type of LLM Provider that allows human intervention in the LLM conversation process
 */

import type { ID, Metadata } from "./common.js";
import type { LLMMessage } from "./message/index.js";

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
 * HumanRelay Context
 * The execution context provided by the SDK to the application layer.
 */
export interface HumanRelayContext {
  /** Execution ID */
  executionId: ID;
  /** Workflow ID */
  workflowId: ID;
  /** Node ID */
  nodeId: ID;
  /** Getting the value of a variable */
  getVariable(variableName: string): unknown;
  /** Setting variable values */
  setVariable(variableName: string, value: unknown): Promise<void>;
  /** Get all variables */
  getVariables(): Record<string, unknown>;
  /** timeout control */
  timeout: number;
  /** Cancel Token */
  cancelToken: {
    cancelled: boolean;
    cancel(): void;
  };
}

/**
 * HumanRelay Processor Interface
 * Interface that must be implemented by the application layer to process manual inputs
 */
export interface HumanRelayHandler {
  /**
   * 处理 HumanRelay 请求
   * @param request HumanRelay 请求
   * @param context HumanRelay 上下文
   * @returns HumanRelay 响应
   */
  handle(request: HumanRelayRequest, context: HumanRelayContext): Promise<HumanRelayResponse>;
}
