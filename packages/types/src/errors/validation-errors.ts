/**
 * Validation-Related Error Type Definitions
 * Defining Configuration Validation, Runtime Validation, and Schema Validation Related Error Types
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
    | "llm";
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
 * Configuring Validation Error Types
 *
 * Validation errors specialized for static configurations such as workflows, nodes, triggers, etc.
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
 * Schema Validation Error Options
 */
export interface SchemaValidationErrorOptions {
  /** Schema Path */
  schemaPath?: string;
  /** Validation Error List */
  validationErrors?: Array<{ path: string; message: string }>;
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
 * Schema Validation Error Types
 *
 * Specialized for JSON Schema validation failures
 * Inherited from ValidationError
 */
export class SchemaValidationError extends ValidationError {
  constructor(message: string, options?: SchemaValidationErrorOptions) {
    const { schemaPath, validationErrors, field, value, context, severity } = options || {};
    super(message, field, value, { ...context, schemaPath, validationErrors }, severity);
  }

  protected override getDefaultSeverity(): ErrorSeverity {
    return "error";
  }
}
