/**
 * Script Module Type Definition
 * Defines basic information and configuration for script execution
 */

import type { ID, Metadata } from "../common.js";
import type { ScriptArgument } from "./script-argument.js";
import type { ScriptExecutorConfig, ExecutorMode } from "./script-executor.js";
import type { ScriptLanguage, SandboxConfig } from "./script-sandbox.js";

/**
 * Script execution options
 */
export interface ScriptExecutionOptions {
  /** Timeout time (milliseconds) */
  timeout?: number;
  /** Retries */
  retries?: number;
  /** Retry delay (milliseconds) */
  retryDelay?: number;
  /** Whether to enable exponential backoff */
  exponentialBackoff?: boolean;
  /** Job Catalog */
  workingDirectory?: string;
  /** environment variable */
  environment?: Record<string, string>;
  /** Whether to enable sandboxing */
  sandbox?: boolean;
  /** Sandbox Configuration */
  sandboxConfig?: SandboxConfig;
  /** Abort signal (for canceling execution) */
  signal?: AbortSignal;
  /** Executor mode (direct/shared/pty/sandbox-*) */
  executorMode?: ExecutorMode;
  /** Script language for sandbox routing */
  language?: ScriptLanguage;
}

/**
 * Script execution results
 */
export interface ScriptExecutionResult {
  /** Successful implementation */
  success: boolean;
  /** screenplay title */
  scriptName: string;
  /** standard output */
  stdout?: string;
  /** standard error */
  stderr?: string;
  /** exit code */
  exitCode?: number;
  /** Execution time (milliseconds) */
  executionTime: number;
  /** error message */
  error?: string;
  /** Implementation of environmental information */
  environment?: Record<string, unknown>;
  /** Retries */
  retryCount?: number;
}

/**
 * Script Definition
 */
export interface Script {
  /** Script Unique Identifier */
  id: ID;
  /** screenplay title */
  name: string;
  /** Script Description */
  description: string;
  /** Script content (inline code) */
  content?: string;
  /** Script file path (external file) */
  filePath?: string;
  /** Command template with {{var}} placeholders */
  template?: string;
  /** Parameter declarations for the template */
  arguments?: ScriptArgument[];
  /** Executor configuration (mode, shell type, etc.) */
  executor?: ScriptExecutorConfig;
  /** Script execution options */
  options: ScriptExecutionOptions;
  /** Script language (auto-detect if omitted) */
  language?: ScriptLanguage;
  /** Script Metadata */
  metadata?: ScriptMetadata;
  /** Enable or not (default is true) */
  enabled?: boolean;
}

/**
 * Script Metadata
 */
export interface ScriptMetadata {
  /** Script Category */
  category?: string;
  /** tagged array */
  tags?: string[];
  /** author */
  author?: string;
  /** releases */
  version?: string;
  /** Document URL */
  documentationUrl?: string;
  /** Custom Fields */
  customFields?: Metadata;
}
