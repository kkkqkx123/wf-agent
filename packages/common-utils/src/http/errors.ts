/**
 * HTTP Error Type Definitions
 * Define HTTP client-specific error types
 */

import { HttpError } from "@wf-agent/types";

/**
 * HTTP 400 - request format error
 */
export class BadRequestError extends HttpError {
  constructor(message: string, context?: Record<string, unknown>, cause?: Error) {
    super(message, 400, context, cause);
  }
}

/**
 * HTTP 401 - Authentication failure
 */
export class UnauthorizedError extends HttpError {
  constructor(message: string, context?: Record<string, unknown>, cause?: Error) {
    super(message, 401, context, cause);
  }
}

/**
 * HTTP 403 - Insufficient authority
 */
export class ForbiddenError extends HttpError {
  constructor(message: string, context?: Record<string, unknown>, cause?: Error) {
    super(message, 403, context, cause);
  }
}

/**
 * HTTP 404 - Resource does not exist
 */
export class NotFoundHttpError extends HttpError {
  constructor(
    message: string,
    public readonly url: string,
    context?: Record<string, unknown>,
    cause?: Error,
  ) {
    super(message, 404, { ...context, url }, cause);
  }
}

/**
 * HTTP 409 - Conflict
 */
export class ConflictError extends HttpError {
  constructor(message: string, context?: Record<string, unknown>, cause?: Error) {
    super(message, 409, context, cause);
  }
}

/**
 * HTTP 422 - Unprocessable Entity
 */
export class UnprocessableEntityError extends HttpError {
  constructor(message: string, context?: Record<string, unknown>, cause?: Error) {
    super(message, 422, context, cause);
  }
}

/**
 * HTTP 429 - Stream Limit Error
 */
export class RateLimitError extends HttpError {
  constructor(
    message: string,
    public readonly retryAfter?: number,
    context?: Record<string, unknown>,
  ) {
    super(message, 429, context);
  }
}

/**
 * HTTP 500 - server error
 */
export class InternalServerError extends HttpError {
  constructor(message: string, context?: Record<string, unknown>, cause?: Error) {
    super(message, 500, context, cause);
  }
}

/**
 * HTTP 503 - Service unavailable
 */
export class ServiceUnavailableError extends HttpError {
  constructor(message: string, context?: Record<string, unknown>, cause?: Error) {
    super(message, 503, context, cause);
  }
}
