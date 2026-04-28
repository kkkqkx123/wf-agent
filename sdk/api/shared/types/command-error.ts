/**
 * CommandError - Definition of command error types
 *
 * Provides structured error information, including an error code and context.
 */

import { SDKError, ErrorSeverity } from "@wf-agent/types";

/**
 * Command Error Base Class
 * Inherits from SDKError, adds support for error codes
 */
export class CommandError extends SDKError {
  constructor(
    message: string,
    public readonly code: string,
    context?: Record<string, unknown>,
    severity?: ErrorSeverity,
  ) {
    super(message, severity, { ...context, code });
    this.name = "CommandError";
  }

  /**
   * Translate from "Convert to JSON format" to English:
   */
  override toJSON(): Record<string, unknown> {
    return {
      name: this.name,
      message: this.message,
      code: this.code,
      severity: this.severity,
      context: this.context,
      stack: this.stack,
    };
  }
}

/**
 * Validation Error
 * A validation error is thrown when the command parameters fail to pass the validation.
 */
export class CommandValidationError extends CommandError {
  constructor(message: string, context?: Record<string, unknown>, severity?: ErrorSeverity) {
    super(message, "VALIDATION_ERROR", context, severity);
    this.name = "CommandValidationError";
  }
}

/**
 * Execution Error
 * An error is thrown when an error occurs during the execution of a command.
 */
export class CommandExecutionError extends CommandError {
  constructor(message: string, context?: Record<string, unknown>, severity?: ErrorSeverity) {
    super(message, "EXECUTION_ERROR", context, severity);
    this.name = "CommandExecutionError";
  }
}

/**
 * Permission Error
 * This error is thrown when a user does not have the necessary permissions to execute the command.
 */
export class PermissionError extends CommandError {
  constructor(message: string, context?: Record<string, unknown>, severity?: ErrorSeverity) {
    super(message, "PERMISSION_ERROR", context, severity);
    this.name = "PermissionError";
  }
}

/**
 * Resource Not Found Error
 * This error is thrown when the requested resource does not exist.
 */
export class CommandNotFoundError extends CommandError {
  constructor(message: string, context?: Record<string, unknown>, severity?: ErrorSeverity) {
    super(message, "NOT_FOUND_ERROR", context, severity);
    this.name = "CommandNotFoundError";
  }
}

/**
 * Timeout Error
 * This error is thrown when a command execution times out.
 */
export class CommandTimeoutError extends CommandError {
  constructor(message: string, context?: Record<string, unknown>, severity?: ErrorSeverity) {
    super(message, "TIMEOUT_ERROR", context, severity);
    this.name = "CommandTimeoutError";
  }
}

/**
 * Cancel an error
 * An error is thrown when the command is canceled.
 */
export class CancelledError extends CommandError {
  constructor(message: string, context?: Record<string, unknown>, severity?: ErrorSeverity) {
    super(message, "CANCELLED_ERROR", context, severity);
    this.name = "CancelledError";
  }
}

/**
 * Status Error
 * An error is thrown when the command is executed in an incorrect state.
 */
export class StateError extends CommandError {
  constructor(message: string, context?: Record<string, unknown>, severity?: ErrorSeverity) {
    super(message, "STATE_ERROR", context, severity);
    this.name = "StateError";
  }
}

/**
 * Dependency Error
 * This error is thrown when the service that the command depends on is unavailable.
 */
export class DependencyError extends CommandError {
  constructor(message: string, context?: Record<string, unknown>, severity?: ErrorSeverity) {
    super(message, "DEPENDENCY_ERROR", context, severity);
    this.name = "DependencyError";
  }
}
