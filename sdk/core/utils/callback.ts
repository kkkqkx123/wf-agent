/**
 * Callback utility functions
 * Provide auxiliary features, including callback wrapping, error handling, timeout control, etc.
 *
 * Design principles:
 * - Stateless, exported as pure functions
 * - Offer general-purpose callback handling tools
 * - Support error handling and timeout control
 */

import { getErrorOrNew, now, diffTimestamp } from "@wf-agent/common-utils";
import { sdkLogger as logger } from "../../utils/logger.js";

/**
 * Wrap the callback function and add error handling
 * @param callback The original callback function
 * @returns The wrapped callback function
 */
export function wrapCallback<T extends (...args: unknown[]) => unknown>(callback: T): T {
  return ((...args: unknown[]) => {
    try {
      return callback(...args);
    } catch (error) {
      logger.error("Error in callback", { error: getErrorOrNew(error) });
      throw error;
    }
  }) as T;
}

/**
 * Create a timeout Promise
 * @param timeout Timeout duration (in milliseconds)
 * @param errorMessage Timeout error message
 * @returns Timeout Promise
 */
export function createTimeoutPromise(
  timeout: number,
  errorMessage: string = "Operation timed out",
): Promise<never> {
  return new Promise((_, reject) => {
    setTimeout(() => {
      reject(new Error(errorMessage));
    }, timeout);
  });
}

/**
 * Executing a Promise with a timeout
 * @param promise The Promise to be executed
 * @param timeout The timeout period in milliseconds
 * @param errorMessage The error message to display in case of a timeout
 * @returns The result of the Promise
 */
export async function withTimeout<T>(
  promise: Promise<T>,
  timeout: number,
  errorMessage: string = "Operation timed out",
): Promise<T> {
  return Promise.race([promise, createTimeoutPromise(timeout, errorMessage)]);
}

/**
 * Verify the validity of the callback function
 * @param callback The callback function
 * @returns Whether it is valid
 */
export function validateCallback(callback: unknown): boolean {
  return typeof callback === "function";
}

/**
 * Create a safe callback function
 * @param callback The original callback function
 * @param defaultValue The default return value
 * @returns The safe callback function
 */
export function createSafeCallback<T extends (...args: unknown[]) => unknown>(
  callback: T,
  defaultValue: ReturnType<T>,
): T {
  return ((...args: unknown[]) => {
    try {
      if (validateCallback(callback)) {
        return callback(...args);
      }
      return defaultValue;
    } catch (error) {
      logger.error("Error in safe callback", { error: getErrorOrNew(error) });
      return defaultValue;
    }
  }) as T;
}

/**
 * Batch execution of callback functions
 * @param callbacks Array of callback functions
 * @param args Callback arguments
 * @returns Array of execution results
 */
export function executeCallbacks<T extends (...args: unknown[]) => unknown>(
  callbacks: T[],
  ...args: Parameters<T>
): Array<ReturnType<T> | Error> {
  return callbacks.map((callback): ReturnType<T> | Error => {
    try {
      return callback(...args) as ReturnType<T>;
    } catch (error) {
      return getErrorOrNew(error);
    }
  });
}

/**
 * Create a retry callback
 * @param callback The original callback function
 * @param maxRetries The maximum number of retries
 * @param delay The retry delay (in milliseconds)
 * @returns The callback function with retries implemented
 */
export function createRetryCallback<T extends (...args: unknown[]) => Promise<unknown>>(
  callback: T,
  maxRetries: number = 3,
  delay: number = 1000,
): T {
  return (async (...args: Parameters<T>): Promise<ReturnType<T>> => {
    let lastError: Error | null = null;

    for (let i = 0; i <= maxRetries; i++) {
      try {
        return (await callback(...args)) as ReturnType<T>;
      } catch (error) {
        lastError = getErrorOrNew(error);
        if (i < maxRetries) {
          await new Promise(resolve => setTimeout(resolve, delay));
        }
      }
    }

    throw lastError;
  }) as T;
}

/**
 * Create a throttled callback function
 * @param callback The original callback function
 * @param delay The throttling delay (in milliseconds)
 * @returns The throttled callback function
 */
export function createThrottledCallback<T extends (...args: unknown[]) => unknown>(
  callback: T,
  delay: number = 1000,
): T {
  let lastCallTime = 0;
  let timeoutId: NodeJS.Timeout | null = null;

  return ((...args: Parameters<T>) => {
    const currentTime = now();
    const timeSinceLastCall = currentTime - lastCallTime;

    if (timeSinceLastCall >= delay) {
      lastCallTime = currentTime;
      return callback(...args);
    } else {
      if (timeoutId) {
        clearTimeout(timeoutId);
      }
      timeoutId = setTimeout(() => {
        lastCallTime = now();
        callback(...args);
      }, delay - timeSinceLastCall);
      return undefined;
    }
  }) as T;
}

/**
 * Create a debounced callback function
 * @param callback The original callback function
 * @param delay The debouncing delay (in milliseconds)
 * @returns The debounced callback function
 */
export function createDebouncedCallback<T extends (...args: unknown[]) => unknown>(
  callback: T,
  delay: number = 1000,
): T {
  let timeoutId: NodeJS.Timeout | null = null;

  return ((...args: Parameters<T>) => {
    if (timeoutId) {
      clearTimeout(timeoutId);
    }
    timeoutId = setTimeout(() => {
      callback(...args);
    }, delay);
  }) as T;
}

/**
 * Create a one-time callback
 * @param callback The original callback function
 * @returns The one-time callback function
 */
export function createOnceCallback<T extends (...args: unknown[]) => unknown>(callback: T): T {
  let called = false;

  return ((...args: Parameters<T>) => {
    if (!called) {
      called = true;
      return callback(...args);
    }
    return undefined;
  }) as T;
}

/**
 * Create a cached callback
 * @param callback The original callback function
 * @param keyGenerator The function for generating cache keys
 * @param ttl The cache lifetime (in milliseconds)
 * @returns The cached callback function
 */
export function createCachedCallback<T extends (...args: unknown[]) => unknown>(
  callback: T,
  keyGenerator: (...args: Parameters<T>) => string = (...args) => JSON.stringify(args),
  ttl: number = 60000,
): T {
  const cache = new Map<string, { value: ReturnType<T>; timestamp: number }>();

  return ((...args: Parameters<T>) => {
    const key = keyGenerator(...args);
    const cached = cache.get(key);

    if (cached && diffTimestamp(cached.timestamp, now()) < ttl) {
      return cached.value;
    }

    const result = callback(...args) as ReturnType<T>;
    cache.set(key, { value: result, timestamp: now() });
    return result;
  }) as T;
}

/**
 * Clear the cache
 * @param cache: The cache map
 * @param ttl: The cache lifetime (in milliseconds)
 */
export function cleanupCache<T>(
  cache: Map<string, { value: T; timestamp: number }>,
  ttl: number,
): void {
  const currentTime = now();
  const keysToDelete: string[] = [];

  cache.forEach((entry, key) => {
    if (currentTime - entry.timestamp >= ttl) {
      keysToDelete.push(key);
    }
  });

  keysToDelete.forEach(key => cache.delete(key));
}
