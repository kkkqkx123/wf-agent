/**
 * Script Module Type Definitions
 * Define types and interfaces related to the script API
 */

import type { Timestamp } from "@wf-agent/types";

/**
 * Script Filter
 */
export interface ScriptFilter {
  /** List of script IDs */
  ids?: string[];
  /** Script name (fuzzy matching is supported) */
  name?: string;
  /** Classification */
  category?: string;
  /** Tag array */
  tags?: string[];
  /** Whether to enable */
  enabled?: boolean;
}

/**
 * Script Options
 */
export interface ScriptOptions {
  /** Timeout period (in milliseconds) */
  timeout?: number;
  /** Number of retries */
  retries?: number;
  /** Retry delay (in milliseconds) */
  retryDelay?: number;
  /** Working directory */
  workingDirectory?: string;
  /** Environment Variables */
  environment?: Record<string, string>;
  /** Whether to enable the sandbox */
  sandbox?: boolean;
}

/**
 * Script test results
 */
export interface ScriptTestResult {
  /** Script ID */
  scriptId: string;
  /** Script Name */
  scriptName: string;
  /** Test whether it was successful. */
  success: boolean;
  /** Standard output */
  stdout?: string;
  /** Standard Error */
  stderr?: string;
  /** Exit code */
  exitCode?: number;
  /** Error message */
  error?: string;
  /** Execution time (in milliseconds) */
  executionTime: number;
  /** Test timestamp */
  timestamp: Timestamp;
}

/**
 * Script execution log
 */
export interface ScriptExecutionLog {
  /** Log ID */
  id: string;
  /** Script ID */
  scriptId: string;
  /** Script Name */
  scriptName: string;
  /** Did the execution succeed? */
  success: boolean;
  /** Standard output */
  stdout?: string;
  /** Standard Error */
  stderr?: string;
  /** Exit code */
  exitCode?: number;
  /** Execution time (in milliseconds) */
  executionTime: number;
  /** Error message */
  error?: string;
  /** Execution timestamp */
  timestamp: Timestamp;
}

/**
 * Script statistics information
 */
export interface ScriptStatistics {
  /** Script ID */
  scriptId: string;
  /** Script Name */
  scriptName: string;
  /** Total number of executions */
  totalExecutions: number;
  /** Number of successes */
  successCount: number;
  /** Number of failures */
  failureCount: number;
  /** Average execution time (in milliseconds) */
  averageExecutionTime: number;
  /** Last execution time */
  lastExecutionTime?: Timestamp;
  /** Success rate */
  successRate: number;
}

/**
 * Script registration configuration
 */
export interface ScriptRegistrationConfig {
  /** Script ID */
  id?: string;
  /** Script Name */
  name: string;
  /** Script Description */
  description: string;
  /** Script content (inline code) */
  content?: string;
  /** Script file path (external file) */
  filePath?: string;
  /** Script execution options */
  options?: unknown;
  /** Script Metadata */
  metadata?: unknown;
  /** Is it enabled? */
  enabled?: boolean;
}

/**
 * The script executes configurations in batches.
 */
export interface ScriptBatchExecutionConfig {
  /** List of script IDs */
  scriptIds: string[];
  /** Execute the option (override the script's default settings). */
  options?: unknown;
  /** Whether to execute in parallel */
  parallel?: boolean;
  /** The maximum number of concurrent executions during parallel execution */
  maxConcurrency?: number;
}
