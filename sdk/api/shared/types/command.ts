/**
 * Core Interface of the Command Pattern
 * Defines a unified interface for command execution
 */

import type { ExecutionResult } from "./execution-result.js";
import { SDKError, ExecutionError as SDKExecutionError } from "@wf-agent/types";
import { CommandValidator } from "../utils/command-validator.js";
import { ok, err, isError, now, diffTimestamp } from "@wf-agent/common-utils";

/**
 * Command metadata
 */
export interface CommandMetadata {
  /** Command Name */
  name: string;
  /** Command Description */
  description: string;
  /** Command Classification */
  category: "execution" | "monitoring" | "management";
  /** Is authentication required? */
  requiresAuth: boolean;
  /** Command Version */
  version: string;
}

/**
 * Command validation results
 */
export interface CommandValidationResult {
  /** Did the verification pass? */
  valid: boolean;
  /** Error message list */
  errors: string[];
}

/**
 * Creation of the validation result was successful.
 */
export function validationSuccess(): CommandValidationResult {
  return { valid: true, errors: [] };
}

/**
 * Creation failure verification result
 */
export function validationFailure(errors: string[]): CommandValidationResult {
  return { valid: false, errors };
}

/**
 * Command Interface
 * All commands must implement this interface.
 */
export interface Command<T> {
  /**
   * Execute the command
   * @returns Execution result
   */
  execute(): Promise<ExecutionResult<T>>;

  /**
   * Cancel command (optional)
   * @returns Cancel result
   */
  undo?(): Promise<ExecutionResult<void>>;

  /**
   * Verify command parameters
   * @returns Verification result
   */
  validate(): CommandValidationResult;

  /**
   * Get command metadata
   * @returns Command metadata
   */
  getMetadata(): CommandMetadata;
}

/**
 * Abstract Command Base Class
 * Provides a general implementation for commands
 */
export abstract class BaseCommand<T> implements Command<T> {
  protected readonly startTime: number = now();

  /**
   * Execute the command - unify the error handling entry point
   */
  async execute(): Promise<ExecutionResult<T>> {
    const startTime = now();
    try {
      const result = await this.executeInternal();
      return this.success(result, diffTimestamp(startTime, now()));
    } catch (error) {
      return this.handleError(error, startTime);
    }
  }

  /**
   * Internal Execution Method - The subclass implements the specific execution logic.
   */
  protected abstract executeInternal(): Promise<T>;

  /**
   * Cancel command (not supported by default)
   */
  async undo(): Promise<ExecutionResult<void>> {
    const startTime = now();
    try {
      throw new Error(`Command ${this.getMetadata().name} does not support undo`);
    } catch (error) {
      return this.handleError(error, startTime);
    }
  }

  /**
   * Verify command parameters
   */
  abstract validate(): CommandValidationResult;

  /**
   * Get command metadata
   * Subclasses can override this method to provide more detailed metadata
   */
  getMetadata(): CommandMetadata {
    return {
      name: this.constructor.name,
      description: "",
      category: "execution",
      requiresAuth: false,
      version: "1.0.0.0",
    };
  }

  /**
   * Get the execution time
   */
  protected getExecutionTime(): number {
    return diffTimestamp(this.startTime, now());
  }

  /**
   * Unified error handling method
   * @param error Error object
   * @param startTime Start time
   * @returns Execution result
   */
  protected handleError<T>(error: unknown, startTime: number): ExecutionResult<T> {
    let sdkError: SDKError;

    // If it is already an SDKError (including all its subclasses), use it directly.
    if (error instanceof SDKError) {
      sdkError = error;
    }
    // If it's a regular error, convert it to SDKExecutionError.
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
    // For other types, convert to SDKExecutionError.
    else {
      sdkError = new SDKExecutionError(String(error), undefined, undefined, undefined, undefined);
    }

    // Return a failure result with detailed error information.
    return this.failure(sdkError, diffTimestamp(startTime, now()));
  }

  /**
   * Creation successful.
   */
  protected success<T>(data: T, executionTime: number): ExecutionResult<T> {
    return {
      result: ok(data),
      executionTime,
    };
  }

  /**
   * Creation failed.
   */
  protected failure<T>(error: SDKError, executionTime: number): ExecutionResult<T> {
    return {
      result: err(error),
      executionTime,
    };
  }

  /**
   * Get a validator instance
   * @returns CommandValidator instance
   */
  protected createValidator(): CommandValidator {
    return new CommandValidator();
  }
}

/**
 * Synchronous command interface
 * For commands that do not require asynchronous execution
 */
export interface SyncCommand<T> extends Command<T> {
  executeSync(): ExecutionResult<T>;
}

/**
 * Abstract Synchronous Command Base Class
 */
export abstract class BaseSyncCommand<T> extends BaseCommand<T> implements SyncCommand<T> {
  /**
   * Asynchronous execution - Calling synchronous execution methods
   */
  override async execute(): Promise<ExecutionResult<T>> {
    return this.executeSync();
  }

  /**
   * Synchronous Execution Method - The subclass implements the specific execution logic.
   */
  abstract executeSync(): ExecutionResult<T>;

  /**
   * Internal execution method - Synchronous commands do not require implementation
   */
  protected async executeInternal(): Promise<T> {
    throw new Error("Sync commands should implement executeSync() instead of executeInternal()");
  }
}
