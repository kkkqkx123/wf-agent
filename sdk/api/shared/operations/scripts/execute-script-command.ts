/**
 * ExecuteScriptCommand - Execute a script command
 */

import { now, diffTimestamp } from '@wf-agent/common-utils';
import { BaseCommand, CommandValidationResult, validationSuccess, validationFailure } from '../../types/command.js';
import type { ScriptOptions } from '../../types/code-types.js';
import type { ScriptExecutionResult } from '@wf-agent/types';
import type { APIDependencyManager } from '../../core/sdk-dependencies.js';

/**
 * Execute the script command
 */
export class ExecuteScriptCommand extends BaseCommand<ScriptExecutionResult> {
  constructor(
    private readonly scriptName: string,
    private readonly options: ScriptOptions | undefined,
    private readonly dependencies: APIDependencyManager
  ) {
    super();
  }

  protected async executeInternal(): Promise<ScriptExecutionResult> {
    const startTime = now();
    const executionOptions = {
      timeout: this.options?.timeout,
      retries: this.options?.retries,
      retryDelay: this.options?.retryDelay,
      workingDirectory: this.options?.workingDirectory,
      environment: this.options?.environment,
      sandbox: this.options?.sandbox
    };

    // Verify the script exists and is valid
    const script = this.dependencies.getScriptService().getScript(this.scriptName);
    this.dependencies.getScriptService().validateScript(script);

    // Execute the script.
    const result = await this.dependencies.getScriptService().execute(this.scriptName, executionOptions);
    const executionTime = diffTimestamp(startTime, now());

    // Handle the Result type, either extracting the successful result or throwing an error.
    if (result.isErr()) {
      throw result.error;
    }

    const executionResult: ScriptExecutionResult = {
      ...result.value,
      executionTime
    };

    return executionResult;
  }

  validate(): CommandValidationResult {
    const errors: string[] = [];

    if (!this.scriptName || this.scriptName.trim().length === 0) {
      errors.push('Script name cannot be empty');
    }

    return errors.length > 0 ? validationFailure(errors) : validationSuccess();
  }
}
