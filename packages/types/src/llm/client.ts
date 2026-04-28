/**
 * LLM Client Type Definition
 */

import type { LLMProvider } from "./state.js";
import type { LLMRequest } from "./request.js";
import type { LLMResult } from "./response.js";
import type { TokenCountResult } from "./usage.js";

/**
 * LLM client interface
 */
export interface LLMClient {
  /**
   * non-streaming
   */
  generate(request: LLMRequest): Promise<LLMResult>;

  /**
   * streaming generation
   */
  generateStream(request: LLMRequest): AsyncIterable<LLMResult>;

  /**
   * Counting the number of Token
   * Calling the LLM provider's Token counting API
   * @param request LLM request
   * @returns Token counting result
   */
  countTokens?(request: LLMRequest): Promise<TokenCountResult>;
}

/**
 * LLM client configuration type
 */
export interface LLMClientConfig {
  /** LLM Providers */
  provider: LLMProvider;
  /** API key */
  apiKey: string;
  /** Base URL */
  baseUrl?: string;
  /** timeout */
  timeout?: number;
  /** Maximum number of retries */
  maxRetries?: number;
  /** retry delay */
  retryDelay?: number;
}
