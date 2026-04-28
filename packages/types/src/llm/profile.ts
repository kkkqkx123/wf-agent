/**
 * LLM profile type definition
 */

import type { ID, Metadata } from "../common.js";
import type { LLMProvider } from "./state.js";
import type { ToolCallFormat, ToolCallFormatConfig } from "./tool-call-format.js";

/**
 * LLM profile type for standalone configuration and multiplexing
 */
export interface LLMProfile {
  /** Profile Unique Identifier */
  id: ID;
  /** Profile Name */
  name: string;
  /** LLM Providers */
  provider: LLMProvider;
  /** Model name */
  model: string;
  /** API key */
  apiKey: string;
  /** Optional base URL (for third-party API channels) */
  baseUrl?: string;
  /** Model parameter objects (temperature, maxTokens, etc., not forced type) */
  parameters: Record<string, unknown>;
  /** Custom HTTP request headers (for 3rd party API channels) */
  headers?: Record<string, string>;
  /** Timeout time (milliseconds) */
  timeout?: number;
  /** Maximum number of retries */
  maxRetries?: number;
  /** Retry delay (milliseconds) */
  retryDelay?: number;
  /** Optional metadata */
  metadata?: Metadata;

  /**
   * Tool call format configuration
   * Provides detailed control over tool calling behavior, including format,
   * custom markers, XML tags, and description style.
   */
  toolCallFormat?: ToolCallFormatConfig;
}
