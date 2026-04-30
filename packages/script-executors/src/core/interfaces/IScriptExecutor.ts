/**
 * Script Executor Interface
 * Defines the core contracts that all executors must implement
 */

import type {
  Script,
  ScriptType,
  ScriptExecutionOptions,
  ScriptExecutionResult,
} from "@wf-agent/types";
import type { ExecutionContext, ValidationResult } from "../types.js";

/**
 * Script Executor Interface
 */
export interface IScriptExecutor {
  /**
   * Execute the script
   * @param script: Script definition
   * @param options: Execution options
   * @param context: Execution context (such as execution isolation)
   * @returns: Execution result
   */
  execute(
    script: Script,
    options?: ScriptExecutionOptions,
    context?: ExecutionContext,
  ): Promise<ScriptExecutionResult>;

  /**
   * Verify script configuration
   * @param script Script definition
   * @returns Verification result
   */
  validate(script: Script): ValidationResult;

  /**
   * Get the supported script types
   * @returns An array of supported script types
   */
  getSupportedTypes(): ScriptType[];

  /**
   * Clean up resources (optional)
   * @returns Promise
   */
  cleanup?(): Promise<void>;

  /**
   * Get the executor type
   * @returns Executor type as a string
   */
  getExecutorType(): string;
}
