/**
 * Retry Strategy
 * Implemented based on `common-utils`'s `executeWithRetry`, providing a convenient interface.
 */

import { TimeoutError, HttpError, NetworkError } from "@wf-agent/types";
import {
  InternalServerError,
  RateLimitError,
  ServiceUnavailableError,
  executeWithRetry,
  type RetryConfig,
} from "@wf-agent/common-utils";

/**
 * Retry strategy configuration
 */
export interface RetryStrategyConfig {
  /** Maximum number of retries */
  maxRetries: number;
  /** Basic latency time (in milliseconds) */
  baseDelay: number;
  /** Should exponential backoff be used? */
  exponentialBackoff: boolean;
  /** Maximum delay time (in milliseconds) */
  maxDelay?: number;
}

/**
 * Retry Strategy
 * Wraps the `executeWithRetry` function from `common-utils` to provide a more user-friendly interface.
 */
export class RetryStrategy {
  private config: RetryStrategyConfig;

  constructor(config: RetryStrategyConfig) {
    this.config = config;
  }

  /**
   * Determine whether a retry should be attempted
   * @param error The error object
   * @param retryCount The current number of retries
   * @returns Whether a retry should be performed
   */
  shouldRetry(error: Error, retryCount: number): boolean {
    // Exceeded the maximum number of retries.
    if (retryCount >= this.config.maxRetries) {
      return false;
    }

    // TimeoutError - Retry after timeout
    if (error instanceof TimeoutError) {
      return true;
    }

    // Specific HTTP error types - Check these first
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
      // 5xx Server Error Retry
      if (error.statusCode >= 500 && error.statusCode < 600) {
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
   * Get the retry delay time
   * @param retryCount: The current number of retries
   * @returns: Delay time in milliseconds
   */
  getRetryDelay(retryCount: number): number {
    let delay: number;

    if (this.config.exponentialBackoff) {
      // Exponential backoff: baseDelay * 2^retryCount
      delay = this.config.baseDelay * Math.pow(2, retryCount);
    } else {
      // Fixed delay
      delay = this.config.baseDelay;
    }

    // Apply the maximum latency limit.
    if (this.config.maxDelay && delay > this.config.maxDelay) {
      delay = this.config.maxDelay;
    }

    return delay;
  }

  /**
   * Execute a function with retries
   * @param fn The asynchronous function to be executed
   * @returns The return value of the function
   */
  async execute<T>(fn: () => Promise<T>): Promise<T> {
    const retryConfig: RetryConfig = {
      maxRetries: this.config.maxRetries,
      baseDelay: this.config.baseDelay,
      maxDelay: this.config.maxDelay,
    };

    return executeWithRetry(fn, retryConfig);
  }

  /**
   * Create a default retry policy
   */
  static createDefault(): RetryStrategy {
    return new RetryStrategy({
      maxRetries: 3,
      baseDelay: 1000,
      exponentialBackoff: true,
      maxDelay: 30000,
    });
  }

  /**
   * Create a no-retry strategy
   */
  static createNoRetry(): RetryStrategy {
    return new RetryStrategy({
      maxRetries: 0,
      baseDelay: 0,
      exponentialBackoff: false,
    });
  }

  /**
   * Create a custom retry strategy
   */
  static createCustom(config: Partial<RetryStrategyConfig>): RetryStrategy {
    return new RetryStrategy({
      maxRetries: config.maxRetries ?? 3,
      baseDelay: config.baseDelay ?? 1000,
      exponentialBackoff: config.exponentialBackoff ?? true,
      maxDelay: config.maxDelay ?? 30000,
    });
  }
}
