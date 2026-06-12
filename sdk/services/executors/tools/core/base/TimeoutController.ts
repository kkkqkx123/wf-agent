/**
 * Timeout Controller
 *
 * Lightweight wrapper for tool execution timeout control.
 * Internally reuses core timeout utility functions to avoid code duplication.
 *
 * Note: This is a specialized controller for services layer that doesn't need
 * full state management. For complex timeout scenarios, use TimeoutManager instead.
 */

import { TimeoutError } from "@wf-agent/types";
import { combineTimeoutWithSignal } from "../../../../../core/utils/timeout/index.js";

/**
 * Timeout Controller
 */
export class TimeoutController {
  constructor(private defaultTimeout: number = 30000) {}

  /**
   * Timed execution
   * @param fn The function to be executed
   * @param timeout The timeout period in milliseconds, using the default value set by the constructor by default
   * @param signal Optional abort signal for external cancellation
   * @returns The execution result
   * @throws TimeoutError If the timeout is reached
   */
  async executeWithTimeout<T>(
    fn: () => Promise<T>,
    timeout?: number,
    signal?: AbortSignal,
  ): Promise<T> {
    const actualTimeout = timeout ?? this.defaultTimeout;

    // Handle zero or negative timeout (no timeout)
    if (actualTimeout <= 0) {
      return await fn();
    }

    // Combine timeout with abort signal if provided
    const { clearTimeout } = combineTimeoutWithSignal(actualTimeout, signal);

    try {
      // Execute function and race against timeout
      const result = await Promise.race([
        fn(),
        new Promise<never>((_, reject) => {
          setTimeout(() => {
            reject(
              new TimeoutError(`Tool execution timeout after ${actualTimeout}ms`, actualTimeout),
            );
          }, actualTimeout);
        }),
      ]);

      return result;
    } catch (error) {
      // If aborted via signal, throw appropriate error
      if (signal?.aborted) {
        throw new Error("Tool execution aborted", { cause: error });
      }
      throw error;
    } finally {
      // Clean up resources
      clearTimeout();
    }
  }

  /**
   * Create a default timeout controller
   */
  static createDefault(): TimeoutController {
    return new TimeoutController(30000);
  }

  /**
   * Create a timeout-free controller
   */
  static createNoTimeout(): TimeoutController {
    return new TimeoutController(0);
  }
}
