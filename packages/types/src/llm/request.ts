/**
 * LLM Request Type Definition
 */

import type { ID } from "../common.js";
import type { Message } from "../message/index.js";
import type { ToolSchema } from "../tool/index.js";

/**
 * LLM Request Type
 */
export interface LLMRequest {
  /** Referenced LLM Profile ID (optional, default configuration used if not provided) */
  profileId?: ID;
  /** message array */
  messages: Message[];
  /** Request Parameters object (overrides parameters in Profile) */
  parameters?: Record<string, unknown>;
  /** Definition of available tools */
  tools?: ToolSchema[];
  /** Streaming or not */
  stream?: boolean;
  /** AbortSignal for interrupt requests */
  signal?: AbortSignal;
}
