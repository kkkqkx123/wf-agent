/**
 * The tool implements the relevant type definitions
 */

import type { ID, Timestamp } from "../common.js";

/**
 * Tool Call Record Type
 */
export interface ToolCall {
  /** Tool Call ID */
  id: ID;
  /** Tool ID */
  toolId: ID;
  /** Tool name */
  toolName?: string;
  /** Tool parameters */
  parameters: Record<string, unknown>;
  /** invocation result */
  result?: unknown;
  /** error message */
  error?: unknown;
  /** invocation time */
  timestamp: Timestamp;
  /** Execution time (milliseconds) */
  executionTime?: Timestamp;
}

/**
 * Tool implementation options
 */
export interface ToolExecutionOptions {
  /** Timeout time (milliseconds) */
  timeout?: number;
  /** Maximum number of retries */
  retries?: number;
  /** Retry delay (milliseconds) */
  retryDelay?: number;
  /** Whether to enable exponential backoff */
  exponentialBackoff?: boolean;
  /** Abort signal (for canceling execution) */
  signal?: AbortSignal;
}

/**
 * Tool execution results (SDK internal use)
 * Contains complete execution metadata for internal streaming
 */
export interface ToolExecutionResult {
  /** success or failure */
  success: boolean;
  /** Implementation results (raw data) */
  result?: unknown;
  /** error message */
  error?: string;
  /** Execution time (milliseconds) */
  executionTime: number;
  /** Retries */
  retryCount: number;
}

/**
 * Tool output (used by the tool implementation layer)
 * Tool execute function return value type
 */
export interface ToolOutput {
  /** success or failure */
  success: boolean;
  /** Output content (can be of any type, will eventually be serialized as a string and sent to LLM) */
  content: unknown;
  /** error message */
  error?: string;
}
