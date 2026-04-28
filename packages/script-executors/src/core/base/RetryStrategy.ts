/**
 * Retry Strategy
 * Manages the retry logic in the event of execution failures
 */

/**
 * Retry strategy configuration
 */
export interface RetryStrategyConfig {
  /** Maximum number of retries */
  maxRetries?: number;
  /** Basic latency (in milliseconds) */
  baseDelay?: number;
  /** Whether to use exponential backoff */
  exponentialBackoff?: boolean;
  /** Maximum Delay (in milliseconds) */
  maxDelay?: number;
}

/**
 * Retry Strategy
 */
export class RetryStrategy {
  private maxRetries: number;
  private baseDelay: number;
  private exponentialBackoff: boolean;
  private maxDelay: number;

  constructor(config: RetryStrategyConfig = {}) {
    this.maxRetries = config.maxRetries ?? 3;
    this.baseDelay = config.baseDelay ?? 1000;
    this.exponentialBackoff = config.exponentialBackoff ?? true;
    this.maxDelay = config.maxDelay ?? 30000;
  }

  /**
   * Determine whether a retry should be attempted
   * @param error The error object
   * @param attempt The current number of attempts
   * @returns Whether a retry is necessary
   */
  shouldRetry(error: Error, attempt: number): boolean {
    // If the maximum number of retries has been reached, no further retries will be attempted.
    if (attempt >= this.maxRetries) {
      return false;
    }

    // Check the error types; some errors should not be retried.
    const nonRetryableErrors = ["ValidationError", "ConfigurationError", "ScriptNotFoundError"];

    return !nonRetryableErrors.some(
      errorType => error.name.includes(errorType) || error.message.includes(errorType),
    );
  }

  /**
   * Get the retry delay time
   * @param attempt The current number of attempts
   * @returns Delay time (in milliseconds)
   */
  getRetryDelay(attempt: number): number {
    if (this.exponentialBackoff) {
      // Exponential backoff: baseDelay * 2^attempt
      const delay = this.baseDelay * Math.pow(2, attempt);
      return Math.min(delay, this.maxDelay);
    } else {
      // Fixed delay
      return this.baseDelay;
    }
  }

  /**
   * Create a default retry policy
   * @returns An instance of the default retry policy
   */
  static createDefault(): RetryStrategy {
    return new RetryStrategy();
  }
}
