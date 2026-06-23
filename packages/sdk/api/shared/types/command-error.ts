/**
 * CommandError - Definition of command error types
 *
 * Provides structured error information, including an error code and context.
 * Designed to work seamlessly with Result pattern from @wf-agent/common-utils
 */

import { SDKError, ErrorSeverity } from "@wf-agent/types";

/**
 * Command Error Base Class
 * Inherits from SDKError, adds support for error codes
 *
 * Design: Errors are treated as values in the Result type system, not exceptions
 * This allows for better error handling and composition.
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
    // Prevent the error from being treated as an exception
    Object.setPrototypeOf(this, CommandError.prototype);
  }

  /**
   * Convert to JSON format for serialization
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

  /**
   * Check if this is a specific error type by code
   * Useful for pattern matching in Result handlers
   */
  isCode(code: string): boolean {
    return this.code === code;
  }
}

/**
 * Validation Error
 * Returned as a Result value when command parameters fail validation
 */
export class CommandValidationError extends CommandError {
  public readonly field?: string;
  public readonly value?: unknown;

  constructor(
    message: string,
    context?: Record<string, unknown>,
    severity?: ErrorSeverity,
    field?: string,
    value?: unknown
  ) {
    super(message, "VALIDATION_ERROR", { ...context, field, value }, severity || "error");
    this.name = "CommandValidationError";
    this.field = field;
    this.value = value;
    Object.setPrototypeOf(this, CommandValidationError.prototype);
  }
}

/**
 * Execution Error
 * Returned as a Result value when an error occurs during command execution
 */
export class CommandExecutionError extends CommandError {
  constructor(message: string, context?: Record<string, unknown>, severity?: ErrorSeverity) {
    super(message, "EXECUTION_ERROR", context, severity || "error");
    this.name = "CommandExecutionError";
    Object.setPrototypeOf(this, CommandExecutionError.prototype);
  }
}

/**
 * Permission Error
 * Returned when user lacks necessary permissions
 */
export class PermissionError extends CommandError {
  constructor(message: string, context?: Record<string, unknown>, severity?: ErrorSeverity) {
    super(message, "PERMISSION_ERROR", context, severity || "error");
    this.name = "PermissionError";
    Object.setPrototypeOf(this, PermissionError.prototype);
  }
}

/**
 * Resource Not Found Error
 * Returned when requested resource does not exist
 */
export class CommandNotFoundError extends CommandError {
  public readonly resourceType?: string;
  public readonly resourceId?: string;

  constructor(
    message: string,
    context?: Record<string, unknown>,
    severity?: ErrorSeverity,
    resourceType?: string,
    resourceId?: string
  ) {
    super(
      message,
      "NOT_FOUND_ERROR",
      { ...context, resourceType, resourceId },
      severity || "error"
    );
    this.name = "CommandNotFoundError";
    this.resourceType = resourceType;
    this.resourceId = resourceId;
    Object.setPrototypeOf(this, CommandNotFoundError.prototype);
  }
}

/**
 * Timeout Error
 * Returned when command execution times out
 */
export class CommandTimeoutError extends CommandError {
  constructor(
    message: string,
    context?: Record<string, unknown>,
    severity?: ErrorSeverity,
    public readonly timeoutMs?: number
  ) {
    super(message, "TIMEOUT_ERROR", { ...context, timeoutMs }, severity || "warning");
    this.name = "CommandTimeoutError";
    Object.setPrototypeOf(this, CommandTimeoutError.prototype);
  }
}

/**
 * Cancelled Error
 * Returned when command is cancelled
 */
export class CancelledError extends CommandError {
  constructor(message: string, context?: Record<string, unknown>, severity?: ErrorSeverity) {
    super(message, "CANCELLED_ERROR", context, severity || "warning");
    this.name = "CancelledError";
    Object.setPrototypeOf(this, CancelledError.prototype);
  }
}

/**
 * State Error
 * Returned when command is executed in an incorrect state
 */
export class StateError extends CommandError {
  constructor(message: string, context?: Record<string, unknown>, severity?: ErrorSeverity) {
    super(message, "STATE_ERROR", context, severity || "error");
    this.name = "StateError";
    Object.setPrototypeOf(this, StateError.prototype);
  }
}

/**
 * Dependency Error
 * Returned when required service or dependency is unavailable
 */
export class DependencyError extends CommandError {
  constructor(message: string, context?: Record<string, unknown>, severity?: ErrorSeverity) {
    super(message, "DEPENDENCY_ERROR", context, severity || "error");
    this.name = "DependencyError";
    Object.setPrototypeOf(this, DependencyError.prototype);
  }
}
