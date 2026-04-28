/**
 * Definition of base error types
 * Define the base type and base class of the error system
 */

/**
 * Severity of error
 */
export type ErrorSeverity =
  /**
   * Critical error - causes execution to stop
   * Applies to: configuration errors, validation errors, unrecoverable logic errors
   */
  | "error"
  /**
   * Warning Error - Continue Execution
   * Applies to: network timeouts, temporary failures, retryable errors
   */
  | "warning"
  /**
   * Message Error - Continue Execution
   * Applies to: debugging messages, non-critical warnings, monitoring events
   */
  | "info";

/**
 * error context
 */
export interface ErrorContext {
  /** Thread ID */
  threadId?: string;
  /** Workflow ID */
  workflowId?: string;
  /** Node ID */
  nodeId?: string;
  /** Operation Name */
  operation?: string;
  /** Tool ID */
  toolId?: string;
  /** Tool name */
  toolName?: string;
  /** Tool type */
  toolType?: string;
  /** field name */
  field?: string;
  /** field value */
  value?: unknown;
  /** Resource type */
  resourceType?: string;
  /** Resource ID */
  resourceId?: string;
  /** severity */
  severity?: "error" | "warning" | "info";
  /** additional contextual information */
  [key: string]: unknown;
}

/**
 * Error handling results
 */
export interface ErrorHandlingResult {
  /** Whether implementation should be halted */
  shouldStop: boolean;
  /** Standardized error objects */
  error: SDKError;
}

/**
 * SDK base error class
 * Provides default severity levels that subclasses can override
 */
export class SDKError extends Error {
  /**
   * Get the default severity
   * Subclasses can override this method to provide different defaults
   */
  protected getDefaultSeverity(): ErrorSeverity {
    return "error";
  }

  constructor(
    message: string,
    severity?: ErrorSeverity,
    public readonly context?: Record<string, unknown>,
    public override readonly cause?: Error,
  ) {
    super(message);
    // Uses the severity passed in, or defaults to it if not.
    this.severity = severity ?? this.getDefaultSeverity();
    Error.captureStackTrace(this, this.constructor);
  }

  /**
   * Severity of error
   */
  public readonly severity: ErrorSeverity;

  /**
   * Convert to JSON object
   */
  toJSON(): Record<string, unknown> {
    return {
      name: this.constructor.name,
      message: this.message,
      severity: this.severity,
      context: this.context,
      cause: this.cause
        ? {
            name: this.cause.name,
            message: this.cause.message,
            stack: this.cause.stack,
          }
        : undefined,
      stack: this.stack,
    };
  }
}

/**
 * Validating Error Types
 */
export class ValidationError extends SDKError {
  constructor(
    message: string,
    public readonly field?: string,
    public readonly value?: unknown,
    context?: Record<string, unknown>,
    severity?: ErrorSeverity,
  ) {
    super(message, severity, { ...context, field, value });
  }

  protected override getDefaultSeverity(): ErrorSeverity {
    return "error";
  }
}

/**
 * Execution Error Type
 */
export class ExecutionError extends SDKError {
  constructor(
    message: string,
    public readonly nodeId?: string,
    public readonly workflowId?: string,
    context?: Record<string, unknown>,
    cause?: Error,
    severity?: ErrorSeverity,
  ) {
    super(message, severity, { ...context, nodeId, workflowId }, cause);
  }

  protected override getDefaultSeverity(): ErrorSeverity {
    return "error";
  }
}

/**
 * 资源未找到错误类型
 *
 * 注意：资源未找到通常是严重错误，会导致执行中断
 * 如果需要记录警告但不中断执行，请使用 ContextualLogger.resourceNotFoundWarning()
 */
export class NotFoundError extends SDKError {
  constructor(
    message: string,
    public readonly resourceType: string,
    public readonly resourceId: string,
    context?: Record<string, unknown>,
    severity?: ErrorSeverity,
  ) {
    super(message, severity, { ...context, resourceType, resourceId });
  }

  protected override getDefaultSeverity(): ErrorSeverity {
    return "error";
  }
}
