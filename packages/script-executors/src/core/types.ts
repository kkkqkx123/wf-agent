/**
 * Core Type Definition
 */

import type { ScriptType } from "@wf-agent/types";

/**
 * Actuator type
 */
export type ExecutorType =
  | "SHELL" /** Shell Executor */
  | "CMD" /** CMD Executor */
  | "POWERSHELL" /** PowerShell Executor */
  | "PYTHON" /** Python Executor */
  | "JAVASCRIPT"; /** JavaScript Executor */

/**
 * Actuator Configuration
 */
export interface ExecutorConfig {
  /** Actuator type */
  type: ExecutorType;
  /** Whether to enable retry */
  enableRetry?: boolean;
  /** Maximum number of retries */
  maxRetries?: number;
  /** Retry delay (milliseconds) */
  retryDelay?: number;
  /** Whether to use exponential backoff */
  exponentialBackoff?: boolean;
  /** Timeout time (milliseconds) */
  timeout?: number;
}

/**
 * execution context
 */
export interface ExecutionContext {
  /** Execution ID (for execution isolation) */
  executionId?: string;
  /** Job Catalog */
  workingDirectory?: string;
  /** environment variable */
  environment?: Record<string, string>;
  /** abort signal */
  signal?: AbortSignal;
}

/**
 * executable output
 */
export interface ExecutionOutput {
  /** standard output */
  stdout: string;
  /** standard error */
  stderr: string;
  /** exit code */
  exitCode: number;
}

/**
 * Verification results
 */
export interface ValidationResult {
  /** validity */
  valid: boolean;
  /** Error Message Array */
  errors: string[];
}

/**
 * Actuator metadata
 */
export interface ExecutorMetadata {
  /** Actuator type */
  type: ExecutorType;
  /** Actuator name */
  name: string;
  /** releases */
  version: string;
  /** descriptive */
  description?: string;
  /** Supported Script Types */
  supportedScriptTypes: ScriptType[];
}
