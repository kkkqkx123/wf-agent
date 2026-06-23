/**
 * ExecuteScriptCommand - Execute a script command
 *
 * Category: Execution
 * Validates script existence, executes with options, returns execution result
 */

import {
  ExecutionCommand,
  CommandValidationResult,
  validationSuccess,
  validationFailure,
  type CommandMetadataDefinition,
} from "../../types/command.js";
import { validateScriptExecutionParams } from "../validators/shared-validators.js";
import type { ScriptOptions } from "../../types/code-types.js";
import type { ScriptExecutionResult } from "@wf-agent/types";
import type { APIDependencyManager } from "../../core/sdk-dependencies.js";

/**
 * Script execution command parameters
 */
export interface ExecuteScriptParams {
  /** Script name to execute */
  scriptName: string;
  /** Execution options (timeout, retries, environment, etc.) */
  options?: ScriptOptions;
}

/**
 * Execute script command
 */
export class ExecuteScriptCommand extends ExecutionCommand<ScriptExecutionResult> {
  constructor(
    private readonly params: ExecuteScriptParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  protected override getMetadataDefinition(): CommandMetadataDefinition {
    return {
      name: "ExecuteScriptCommand",
      description: "Execute a script with options",
      category: "execution",
      requiresAuth: false,
      version: "1.0.0",
      supportCancellation: true,
      idempotent: false,
    };
  }

  protected async executeInternal(): Promise<ScriptExecutionResult> {
    const executionOptions = {
      timeout: this.params.options?.timeout,
      retries: this.params.options?.retries,
      retryDelay: this.params.options?.retryDelay,
      workingDirectory: this.params.options?.workingDirectory,
      environment: this.params.options?.environment,
      sandbox: this.params.options?.sandbox,
    };

    // Execute the script.
    const result = await this.dependencies
      .getScriptExecutor()
      .execute(this.params.scriptName, executionOptions, this.dependencies.getScriptService());

    // Handle the Result type, either extracting the successful result or throwing an error.
    if (result.isErr()) {
      throw result.error;
    }

    const executionResult: ScriptExecutionResult = {
      ...result.value,
      executionTime: 0,
    };

    return executionResult;
  }

  validate(): CommandValidationResult {
    // Use shared validator for parameter validation
    const result = validateScriptExecutionParams(this.params.scriptName);
    if (!result.valid) {
      return result;
    }

    // Verify the script exists and is valid
    const script = this.dependencies.getScriptService().getScript(this.params.scriptName);
    try {
      this.dependencies.getScriptService().validateScript(script);
    } catch (error) {
      return validationFailure([
        `Script validation failed: ${error instanceof Error ? error.message : String(error)}`,
      ]);
    }

    return validationSuccess();
  }
}
