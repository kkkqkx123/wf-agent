/**
 * ExecuteToolCommand - Execute a tool command
 */

import { diffTimestamp, now } from "@wf-agent/common-utils";
import {
  BaseCommand,
  CommandValidationResult,
  validationSuccess,
  validationFailure,
} from "../../types/command.js";
import type { ID } from "@wf-agent/types";
import type { ToolOptions } from "../../resources/tools/tool-registry-api.js";
import type { ToolExecutionResult } from "@wf-agent/types";
import type { APIDependencyManager } from "../../core/sdk-dependencies.js";

/**
 * Execute the tool command.
 */
export class ExecuteToolCommand extends BaseCommand<ToolExecutionResult> {
  constructor(
    private readonly toolId: ID,
    private readonly parameters: Record<string, unknown>,
    private readonly options: ToolOptions | undefined,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  protected async executeInternal(): Promise<ToolExecutionResult> {
    const startTime = now();
    const executionOptions = {
      timeout: this.options?.timeout,
      maxRetries: this.options?.maxRetries,
      retryDelay: this.options?.retryDelay,
      enableLogging: this.options?.enableLogging ?? true,
    };

    // Verify tool parameters
    const validation = this.dependencies
      .getToolService()
      .validateParameters(this.toolId, this.parameters);
    if (!validation.valid) {
      throw new Error(`Parameter validation failed: ${validation.errors.join(", ")}`);
    }

    // Execution Tool
    const result = await this.dependencies
      .getToolService()
      .execute(this.toolId, this.parameters, executionOptions);
    const executionTime = diffTimestamp(startTime, now());

    // Handle the Result type, extracting the successful result or throwing an error.
    if (result.isErr()) {
      throw result.error;
    }

    const executionResult: ToolExecutionResult = {
      success: true,
      result: result.value.result,
      executionTime,
      retryCount: 0,
    };

    return executionResult;
  }

  validate(): CommandValidationResult {
    const errors: string[] = [];

    if (!this.toolId || this.toolId.trim().length === 0) {
      errors.push("The tool ID cannot be empty.");
    }

    if (!this.parameters) {
      errors.push("The parameter cannot be null.");
    }

    return errors.length > 0 ? validationFailure(errors) : validationSuccess();
  }
}
