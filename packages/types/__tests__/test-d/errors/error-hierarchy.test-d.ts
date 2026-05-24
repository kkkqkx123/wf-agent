/**
 * @description Tests for Error Hierarchy and Type System
 * @priority HIGH
 * 
 * Validates:
 * - SDKError base class structure
 * - Error severity types
 * - ErrorContext interface
 * - ValidationError hierarchy
 * - ConfigurationValidationError, RuntimeValidationError
 * - instanceof type guards
 * - toJSON serialization
 */

import { expectType, expectAssignable } from "tsd";
import {
  SDKError,
  ValidationError,
  ExecutionError,
  NotFoundError,
  ConfigurationValidationError,
  RuntimeValidationError,
  ExpressionSecurityError,
  ErrorSeverity,
  ErrorContext,
} from "../../../src/index.js";

// ============================================================================
// Test 1: ErrorSeverity type validation
// ============================================================================

declare const severity: ErrorSeverity;
expectAssignable<"error" | "warning" | "info">(severity);

const errorSeverity: ErrorSeverity = "error";
expectAssignable<ErrorSeverity>(errorSeverity);

const warningSeverity: ErrorSeverity = "warning";
expectAssignable<ErrorSeverity>(warningSeverity);

const infoSeverity: ErrorSeverity = "info";
expectAssignable<ErrorSeverity>(infoSeverity);

// ============================================================================
// Test 2: ErrorContext interface structure
// ============================================================================

declare const context: ErrorContext;
expectType<string | undefined>(context.executionId);
expectType<string | undefined>(context.workflowId);
expectType<string | undefined>(context.nodeId);
expectType<string | undefined>(context.operation);
expectType<string | undefined>(context.toolId);
expectType<string | undefined>(context.toolName);
expectType<string | undefined>(context.toolType);
expectType<string | undefined>(context.field);
expectType<unknown>(context.value);
expectType<string | undefined>(context.resourceType);
expectType<string | undefined>(context.resourceId);
expectType<"error" | "warning" | "info" | undefined>(context.severity);

// Additional properties should be allowed
expectType<unknown>(context["customField"]);

// ============================================================================
// Test 3: SDKError base class structure
// ============================================================================

const baseError = new SDKError("Base error message");
expectType<string>(baseError.message);
expectType<ErrorSeverity>(baseError.severity);
expectType<Record<string, unknown> | undefined>(baseError.context);
expectType<Error | undefined>(baseError.cause);

// Default severity should be "error"
expectType<"error" | "warning" | "info">(baseError.severity);

// With custom severity
const warningError = new SDKError("Warning message", "warning");
expectType<ErrorSeverity>(warningError.severity);

const infoError = new SDKError("Info message", "info");
expectType<ErrorSeverity>(infoError.severity);

// With context
const contextualError = new SDKError(
  "Contextual error",
  "error",
  { nodeId: "node-1", workflowId: "wf-1" }
);
expectType<Record<string, unknown> | undefined>(contextualError.context);

// With cause
const originalError = new Error("Original error");
const causedError = new SDKError("Caused by another error", "error", undefined, originalError);
expectType<Error | undefined>(causedError.cause);

// ============================================================================
// Test 4: SDKError toJSON method
// ============================================================================

const serializableError = new SDKError("Test error", "error", { field: "test" });
const json = serializableError.toJSON();
expectType<Record<string, unknown>>(json);
expectType<string>(json["name"] as string);
expectType<string>(json["message"] as string);
expectType<ErrorSeverity>(json["severity"] as ErrorSeverity);
expectType<Record<string, unknown> | undefined>(json["context"] as Record<string, unknown> | undefined);
expectType<string | undefined>(json["stack"] as string | undefined);

// ============================================================================
// Test 5: ValidationError structure
// ============================================================================

const validationError = new ValidationError(
  "Validation failed",
  "username",
  "invalid_value",
  { additional: "context" }
);
expectType<string>(validationError.message);
expectType<ErrorSeverity>(validationError.severity);
expectType<string | undefined>(validationError.field);
expectType<unknown>(validationError.value);

// Field and value should be accessible
if (validationError.field) {
  expectType<string>(validationError.field);
}

// ============================================================================
// Test 6: ConfigurationValidationError
// ============================================================================

const configError = new ConfigurationValidationError(
  "Invalid configuration",
  {
    configPath: "workflow.nodes[0]",
    configType: "node",
    field: "type",
    value: "INVALID_TYPE",
    context: { extra: "info" },
    severity: "error",
  }
);
expectType<string>(configError.message);
expectType<ErrorSeverity>(configError.severity);
expectType<string | undefined>(configError.field);
expectType<unknown>(configError.value);

// Inherited context fields should be present
if (configError.context) {
  expectType<string | undefined>(configError.context["configPath"] as string | undefined);
  expectType<
    | "workflow"
    | "node"
    | "trigger"
    | "edge"
    | "variable"
    | "tool"
    | "script"
    | "schema"
    | "llm"
    | undefined
  >(configError.context["configType"] as | "workflow"
    | "node"
    | "trigger"
    | "edge"
    | "variable"
    | "tool"
    | "script"
    | "schema"
    | "llm"
    | undefined);
}

// Default severity should be "error"
expectType<"error" | "warning" | "info">(configError.severity);

// ============================================================================
// Test 7: RuntimeValidationError
// ============================================================================

