/**
 * Retry Handler
 * Only functions are exported, not classes.
 *
 * Provides an exponential backoff retry strategy for automatically handling errors that can be retried.
 */

import { TimeoutError, NetworkError, HttpError } from "@wf-agent/types";
import { InternalServerError, RateLimitError, ServiceUnavailableError } from "./errors.js";

/**
 * HTTP status codes that should not be retried
 * These error codes indicate client-side errors or permanent issues, and retrying will not resolve the problem.
 */
export type NonRetryableStatusCode =
  | 400 /** Bad Request - The request format is incorrect. */
  | 401 /** Unauthorized - Authentication failed. */
  | 403 /** Forbidden - Insufficient permissions. */
  | 404 /** Not Found - The resource does not exist. */
  | 405 /** Method Not Allowed */
  | 410 /** Gone - The resource has been permanently deleted. */
  | 411 /** Length Required - Required Content-Length */
  | 412 /** Precondition Failed - The prerequisite condition has not been met. */
  | 413 /** Payload Too Large - The request body is too large. */
  | 414 /** URI Too Long - The URI is excessively long. */
  | 415 /** Unsupported Media Type - The specified media type is not recognized or supported by the system. */
  | 416 /** Range Not Satisfiable - The specified range does not meet the required conditions. */
  | 417 /** Expectation Failed - The expected outcome did not occur. */
  | 418 /** I'm a teapot - Teapot (RFC 2324) */
  | 422 /** Unprocessable Entity - Unable to process the entity. */
  | 426 /** Upgrade Required - The protocol needs to be upgraded. */
  | 428 /** Precondition Required - A prerequisite must be met before proceeding. */
  | 431 /** Request Header Fields Too Large */
  | 451; /** "Unavailable For Legal Reasons" */

/**
 * Retry handler configuration
 */
export interface RetryConfig {
  /** Maximum number of retries */
  maxRetries: number;
  /** Basic latency time (in milliseconds) */
  baseDelay: number;
  /** Maximum delay time (in milliseconds) */
  maxDelay?: number;
}

/**
 * Collection of HTTP status codes that should not be retried, for quick lookup purposes
 *
 */
const NON_RETRYABLE_STATUS_CODES = new Set<number>([
  400, 401, 403, 404, 405, 410, 411, 412, 413, 414, 415, 416, 417, 418, 422, 426, 428, 431, 451,
]);

/**
 * Delayed function
 */
function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}

/**
 * Calculate the retry delay (exponential backoff)
 */
function calculateDelay(attempt: number, baseDelay: number, maxDelay?: number): number {
  const delay = baseDelay * Math.pow(2, attempt);
  const finalMaxDelay = maxDelay || 30000; // Default is 30 seconds.
  return Math.min(delay, finalMaxDelay);
}

/**
 * Determine whether an error can be retried.
 *
 * Adopt a blacklist strategy: Retry all errors except for those that are explicitly not supposed to be retried.
 *
 * Error types that can be retried:
 * - TimeoutError: Request timed out
 * - NetworkError: Network error
 * - RateLimitError (429): Rate limiting error
 * - 5xx server errors: Temporary server issues
 * - 4xx client errors (except for those on the blacklist): May be temporary issues (e.g., 426 Upgrade Required)
 *
 * Errors that cannot be retried (blacklist):
 * - 400, 401, 403, 404, 405, 410, 411, 412, 413, 414, 415, 416, 417, 418, 422, 426, 428, 431, 451
 */
function shouldRetry(error: unknown): boolean {
  // TimeoutError - Retry after timeout
  if (error instanceof TimeoutError) {
    return true;
  }

  // Specific HTTP error types - Check these first.
  if (error instanceof RateLimitError) {
    return true;
  }

  if (error instanceof InternalServerError) {
    return true;
  }

  if (error instanceof ServiceUnavailableError) {
    return true;
  }

  // Generic HttpError - Determine based on the status code (as a fallback mechanism)
  if (error instanceof HttpError) {
    const statusCode = error.statusCode;

    // Check if it is in the blacklist.
    if (statusCode && NON_RETRYABLE_STATUS_CODES.has(statusCode)) {
      return false;
    }

    // 5xx Server Error Retry
    if (statusCode >= 500 && statusCode < 600) {
      return true;
    }

    // Other HTTP errors are not retried (4xx client errors).
    return false;
  }

  // NetworkError - Retry due to a network connection error.
  if (error instanceof NetworkError) {
    return true;
  }

  return false;
}

/**
 * Execute a function with retries
 *
 * @param fn The asynchronous function to be executed
 * @param config The retry configuration
 * @returns The return value of the function
 */
export async function executeWithRetry<T>(fn: () => Promise<T>, config: RetryConfig): Promise<T> {
  let lastError: Error | undefined;

  for (let attempt = 0; attempt <= config.maxRetries; attempt++) {
    try {
      return await fn();
    } catch (error) {
      lastError = error instanceof Error ? error : new Error(String(error));

      // Check whether a retry should be attempted.
      if (!shouldRetry(error) || attempt === config.maxRetries) {
        throw error;
      }

      // Calculate the delay and wait.
      const delay = calculateDelay(attempt, config.baseDelay, config.maxDelay);
      await sleep(delay);
    }
  }

  throw lastError || new Error("Unknown error");
}
