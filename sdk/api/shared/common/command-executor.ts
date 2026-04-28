/**
 * Command executor
 * Responsible for executing commands and managing the middleware chain
 */

import type { Command, CommandValidationResult } from "../types/command.js";
import type { ExecutionResult } from "../types/execution-result.js";
import { failure } from "../types/execution-result.js";
import { SDKError, ExecutionError as SDKExecutionError, ValidationError } from "@wf-agent/types";
import { isError } from "@wf-agent/common-utils";

/**
 * Command Actuator
 */
export class CommandExecutor {
  /**
   * Execute command
   * @param command command
   * @returns Execution results
   */
  async execute<T>(command: Command<T>): Promise<ExecutionResult<T>> {
    // Verify Command
    const validation: CommandValidationResult = command.validate();
    if (!validation.valid) {
      return failure<T>(
        new ValidationError(
          `Validation failed: ${validation.errors.join(", ")}`,
          undefined,
          undefined,
          { errors: validation.errors },
        ),
        0,
      );
    }

    // execute a command
    try {
      return await command.execute();
    } catch (error) {
      let sdkError: SDKError;

      // If it is already SDKError (including all subclasses), it is straightforward to use the
      if (error instanceof SDKError) {
        sdkError = error;
      }
      // If it's a normal Error, it's converted to SDKExecutionError.
      else if (isError(error)) {
        sdkError = new SDKExecutionError(
          error.message,
          undefined,
          undefined,
          {
            originalError: error.name,
            stack: error.stack,
          },
          error,
        );
      }
      // Other types, converted to SDKExecutionError
      else {
        sdkError = new SDKExecutionError(String(error));
      }

      return failure<T>(sdkError, 0);
    }
  }
}
