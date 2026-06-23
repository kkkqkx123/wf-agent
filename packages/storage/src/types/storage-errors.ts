/**
 * Storing Error Type Definitions
 */

import { SDKError, type ErrorSeverity } from "@wf-agent/types";

/**
 * Stored Error Base Class
 */
export class StorageError extends SDKError {
  constructor(
    message: string,
    public readonly operation: string,
    context?: Record<string, unknown>,
    cause?: Error,
    severity?: ErrorSeverity,
  ) {
    super(message, severity, { ...context, operation }, cause);
  }

  protected override getDefaultSeverity(): ErrorSeverity {
    return "error";
  }
}

/**
 * Storage quota overrun error
 */
export class StorageQuotaExceededError extends StorageError {
  constructor(
    message: string,
    public readonly requiredBytes: number,
    public readonly availableBytes: number,
    context?: Record<string, unknown>,
  ) {
    super(message, "quota", { ...context, requiredBytes, availableBytes });
  }
}

/**
 * Entity not found error
 */
export class EntityNotFoundError extends StorageError {
  constructor(
    message: string,
    public readonly entityId: string,
    public readonly entityType: string,
    context?: Record<string, unknown>,
  ) {
    super(message, "load", { ...context, entityId, entityType });
  }
}

/**
 * Storage initialization error
 */
export class StorageInitializationError extends StorageError {
  constructor(message: string, cause?: Error, context?: Record<string, unknown>) {
    super(message, "initialize", context, cause);
  }
}

/**
 * serialization error
 */
export class SerializationError extends StorageError {
  constructor(
    message: string,
    public readonly entityId: string,
    cause?: Error,
    context?: Record<string, unknown>,
  ) {
    super(message, "serialize", { ...context, entityId }, cause);
  }
}
