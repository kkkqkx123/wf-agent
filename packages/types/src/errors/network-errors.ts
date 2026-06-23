/**
 * 网络相关错误类型定义
 * 定义网络、HTTP和LLM调用相关的错误类型
 *
 * 注意：这些错误默认为警告级别（warning），因为它们通常表示可重试的临时性错误
 * 如果需要记录警告但不中断执行，请使用 ContextualLogger.networkWarning()
 */

import { SDKError, ErrorSeverity } from "./base.js";

/**
 * LLM error type enumeration
 *
 * Used to distinguish between different types of LLM errors for error handling and retry decisions
 */
export type LLMErrorType =
  | "CONFIG_ERROR"
  | "NETWORK_ERROR"
  | "API_ERROR"
  | "PARSE_ERROR"
  | "TIMEOUT_ERROR"
  | "CANCELLED_ERROR"
  | "RATE_LIMIT_ERROR"
  | "VALIDATION_ERROR"
  | "UNKNOWN_ERROR";

/**
 * Network Error Type
 * Indicates a generic network connectivity problem (e.g. DNS resolution failure, connection timeout, network unreachable, etc.)
 * Note: HTTP protocol errors should use HttpError and its subclasses.
 *
 * Default severity: warning (temporary error that can be retried)
 */
export class NetworkError extends SDKError {
  constructor(
    message: string,
    context?: Record<string, unknown>,
    cause?: Error,
    severity?: ErrorSeverity,
  ) {
    super(message, severity, context, cause);
  }

  protected override getDefaultSeverity(): ErrorSeverity {
    return "warning";
  }
}

/**
 * HTTP Error Type
 * Indicates errors at the HTTP protocol level (e.g., 4xx, 5xx status codes).
 * Specific HTTP status code error types are defined in packages/common-utils/src/http/errors.ts.
 * This type is used as fallback logic for undefined status codes.
 * 
 * Default severity: warning (temporary error that can be retried)
 */
export class HttpError extends SDKError {
  constructor(
    message: string,
    public readonly statusCode: number,
    context?: Record<string, unknown>,
    cause?: Error,
    severity?: ErrorSeverity,
  ) {
    super(message, severity, { ...context, statusCode }, cause);
  }

  protected override getDefaultSeverity(): ErrorSeverity {
    return "warning";
  }
}

/**
 * LLM Error Types
 *
 * Description:
 * 1. Inherits from HttpError, because LLM API calls are essentially HTTP requests.
 * 2. BaseLLMClient catches all upstream errors (including HttpError, BadRequestError, TimeoutError, etc. 
 *    thrown by the HTTP client) via try-catch in the generate/generateStream methods.
 * 3. The handleError() method normalizes these heterogeneous errors into LLMError, 
 *    attaching provider and model information.
 * 4. The original error is preserved in the `cause` property, ensuring no loss of error details.
 * 5. The error chain is retained through the `cause` property, facilitating tracing to the root cause.
 *
 * Examples:
 * - HTTP 401 (UnauthorizedError) → LLMError (type: API_ERROR, statusCode: 401)
 * - HTTP 429 (RateLimitError) → LLMError (type: RATE_LIMIT_ERROR, statusCode: 429)
 * - HTTP 500 (InternalServerError) → LLMError (type: API_ERROR, statusCode: 500)
 * - Request timeout (TimeoutError) → LLMError (type: TIMEOUT_ERROR, statusCode: undefined)
 * - JSON parsing error (Error) → LLMError (type: PARSE_ERROR, statusCode: undefined)
 * - User cancellation (AbortError) → LLMError (type: CANCELLED_ERROR, statusCode: undefined)
 *
 * Default severity: warning (retriable transient error)
 */
export class LLMError extends HttpError {
  /**
   * Type of error
   */
  public readonly type: LLMErrorType;

  constructor(
    message: string,
    public readonly provider: string,
    public readonly model?: string,
    type?: LLMErrorType,
    statusCode?: number,
    context?: Record<string, unknown>,
    cause?: Error,
    severity?: ErrorSeverity,
  ) {
    // If there is no statusCode, use 0 for non-HTTP errors.
    super(message, statusCode ?? 0, { ...context, provider, model, type }, cause, severity);
    // If no type is provided, inferred from statusCode
    this.type = type ?? LLMError.inferErrorType(statusCode, cause);
  }

  /**
   * Inferring error types from HTTP status codes and raw errors
   */
  private static inferErrorType(statusCode?: number, cause?: Error): LLMErrorType {
    // If there is a raw error, check for a canceled error
    if (cause) {
      const errorName = cause.name?.toLowerCase() || "";
      const errorMessage = cause.message?.toLowerCase() || "";
      if (
        errorName === "aborterror" ||
        errorName.includes("abort") ||
        errorMessage.includes("abort") ||
        errorMessage.includes("cancel")
      ) {
        return "CANCELLED_ERROR";
      }
    }

    // Inferred from HTTP status code
    if (statusCode) {
      if (statusCode === 429) {
        return "RATE_LIMIT_ERROR";
      }
      if (statusCode === 401 || statusCode === 403) {
        return "CONFIG_ERROR";
      }
      if (statusCode === 400) {
        return "VALIDATION_ERROR";
      }
      if (statusCode >= 500) {
        return "API_ERROR";
      }
      if (statusCode >= 400) {
        return "API_ERROR";
      }
    }

    // Check for timeout errors
    if (cause) {
      const errorName = cause.name?.toLowerCase() || "";
      if (errorName.includes("timeout")) {
        return "TIMEOUT_ERROR";
      }
    }

    return "UNKNOWN_ERROR";
  }

  /**
   * Determine if an error is retryable
   *
   * @returns true means it can be retried, false means it should not be retried.
   */
  isRetryable(): boolean {
    switch (this.type) {
      case "CONFIG_ERROR":
      case "PARSE_ERROR":
      case "CANCELLED_ERROR":
      case "VALIDATION_ERROR":
        return false;
      case "NETWORK_ERROR":
      case "API_ERROR":
      case "TIMEOUT_ERROR":
      case "RATE_LIMIT_ERROR":
      case "UNKNOWN_ERROR":
        return true;
      default:
        return true;
    }
  }

  /**
   * Get retry delay in milliseconds
   *
   * For flow-limiting errors, a longer delay is recommended
   */
  getRetryDelay(): number {
    if (this.type === "RATE_LIMIT_ERROR") {
      // Error in limiting flow: suggest waiting longer
      return 5000;
    }
    return 1000;
  }

  protected override getDefaultSeverity(): ErrorSeverity {
    // Configuration errors and authentication errors are serious errors
    if (this.type === "CONFIG_ERROR" || this.type === "VALIDATION_ERROR") {
      return "error";
    }
    // User cancelation is the information level
    if (this.type === "CANCELLED_ERROR") {
      return "info";
    }
    // Other errors default to warning level
    return "warning";
  }
}

/**
 * Fuse open error type
 *
 * Default severity: WARNING (retryable temporary error)
 */
export class CircuitBreakerOpenError extends SDKError {
  constructor(
    message: string,
    public readonly state?: string,
    context?: Record<string, unknown>,
    severity?: ErrorSeverity,
  ) {
    super(message, severity, { ...context, state });
  }

  protected override getDefaultSeverity(): ErrorSeverity {
    return "warning";
  }
}
