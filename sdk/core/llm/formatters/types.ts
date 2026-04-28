/**
 * Type Definition for Formatter Class
 *
 * Defines types related to format converters
 */

import type {
  LLMResult,
  LLMProfile,
  LLMUsage,
  LLMToolCall,
  ToolCallFormat,
  ToolCallFormatConfig,
} from "@wf-agent/types";
import type { ToolSchema } from "@wf-agent/types";

/**
 * HTTP Request Options
 */
export interface HttpRequestOptions {
  /** Request URL (relative path or full URL) */
  url: string;
  /** Request Method */
  method?: "GET" | "POST" | "PUT" | "DELETE";
  /** Request headers */
  headers?: Record<string, string>;
  /** Query parameters */
  query?: Record<string, string | number | boolean>;
  /** Request Body */
  body?: unknown;
  /** Request timeout period (in milliseconds) */
  timeout?: number;
}

/**
 * Streaming Response Block
 */
export interface StreamChunk {
  /** Content increment */
  delta?: string;
  /** Is it completed? */
  done: boolean;
  /** Token Usage */
  usage?: LLMUsage;
  /** Reason for completion: */
  finishReason?: string;
  /** Model Version */
  modelVersion?: string;
  /** Raw data */
  raw?: unknown;
  /**
   * Incremental thinking/reasoning content
   *
   * Used for streaming the output of the thought process
   */
  reasoningDelta?: string;
  /**
   * Incremental tool calls (for streaming tool calls)
   * Each chunk may contain partial tool call data that needs to be accumulated
   */
  toolCallsDelta?: LLMToolCall[];
}

/**
 * Authentication Type
 */
export type AuthType = "native" | "bearer";

/**
 * Custom request header configuration
 */
export interface CustomHeader {
  /** Key name */
  key: string;
  /** Value */
  value: string;
  /** Whether to enable */
  enabled?: boolean;
}

/**
 * Custom request body configuration
 */
export interface CustomBodyConfig {
  /** Simple mode: A list of key-value pairs */
  items?: Array<{
    key: string;
    value: string;
    enabled?: boolean;
  }>;
  /** Advanced mode: JSON string */
  json?: string;
  /** Pattern */
  mode?: "simple" | "advanced";
}

/**
 * Format converter configuration
 */
export interface FormatterConfig {
  /** Profile Configuration */
  profile: LLMProfile;
  /** Is it streaming? */
  stream?: boolean;
  /** Tool Definition */
  tools?: ToolSchema[];
  /** Dynamic system prompts */
  dynamicSystemPrompt?: string;
  /** Dynamic context messages */
  dynamicContextMessages?: unknown[];

  // === Migratable API Request Enhancement Features ===

  /** Authentication Type:
- native: Uses the provider's native authentication headers
- bearer: Uses Authorization Bearer */
  authType?: AuthType;
  /** Custom request headers (simplified version: simple key-value pairs) */
  customHeaders?: Record<string, string>;
  /** Custom Request Headers (Full Version: Support for enabling/disabling) */
  customHeadersList?: CustomHeader[];
  /** Custom request body (simplified version: directly merged objects) */
  customBody?: Record<string, unknown>;
  /** Custom Request Body (Full Version: Supports Simple/Advanced Modes) */
  customBodyConfig?: CustomBodyConfig;
  /** Whether to enable a custom request body */
  customBodyEnabled?: boolean;
  /** Request timeout period (in milliseconds) */
  timeout?: number;
  /** Query parameters */
  queryParams?: Record<string, string | number | boolean>;
  /** Stream options */
  streamOptions?: {
    /** Does it contain usage information? */
    includeUsage?: boolean;
  };

  /**
   * Tool call format configuration
   * Provides detailed control over tool calling behavior, including format,
   * custom markers, XML tags, and description style.
   */
  toolCallFormat?: ToolCallFormatConfig;
}

/**
 * Format converter request construction result
 */
export interface BuildRequestResult {
  /** HTTP Request Options */
  httpRequest: HttpRequestOptions;
  /** The request body for debugging. */
  transformedBody?: unknown;
}

/**
 * Format converter responds with the parsing results.
 */
export interface ParseResponseResult {
  /** LLM results */
  result: LLMResult;
  /** Should we proceed with the processing? */
  needsMoreData?: boolean;
}

/**
 * Format converter stream block parsing results
 */
export interface ParseStreamChunkResult {
  /** Flowing Blocks */
  chunk: StreamChunk;
  /** Is it valid? */
  valid: boolean;
}

/**
 * Tool call parse options
 */
export interface ToolCallParseOptions {
  /** Preferred parsing formats in order of priority */
  preferredFormats?: Array<"xml" | "json" | "raw">;
  /** Custom markers for wrapped JSON format */
  markers?: {
    start: string;
    end: string;
  };
  /** Whether to allow partial parsing (return successfully parsed calls even if some fail) */
  allowPartial?: boolean;
}
