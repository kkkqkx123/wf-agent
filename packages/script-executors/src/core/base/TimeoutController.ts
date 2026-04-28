/**
 * Timeout Controller
 * Manages the timeout control for script execution
 */

/**
 * Timeout Controller Configuration
 */
export interface TimeoutControllerConfig {
  /** Default timeout in milliseconds */
  defaultTimeout?: number;
}

/**
 * timeout controller
 */
export class TimeoutController {
  private defaultTimeout: number;

  constructor(config: TimeoutControllerConfig = {}) {
    this.defaultTimeout = config.defaultTimeout ?? 30000;
  }

  /**
   * Execute the function and apply the timeout control
   * @param fn The function to execute
   * @param timeout timeout in milliseconds
   * @param signal Abort signal (optional)
   * @returns Result of function execution
   * @throws Error If the function times out or is aborted.
   */
  async executeWithTimeout<T>(
    fn: () => Promise<T>,
    timeout: number = this.defaultTimeout,
    signal?: AbortSignal,
  ): Promise<T> {
    // Creating a Timeout Promise
    const timeoutPromise = new Promise<never>((_, reject) => {
      const timeoutId = setTimeout(() => {
        reject(new Error(`Execution timeout after ${timeout}ms`));
      }, timeout);

      // If an abort signal is provided, listen for an abort event.
      if (signal) {
        const abortHandler = () => {
          clearTimeout(timeoutId);
          reject(new Error("Execution aborted by signal"));
        };

        signal.addEventListener("abort", abortHandler, { once: true });
      }
    });

    // Execute the function and apply a timeout
    return Promise.race([fn(), timeoutPromise]);
  }

  /**
   * Creating a Default Timeout Controller
   * @returns Default Timeout Controller instance
   */
  static createDefault(): TimeoutController {
    return new TimeoutController();
  }
}