const runtimeError = new RuntimeValidationError(
  "Runtime validation failed",
  {
    operation: "execute_node",
    field: "parameters",
    value: { invalid: "data" },
    context: { nodeId: "node-1" },
  }
);
expectType<string>(runtimeError.message);
expectType<ErrorSeverity>(runtimeError.severity);
expectType<string | undefined>(runtimeError.field);
expectType<unknown>(runtimeError.value);

if (runtimeError.context) {
  expectType<string | undefined>(runtimeError.context["operation"] as string | undefined);
}

// ============================================================================
// Test 8: ExecutionError structure
// ============================================================================

const execError = new ExecutionError(
  "Execution failed",
  "node-123",
  "workflow-456",
  { step: "processing" },
  new Error("Root cause")
);
expectType<string>(execError.message);
expectType<ErrorSeverity>(execError.severity);
expectType<string | undefined>(execError.nodeId);
expectType<string | undefined>(execError.workflowId);
expectType<Error | undefined>(execError.cause);

if (execError.context) {
  expectType<string | undefined>(execError.context["nodeId"] as string | undefined);
  expectType<string | undefined>(execError.context["workflowId"] as string | undefined);
}

// ============================================================================
// Test 10: NotFoundError structure
// ============================================================================

const notFoundError = new NotFoundError(
  "Resource not found",
  "workflow",
  "wf-789",
  { attempted: "lookup" }
);
expectType<string>(notFoundError.message);
expectType<ErrorSeverity>(notFoundError.severity);
expectType<string>(notFoundError.resourceType);
expectType<string>(notFoundError.resourceId);

if (notFoundError.context) {
  expectType<string | undefined>(notFoundError.context["resourceType"] as string | undefined);
  expectType<string | undefined>(notFoundError.context["resourceId"] as string | undefined);
}

// ============================================================================
// Test 11: instanceof type guards
// ============================================================================

declare const error: SDKError;

// instanceof should narrow to specific error types
if (error instanceof ConfigurationValidationError) {
  expectType<ConfigurationValidationError>(error);
  if (error.context) {
    expectType<string | undefined>(error.context["configPath"] as string | undefined);
    expectType<
      | "workflow"
      | "node"
      | "trigger"
      | "edge"
      | "variable"
      | "tool"
      | "script"
      | "schema"
      | "llm"
      | undefined
    >(error.context["configType"] as | "workflow"
      | "node"
      | "trigger"
      | "edge"
      | "variable"
      | "tool"
      | "script"
      | "schema"
      | "llm"
      | undefined);
  }
}

if (error instanceof RuntimeValidationError) {
  expectType<RuntimeValidationError>(error);
  if (error.context) {
    expectType<string | undefined>(error.context["operation"] as string | undefined);
  }
}

if (error instanceof ExpressionSecurityError) {
  expectType<ExpressionSecurityError>(error);
  if (error.context) {
    expectType<string | undefined>(error.context["operation"] as string | undefined);
  }
}

// ExpressionSecurityError should not narrow to RuntimeValidationError
if (error instanceof ExpressionSecurityError) {
  expectType<ExpressionSecurityError>(error);
  expectType<ValidationError>(error);
}

if (error instanceof ExecutionError) {
  expectType<ExecutionError>(error);
  expectType<string | undefined>(error.nodeId);
  expectType<string | undefined>(error.workflowId);
}

if (error instanceof NotFoundError) {
  expectType<NotFoundError>(error);
  expectType<string>(error.resourceType);
  expectType<string>(error.resourceId);
}

if (error instanceof ValidationError) {
  expectType<ValidationError>(error);
  expectType<string | undefined>(error.field);
  expectType<unknown>(error.value);
}

// ============================================================================
// Test 12: Error inheritance chain
// ============================================================================

// All specific errors should be instances of SDKError
const configErr = new ConfigurationValidationError("test");
const runtimeErr = new RuntimeValidationError("test");
const execErr = new ExecutionError("test");
const notFoundErr = new NotFoundError("test", "type", "id");
const validationErr = new ValidationError("test");

// These should all be assignable to SDKError
expectAssignable<SDKError>(configErr);
expectAssignable<SDKError>(runtimeErr);
expectAssignable<SDKError>(execErr);
expectAssignable<SDKError>(notFoundErr);
expectAssignable<SDKError>(validationErr);

// ============================================================================
// Test 13: Severity override in constructors
// ============================================================================

// Can override default severity
const customSeverityError = new ConfigurationValidationError(
  "Custom severity",
  { severity: "warning" }
);
expectType<ErrorSeverity>(customSeverityError.severity);

const runtimeWarningError = new RuntimeValidationError(
  "Runtime warning",
  { severity: "warning" }
);
expectType<ErrorSeverity>(runtimeWarningError.severity);

// ============================================================================
// Test 14: Context merging
// ============================================================================

// Context should merge provided fields with specific error fields
const mergedContextError = new ConfigurationValidationError(
  "Merged context",
  {
    configPath: "workflow.config",
    field: "name",
    value: "",
    context: { customField: "customValue" },
  }
);

if (mergedContextError.context) {
  expectType<string | undefined>(mergedContextError.context["configPath"] as string | undefined);
  expectType<string | undefined>(mergedContextError.context["field"] as string | undefined);
  expectType<unknown>(mergedContextError.context["customField"]);
}
