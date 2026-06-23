/**
 * Core Interface of the Command Pattern
 * Defines a unified interface for command execution with improved architecture
 *
 * Three-Layer Architecture:
 * 1. Command Classification: ExecutionCommand, ManagementCommand, QueryCommand, StreamingCommand
 * 2. Parameter Management: Unified params interface, early validation
 * 3. Dependency Resolution: Single DependencyManager pattern with type-safe accessors
 */

import type { ExecutionResult } from "./execution-result.js";
import { SDKError, ExecutionError as SDKExecutionError } from "@wf-agent/types";
import { ok, err, isError, now, diffTimestamp } from "@wf-agent/common-utils";

/**
 * Extract meaningful error message from unknown error type
 * Prevents [object Object] error messages in error handling
 */
function extractErrorMessage(error: unknown): string {
  if (typeof error === 'string') return error;
  if (error && typeof error === 'object' && 'message' in error) {
    return (error as any).message || '';
  }
  try {
    return JSON.stringify(error);
  } catch {
    return String(error);
  }
}

/**
 * Extended command metadata with support information
 */
export interface CommandMetadataDefinition {
  /** Command Name */
  name: string;
  /** Command Description */
  description: string;
  /** Command Classification: execution (long-running), management (CRUD), query (read-only) */
  category: "execution" | "management" | "query";
  /** Is authentication required? */
  requiresAuth: boolean;
  /** Command Version */
  version: string;
  /** Support cancellation for execution commands */
  supportCancellation?: boolean;
  /** Support undo/redo for management commands */
  supportUndo?: boolean;
  /** Estimated duration in milliseconds */
  estimatedDuration?: number;
  /** Is the command idempotent? */
  idempotent?: boolean;
}

/**
 * Command metadata (runtime representation)
 */
export interface CommandMetadata extends CommandMetadataDefinition {}

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
   * Cancel command (optional) - for management commands with undo support
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
 * Provides unified error handling and execution pipeline for all commands
 */
export abstract class BaseCommand<T> implements Command<T> {
  protected readonly startTime: number = now();

  /**
   * Execute the command - unified error handling entry point
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
   * Internal Execution Method - Subclass implements specific execution logic.
   * All validation should be done in validate() method, NOT here.
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
   * All parameter validation MUST be done here, not in executeInternal()
   */
  abstract validate(): CommandValidationResult;

  /**
   * Get command metadata definition
   * Each concrete command should override this with its specific metadata
   */
  protected getMetadataDefinition(): CommandMetadataDefinition {
    return {
      name: this.constructor.name,
      description: "No description provided",
      category: "execution",
      requiresAuth: false,
      version: "1.0.0",
    };
  }

  /**
   * Get command metadata (final implementation)
   */
  getMetadata(): CommandMetadata {
    return this.getMetadataDefinition();
  }

  /**
   * Get the execution time
   */
  protected getExecutionTime(): number {
    return diffTimestamp(this.startTime, now());
  }

  /**
   * Unified error handling method
   * Extracts meaningful error messages to prevent [object Object] errors
   */
  protected handleError<T>(error: unknown, startTime: number): ExecutionResult<T> {
    let sdkError: SDKError;

    if (error instanceof SDKError) {
      sdkError = error;
    } else if (isError(error)) {
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
    } else {
      // Extract meaningful error message to prevent [object Object]
      const message = extractErrorMessage(error);
      sdkError = new SDKExecutionError(message, undefined, undefined, undefined, undefined);
    }

    return this.failure(sdkError, diffTimestamp(startTime, now()));
  }

  /**
   * Create successful result
   */
  protected success<T>(data: T, executionTime: number): ExecutionResult<T> {
    return {
      result: ok(data),
      executionTime,
    };
  }

  /**
   * Create failed result
   */
  protected failure<T>(error: SDKError, executionTime: number): ExecutionResult<T> {
    return {
      result: err(error),
      executionTime,
    };
  }
}

/**
 * Execution Command Base Class
 * For long-running operations like agent loops and workflow execution
 * Supports: cancellation, streaming, progress tracking
 */
export abstract class ExecutionCommand<T> extends BaseCommand<T> {
  /**
   * Override metadata with execution-specific defaults
   */
  protected override getMetadataDefinition(): CommandMetadataDefinition {
    return {
      ...super.getMetadataDefinition(),
      category: "execution",
      supportCancellation: true,
    };
  }
}

/**
 * Management Command Base Class
 * For CRUD operations and resource management
 * Supports: undo/redo (if implemented), transaction semantics
 */
export abstract class ManagementCommand<T> extends BaseCommand<T> {
  /**
   * Override metadata with management-specific defaults
   */
  protected override getMetadataDefinition(): CommandMetadataDefinition {
    return {
      ...super.getMetadataDefinition(),
      category: "management",
      supportUndo: false, // Subclasses can override if they support undo
    };
  }

  /**
   * Override undo to provide meaningful error if not supported
   */
  override async undo(): Promise<ExecutionResult<void>> {
    if (!this.getMetadata().supportUndo) {
      const startTime = now();
      try {
        throw new Error(
          `Command ${this.getMetadata().name} does not support undo. ` +
          `Set supportUndo: true in metadata if undo is implemented.`,
        );
      } catch (error) {
        return this.handleError(error, startTime);
      }
    }
    return super.undo();
  }
}

/**
 * Query Command Base Class
 * For read-only operations
 * Features: caching, no undo support
 */
export abstract class QueryCommand<T> extends BaseCommand<T> {
  /**
   * Override metadata with query-specific defaults
   */
  protected override getMetadataDefinition(): CommandMetadataDefinition {
    return {
      ...super.getMetadataDefinition(),
      category: "query",
      idempotent: true,
    };
  }
}

/**
 * Streaming Command Base Class
 * For operations that return AsyncGenerator (streaming results)
 * Example: ExecuteWorkflowStreamCommand, RunAgentLoopStreamCommand
 */
export abstract class StreamingCommand<T> extends ExecutionCommand<T> {
  /**
   * Streaming-specific metadata
   */
  protected override getMetadataDefinition(): CommandMetadataDefinition {
    return {
      ...super.getMetadataDefinition(),
      supportCancellation: true,
    };
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
