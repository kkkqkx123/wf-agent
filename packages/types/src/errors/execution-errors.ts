/**
 * Defining Execution-Related Error Types
 * Define the type of error during execution
 */

import { ExecutionError, ErrorSeverity } from "./base.js";

/**
 * Business Logic Error Types
 *
 * Specialized for business logic related execution errors (e.g. route mismatch, condition not satisfied, etc.)
 * Inherited from ExecutionError to maintain backward compatibility
 *
 * Usage Scenario:
 * - Routing condition evaluation failure
 * - Condition node judgment failure
 * - Loop termination condition exception
 * - Business rule validation failure
 */
export class BusinessLogicError extends ExecutionError {
  constructor(
    message: string,
    public readonly businessContext?: string,
    public readonly ruleName?: string,
    nodeId?: string,
    workflowId?: string,
    context?: Record<string, unknown>,
    cause?: Error,
    severity?: ErrorSeverity,
  ) {
    super(message, nodeId, workflowId, { ...context, businessContext, ruleName }, cause, severity);
  }

  protected override getDefaultSeverity(): ErrorSeverity {
    return "error";
  }
}

/**
 * System implementation error type
 *
 * Used for system-level execution errors (e.g. state management failure, context loss, etc.)
 * Inherited from ExecutionError to maintain backward compatibility
 *
 * Note: For more specific error scenarios, use the following specialized error types:
 * - DependencyInjectionError: Dependency injection failure
 * - StateManagementError: State management failure
 * - CheckpointError: Checkpoint operation failed
 * - EventSystemError: Event system error
 */
export class SystemExecutionError extends ExecutionError {
  constructor(
    message: string,
    public readonly systemComponent?: string,
    public readonly failurePoint?: string,
    nodeId?: string,
    workflowId?: string,
    context?: Record<string, unknown>,
    cause?: Error,
    severity?: ErrorSeverity,
  ) {
    super(
      message,
      nodeId,
      workflowId,
      { ...context, systemComponent, failurePoint },
      cause,
      severity,
    );
  }

  protected override getDefaultSeverity(): ErrorSeverity {
    return "error";
  }
}

/**
 * Dependency Injection Error Types
 *
 * Specialized for dependency injection related errors
 * Used when required services, components or dependencies are not provided or cannot be resolved
 *
 * Usage Scenario:
 * - Required services are not injected
 * - Dependency resolution failure
 * - Service initialization failure
 */
export class DependencyInjectionError extends ExecutionError {
  constructor(
    message: string,
    public readonly dependencyName: string,
    public readonly requiredBy?: string,
    nodeId?: string,
    workflowId?: string,
    context?: Record<string, unknown>,
    cause?: Error,
    severity?: ErrorSeverity,
  ) {
    super(message, nodeId, workflowId, { ...context, dependencyName, requiredBy }, cause, severity);
  }

  protected override getDefaultSeverity(): ErrorSeverity {
    return "error";
  }
}

/**
 * Status Management Error Types
 *
 * Specialized for state management related errors
 * Used when a status read, write, update, or delete operation fails
 *
 * Usage Scenario:
 * - Status read failure
 * - Status write failure
 * - Status synchronization failure
 * - Status consistency error
 */
export class StateManagementError extends ExecutionError {
  constructor(
    message: string,
    public readonly stateType: string,
    public readonly operation: "read" | "write" | "delete" | "update" | "sync",
    public readonly stateKey?: string,
    nodeId?: string,
    workflowId?: string,
    context?: Record<string, unknown>,
    cause?: Error,
    severity?: ErrorSeverity,
  ) {
    super(
      message,
      nodeId,
      workflowId,
      { ...context, stateType, operation, stateKey },
      cause,
      severity,
    );
  }

  protected override getDefaultSeverity(): ErrorSeverity {
    return "error";
  }
}

/**
 * Checkpoint Error Type
 *
 * Specialized for checkpoint-related errors
 * Used when a checkpoint creation, recovery, deletion, or validation operation fails
 *
 * Usage Scenario:
 * - Checkpoint creation failure
 * - Checkpoint recovery failure
 * - Checkpoint deletion failure
 * - Checkpoint validation failure
 */
export class CheckpointError extends ExecutionError {
  constructor(
    message: string,
    public readonly operation: "create" | "restore" | "delete" | "validate",
    public readonly checkpointId?: string,
    nodeId?: string,
    workflowId?: string,
    context?: Record<string, unknown>,
    cause?: Error,
    severity?: ErrorSeverity,
  ) {
    super(message, nodeId, workflowId, { ...context, operation, checkpointId }, cause, severity);
  }

  protected override getDefaultSeverity(): ErrorSeverity {
    return "error";
  }
}

/**
 * Event System Error Types
 *
 * Specialized for event system related errors
 * Used when an event firing, listening, or processing operation fails
 *
 * Usage Scenario:
 * - Failure to fire an event
 * - Event listener registration fails
 * - Event handler execution failure
 * - Event bus error
 */
export class EventSystemError extends ExecutionError {
  constructor(
    message: string,
    public readonly operation: "emit" | "subscribe" | "unsubscribe" | "handle",
    public readonly eventType?: string,
    nodeId?: string,
    workflowId?: string,
    context?: Record<string, unknown>,
    cause?: Error,
    severity?: ErrorSeverity,
  ) {
    super(message, nodeId, workflowId, { ...context, eventType, operation }, cause, severity);
  }

  protected override getDefaultSeverity(): ErrorSeverity {
    return "error";
  }
}
