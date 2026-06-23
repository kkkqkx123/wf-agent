/**
 * Timeout Execution Wrapper
 *
 * Provides a simple wrapper to add timeout support to async operations
 * without modifying the internal execution flow.
 *
 * Usage:
 *   const result = await withExecutionTimeout(
 *     executor.execute(config),
 *     60000,
 *     "Agent Loop Execution"
 *   );
 */

import { ExecutionError } from "@wf-agent/types";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "TimeoutWrapper" });

/**
 * Execute a promise with timeout
 * @param promise The promise to execute
 * @param timeoutMs Timeout in milliseconds (if undefined, no timeout)
 * @param operationName Name of the operation for logging
 * @returns The result of the promise or throws ExecutionError on timeout
 */
export async function withExecutionTimeout<T>(
  promise: Promise<T>,
  timeoutMs: number | undefined,
  operationName: string
): Promise<T> {
  // If no timeout specified, just execute the promise as-is
  if (!timeoutMs || timeoutMs <= 0) {
    return promise;
  }

  const startTime = Date.now();
  const timeoutPromise = new Promise<T>((_, reject) => {
    const handle = setTimeout(() => {
      const elapsed = Date.now() - startTime;
      const error = new ExecutionError(
        `Operation "${operationName}" exceeded timeout of ${timeoutMs}ms (elapsed: ${elapsed}ms)`,
        undefined,
        undefined
      );

      logger.warn(
        "Operation timeout exceeded",
        undefined,
        {
          operationName,
          timeoutMs,
          elapsed,
        }
      );

      reject(error);
    }, timeoutMs);

    // Cleanup timeout when the actual promise settles
    promise
      .then(
        () => clearTimeout(handle),
        () => clearTimeout(handle)
      );
  });

  return Promise.race([promise, timeoutPromise]);
}

/**
 * Check if timeout has been exceeded
 * @param startTime Start timestamp
 * @param timeoutMs Timeout in milliseconds
 * @returns true if timeout exceeded, false otherwise
 */
export function isTimeoutExceeded(startTime: number, timeoutMs: number | undefined): boolean {
  if (!timeoutMs || timeoutMs <= 0) return false;
  return Date.now() - startTime > timeoutMs;
}

/**
 * Get remaining time
 * @param startTime Start timestamp
 * @param timeoutMs Timeout in milliseconds
 * @returns Remaining time in milliseconds, or Infinity if no timeout
 */
export function getRemainingTime(startTime: number, timeoutMs: number | undefined): number {
  if (!timeoutMs || timeoutMs <= 0) return Infinity;
  const remaining = timeoutMs - (Date.now() - startTime);
  return Math.max(0, remaining);
}

/**
 * Get elapsed time
 * @param startTime Start timestamp
 * @returns Elapsed time in milliseconds
 */
export function getElapsedTime(startTime: number): number {
  return Date.now() - startTime;
}
