/**
 * Other Error Type Definitions
 * Define configuration, timeout, tooling and code execution related error types
 */

import { SDKError, ErrorSeverity } from "./base.js";

/**
 * Configuration Error Types
 */
export class ConfigurationError extends SDKError {
  constructor(
    message: string,
    public readonly configKey?: string,
    context?: Record<string, unknown>,
    severity?: ErrorSeverity,
  ) {
    super(message, severity, { ...context, configKey });
  }

  protected override getDefaultSeverity(): ErrorSeverity {
    return "error";
  }
}

/**
 * Timeout Error Type
 */
export class TimeoutError extends SDKError {
  constructor(
    message: string,
    public readonly timeout: number,
    context?: Record<string, unknown>,
    severity?: ErrorSeverity,
  ) {
    super(message, severity, { ...context, timeout });
  }

  protected override getDefaultSeverity(): ErrorSeverity {
    return "warning";
  }
}

/**
 * Tool Call Error Type
 */
export class ToolError extends SDKError {
  constructor(
    message: string,
    public readonly toolId?: string,
    public readonly toolType?: string,
    context?: Record<string, unknown>,
    cause?: Error,
    severity?: ErrorSeverity,
  ) {
    super(message, severity, { ...context, toolId, toolType }, cause);
  }

  protected override getDefaultSeverity(): ErrorSeverity {
    return "warning";
  }
}

/**
 * Script execution error type
 */
export class ScriptExecutionError extends SDKError {
  constructor(
    message: string,
    public readonly scriptName?: string,
    public readonly scriptType?: string,
    context?: Record<string, unknown>,
    cause?: Error,
    severity?: ErrorSeverity,
  ) {
    super(message, severity, { ...context, scriptName, scriptType }, cause);
  }

  protected override getDefaultSeverity(): ErrorSeverity {
    return "error";
  }
}
