/**
 * Core type definitions
 */

import type { ToolType } from "@wf-agent/types";

// Re-export ToolType for convenience
export type { ToolType };

/**
 * Executor Configuration
 */
export interface ExecutorConfig {
  /** Executor Type */
  type: ToolType;
  /** Whether to enable retries? */
  enableRetry?: boolean;
  /** Maximum number of retries */
  maxRetries?: number;
  /** Retry delay (in milliseconds) */
  retryDelay?: number;
  /** Whether to use exponential backoff */
  exponentialBackoff?: boolean;
  /** Timeout period (in milliseconds) */
  timeout?: number;
}

/**
 * Executor Metadata
 */
export interface ExecutorMetadata {
  /** Executor Type */
  type: ToolType;
  /** Executor Name */
  name: string;
  /** Version */
  version: string;
  /** Description */
  description?: string;
  /** Supported Tool Types */
  supportedToolTypes: string[];
}
