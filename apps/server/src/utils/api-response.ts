/**
 * API Response Utilities
 *
 * Standard response formatting for all API endpoints
 */

/**
 * Standard API response format
 */
export interface ApiResponse<T = any> {
  success: boolean;
  data?: T;
  error?: {
    code: string;
    message: string;
    details?: Record<string, any>;
  };
  meta?: {
    timestamp: string;
    path?: string;
    method?: string;
  };
}

/**
 * Create a successful response
 */
export function successResponse<T>(
  data: T,
  meta?: Record<string, any>
): ApiResponse<T> {
  return {
    success: true,
    data,
    meta: {
      timestamp: new Date().toISOString(),
      ...meta,
    },
  };
}

/**
 * Create an error response
 */
export function errorResponse(
  code: string,
  message: string,
  details?: Record<string, any>,
  meta?: Record<string, any>
): ApiResponse {
  return {
    success: false,
    error: {
      code,
      message,
      details,
    },
    meta: {
      timestamp: new Date().toISOString(),
      ...meta,
    },
  };
}

/**
 * Map error to API error response
 */
export function mapErrorToResponse(
  error: unknown,
  path?: string,
  method?: string
): ApiResponse {
  let code = "INTERNAL_ERROR";
  let message = "An unexpected error occurred";
  let details: Record<string, any> | undefined;

  if (error instanceof Error) {
    message = error.message;

    // Extract code from error name or message
    if (error.name === "ValidationError") {
      code = "VALIDATION_ERROR";
    } else if (error.message.includes("not found")) {
      code = "NOT_FOUND";
    } else if (error.message.includes("already exists")) {
      code = "CONFLICT";
    } else if (error.message.includes("unauthorized")) {
      code = "UNAUTHORIZED";
    } else if (error.message.includes("forbidden")) {
      code = "FORBIDDEN";
    }

    // Attach stack trace in development
    if (process.env["NODE_ENV"] !== "production") {
      details = { stack: error.stack };
    }
  } else if (typeof error === "object" && error !== null) {
    const obj = error as Record<string, any>;
    if (obj["code"]) code = obj["code"];
    if (obj["message"]) message = obj["message"];
    if (obj["details"]) details = obj["details"];
  }

  return {
    success: false,
    error: {
      code,
      message,
      details,
    },
    meta: {
      timestamp: new Date().toISOString(),
      path,
      method,
    },
  };
}

/**
 * HTTP status code mapper
 */
export function getHttpStatus(errorCode: string): number {
  const statusMap: Record<string, number> = {
    VALIDATION_ERROR: 400,
    UNAUTHORIZED: 401,
    FORBIDDEN: 403,
    NOT_FOUND: 404,
    CONFLICT: 409,
    INTERNAL_ERROR: 500,
    ADAPTER_ERROR: 500,
    ADAPTER_NOT_INITIALIZED: 500,
    INVALID_PAGINATION: 400,
  };

  return statusMap[errorCode] || 500;
}
