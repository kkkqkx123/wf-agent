/**
 * Validation-Related Error Type Definitions
 * Defining Configuration Validation and Runtime Validation Related Error Types
 */

import { ValidationError, ErrorSeverity } from "./base.js";

/**
 * Configuring Validation Error Options
 */
export interface ConfigurationValidationErrorOptions {
  /** Configuration path */
  configPath?: string;
  /** Configuration type */
  configType?:
    | "workflow"
    | "node"
    | "trigger"
    | "edge"
    | "variable"
    | "tool"
    | "script"
    | "schema"
    | "llm"
    | "hook_template";
  /** field name */
  field?: string;
  /** field value */
  value?: unknown;
  /** additional context */
  context?: Record<string, unknown>;
  /** Severity of error */
  severity?: ErrorSeverity;
}

/**
 * Configuration Validation Error Types
 *
 * Validation errors specialized for static configurations such as workflows, nodes, triggers, etc.
 * Also used for Zod schema validation failures (with configType: "schema").
 * Inherited from ValidationError
 */
export class ConfigurationValidationError extends ValidationError {
  constructor(message: string, options?: ConfigurationValidationErrorOptions) {
    const { configPath, configType, field, value, context, severity } = options || {};
    super(message, field, value, { ...context, configPath, configType }, severity);
  }

  protected override getDefaultSeverity(): ErrorSeverity {
    return "error";
  }
}

/**
 * Runtime Validation Error Options
 */
export interface RuntimeValidationErrorOptions {
  /** Operation Name */
  operation?: string;
  /** field name */
  field?: string;
  /** field value */
  value?: unknown;
  /** additional context */
  context?: Record<string, unknown>;
  /** Severity of error */
  severity?: ErrorSeverity;
}

/**
 * Runtime Validation Error Types
 *
 * Validation errors specialized for runtime parameters and state
 * Inherited from ValidationError
 */
export class RuntimeValidationError extends ValidationError {
  constructor(message: string, options?: RuntimeValidationErrorOptions) {
    const { operation, field, value, context, severity } = options || {};
    super(message, field, value, { ...context, operation }, severity);
  }

  protected override getDefaultSeverity(): ErrorSeverity {
    return "error";
  }
}

/**
 * Expression Security Error
 *
 * Security validation errors for expressions and paths.
 * Used to distinguish security policy violations (expression length, forbidden properties, path format)
 * from regular runtime validation errors (array bounds, undefined variables).
 *
 * Inherited from ValidationError.
 */
export class ExpressionSecurityError extends ValidationError {
  constructor(message: string, options?: RuntimeValidationErrorOptions) {
    const { field, value, context, severity } = options || {};
    const operation = options?.operation ? `security:${options.operation}` : "security";
    super(message, field, value, { ...context, operation }, severity);
  }
}
