/**
 * ExecuteToolCommand - Execute a tool command
 *
 * Category: Execution
 * Validates tool parameters, executes with options, returns execution result
 */

import {
  ExecutionCommand,
  CommandValidationResult,
  validationSuccess,
  validationFailure,
  type CommandMetadataDefinition,
} from "../../types/command.js";
import { validateToolExecutionParams } from "../validators/shared-validators.js";
import type { ID, ToolExecutionResult } from "@wf-agent/types";
import type { ToolOptions } from "../../resources/tools/tool-registry-api.js";
import type { APIDependencyManager } from "../../core/sdk-dependencies.js";

/**
 * Tool execution command parameters
 */
export interface ExecuteToolParams {
  /** Tool ID to execute */
  toolId: ID;
  /** Tool input parameters */
  parameters: Record<string, unknown>;
  /** Execution options (timeout, retries, etc.) */
  options?: ToolOptions;
}

/**
 * Execute tool command
 */
export class ExecuteToolCommand extends ExecutionCommand<ToolExecutionResult> {
  constructor(
    private readonly params: ExecuteToolParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  protected override getMetadataDefinition(): CommandMetadataDefinition {
    return {
      name: "ExecuteToolCommand",
      description: "Execute a tool with parameters and optional execution settings",
      category: "execution",
      requiresAuth: false,
      version: "1.0.0",
      supportCancellation: true,
      idempotent: false,
    };
  }

  protected async executeInternal(): Promise<ToolExecutionResult> {
    const executionOptions = {
      timeout: this.params.options?.timeout,
      maxRetries: this.params.options?.maxRetries,
      retryDelay: this.params.options?.retryDelay,
      enableLogging: this.params.options?.enableLogging ?? true,
    };

    // Execute Tool
    const result = await this.dependencies
      .getToolService()
      .execute(this.params.toolId, this.params.parameters, executionOptions);

    // Handle the Result type, extracting the successful result or throwing an error.
    if (result.isErr()) {
      throw result.error;
    }

    const executionResult: ToolExecutionResult = {
      success: true,
      result: result.value.result,
      executionTime: 0,
      retryCount: 0,
    };

    return executionResult;
  }

  validate(): CommandValidationResult {
    // Use shared validator for parameter validation
    const result = validateToolExecutionParams(this.params.toolId, this.params.parameters);
    if (!result.valid) {
      return result;
    }

    // Validate tool parameters with the tool service
    const toolValidation = this.dependencies
      .getToolService()
      .validateParameters(this.params.toolId, this.params.parameters);
    if (!toolValidation.valid) {
      return validationFailure(toolValidation.errors);
    }

    return validationSuccess();
  }
}
