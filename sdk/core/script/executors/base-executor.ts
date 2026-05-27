/**
 * Base Executor
 * Abstract base class for all script execution strategies
 */

import type { ScriptExecutionResult } from "@wf-agent/types";
import type { SandboxConfig, ScriptLanguage } from "@wf-agent/types";

/**
 * Base execution options
 */
export interface BaseExecuteOptions {
  /** Command string to execute */
  command: string;
  /** Working directory */
  cwd?: string;
  /** Environment variables */
  env?: Record<string, string>;
  /** Timeout in milliseconds */
  timeout?: number;
  /** Sandbox configuration (for sandbox executors) */
  sandboxConfig?: SandboxConfig;
  /** Script language (for sandbox routing) */
  language?: ScriptLanguage;
}

/**
 * Abstract base executor
 * Defines the interface that all executors must implement
 */
export abstract class BaseExecutor {
  /**
   * Execute a command
   * @param options Execution options
   * @returns Execution result
   */
  abstract execute(options: BaseExecuteOptions): Promise<ScriptExecutionResult>;

  /**
   * Cleanup resources
   */
  abstract cleanup(): Promise<void>;
}