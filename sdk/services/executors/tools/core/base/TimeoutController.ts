/**
 * Timeout Controller
 * Responsible for controlling execution timeouts
 */

import { TimeoutError } from "@wf-agent/types";
import { createAbortError } from "@wf-agent/common-utils";

/**
 * Timeout Controller
 */
export class TimeoutController {
  constructor(private defaultTimeout: number = 30000) {}

  /**
   * Timed execution
   * @param fn The function to be executed
   * @param timeout The timeout period in milliseconds, using the default value set by the constructor by default
   * @returns The execution result
   * @throws TimeoutError If the timeout is reached
   */
  async executeWithTimeout<T>(
    fn: () => Promise<T>,
    timeout?: number,
    signal?: AbortSignal,
  ): Promise<T> {
    const actualTimeout = timeout ?? this.defaultTimeout;
    let timeoutId: NodeJS.Timeout | undefined;
    let abortListener: (() => void) | undefined;

    // Create a timeout Promise
    const timeoutPromise = new Promise<never>((_, reject) => {
      timeoutId = setTimeout(() => {
        reject(new TimeoutError(`Tool execution timeout after ${actualTimeout}ms`, actualTimeout));
      }, actualTimeout);
    });

    // Create a cancelable Promise
    let abortPromise: Promise<never> | undefined;
    if (signal) {
      abortPromise = new Promise<never>((_, reject) => {
        const onAbort = () => {
          reject(createAbortError("Tool execution aborted", signal));
        };

        if (signal.aborted) {
          onAbort();
        } else {
          abortListener = () => {
            onAbort();
          };
          signal.addEventListener("abort", abortListener);
        }
      });
    }

    try {
      // Competitive execution, timeouts, and termination
      const promises: Array<Promise<T | never>> = [fn(), timeoutPromise];
      if (abortPromise) {
        promises.push(abortPromise);
      }
      return await Promise.race(promises);
    } finally {
      // Clean up resources
      if (timeoutId) {
        clearTimeout(timeoutId);
      }
      if (abortListener && signal) {
        signal.removeEventListener("abort", abortListener);
      }
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
