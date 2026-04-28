/**
 * Type definitions for external executor service
 */

/**
 * Executor status
 */
export type ExecutorStatus = "available" | "unavailable" | "error";

/**
 * Base executor configuration
 */
export interface ExecutorConfig {
  /** Executor name */
  name: string;
  /** Binary name (e.g., "rg", "fd") */
  binaryName: string;
  /** Custom binary path (optional) */
  customPath?: string;
  /** Additional search paths for binary */
  additionalPaths?: string[];
}

/**
 * Executor info
 */
export interface ExecutorInfo {
  /** Executor name */
  name: string;
  /** Binary path */
  binaryPath: string;
  /** Executor status */
  status: ExecutorStatus;
  /** Version info (if available) */
  version?: string;
}

/**
 * Execution options
 */
export interface ExecutionOptions {
  /** Arguments to pass to the binary */
  args: string[];
  /** Maximum output lines (optional) */
  maxLines?: number;
  /** Timeout in milliseconds (optional) */
  timeout?: number;
  /** Working directory (optional) */
  cwd?: string;
  /** Environment variables (optional) */
  env?: Record<string, string>;
}

/**
 * Execution result
 */
export interface ExecutionResult {
  /** Standard output */
  stdout: string;
  /** Standard error */
  stderr: string;
  /** Exit code */
  exitCode: number;
  /** Whether execution was successful */
  success: boolean;
}
