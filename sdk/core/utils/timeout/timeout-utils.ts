/**
 * Timeout Utility Functions
 * 
 * Helper functions for working with timeouts, AbortSignals, and promises.
 */

/**
 * Combine a timeout with an AbortSignal
 * 
 * Creates a new AbortSignal that will be aborted when either:
 * - The timeout expires
 * - The original signal is aborted
 * 
 * @param duration Timeout duration in milliseconds
 * @param signal Optional existing AbortSignal to combine with
 * @returns Object containing the combined signal and a cleanup function
 * 
 * @example
 * ```typescript
 * const { signal, clearTimeout } = combineTimeoutWithSignal(5000, existingSignal);
 * try {
 *   await fetch(url, { signal });
 * } finally {
 *   clearTimeout();
 * }
 * ```
 */
export function combineTimeoutWithSignal(
  duration: number,
  signal?: AbortSignal
): { signal: AbortSignal; clearTimeout: () => void } {
  // If no signal provided, create a simple timeout controller
  if (!signal) {
    const controller = new AbortController();
    const timerId = setTimeout(() => controller.abort(), duration);

    return {
      signal: controller.signal,
      clearTimeout: () => clearTimeout(timerId),
    };
  }

  // If signal is already aborted, return immediately
  if (signal.aborted) {
    return {
      signal,
      clearTimeout: () => {}, // No-op
    };
  }

  // Create a new controller that combines both signals
  const controller = new AbortController();
  const timerId = setTimeout(() => controller.abort(), duration);

  // Listen to the original signal
  const abortHandler = () => {
    clearTimeout(timerId);
    controller.abort(signal.reason);
  };

  signal.addEventListener("abort", abortHandler, { once: true });

  return {
    signal: controller.signal,
    clearTimeout: () => {
      clearTimeout(timerId);
      signal.removeEventListener("abort", abortHandler);
    },
  };
}

/**
 * Create a promise that rejects with a timeout error if it doesn't complete in time
 * 
 * @param promise The promise to wrap with timeout
 * @param duration Timeout duration in milliseconds
 * @param message Optional custom error message
 * @returns Promise that resolves with the original result or rejects on timeout
 * 
 * @example
 * ```typescript
 * const result = await createTimeoutPromise(
 *   fetchData(),
 *   5000,
 *   'Data fetch timed out'
 * );
 * ```
 */
export async function createTimeoutPromise<T>(
  promise: Promise<T>,
  duration: number,
  message?: string
): Promise<T> {
  let timeoutHandle: NodeJS.Timeout;

  const timeoutPromise = new Promise<never>((_, reject) => {
    timeoutHandle = setTimeout(() => {
      reject(new Error(message || `Operation timed out after ${duration}ms`));
    }, duration);
  });

  try {
    return await Promise.race([promise, timeoutPromise]);
  } finally {
    clearTimeout(timeoutHandle!);
  }
}

/**
 * Calculate adaptive timeout based on retry count
 * 
 * Increases timeout exponentially with each retry to handle transient issues.
 * 
 * @param baseTimeout Base timeout in milliseconds
 * @param retryCount Current retry count (0-based)
 * @param maxTimeout Maximum allowed timeout in milliseconds
 * @returns Calculated timeout duration
 * 
 * @example
 * ```typescript
 * // First attempt: 5000ms
 * // Second attempt: 10000ms
 * // Third attempt: 20000ms (capped at maxTimeout)
 * const timeout = calculateAdaptiveTimeout(5000, retryCount, 30000);
 * ```
 */
export function calculateAdaptiveTimeout(
  baseTimeout: number,
  retryCount: number,
  maxTimeout: number
): number {
  // Exponential backoff: baseTimeout * 2^retryCount
  const calculatedTimeout = baseTimeout * Math.pow(2, retryCount);

  // Cap at maximum timeout
  return Math.min(calculatedTimeout, maxTimeout);
}

/**
 * Create a delay promise
 * 
 * @param ms Delay in milliseconds
 * @param signal Optional AbortSignal to cancel the delay
 * @returns Promise that resolves after the delay
 * 
 * @example
 * ```typescript
 * await delay(1000); // Wait 1 second
 * ```
 */
export function delay(ms: number, signal?: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) {
      reject(signal.reason || new Error("Delay aborted"));
      return;
    }

    const timerId = setTimeout(() => resolve(), ms);

    if (signal) {
      signal.addEventListener(
        "abort",
        () => {
          clearTimeout(timerId);
          reject(signal.reason || new Error("Delay aborted"));
        },
        { once: true }
      );
    }
  });
}

/**
 * Execute a function with a timeout
 * 
 * @param fn Function to execute
 * @param duration Timeout duration in milliseconds
 * @param options Optional configuration
 * @returns Result of the function
 * 
 * @example
 * ```typescript
 * const result = await withTimeout(
 *   () => fetchData(),
 *   5000,
 *   { onTimeout: () => console.log('Timed out') }
 * );
 * ```
 */
export async function withTimeout<T>(
  fn: () => Promise<T>,
  duration: number,
  options?: {
    onTimeout?: () => void | Promise<void>;
    message?: string;
  }
): Promise<T> {
  let timedOut = false;

  const timeoutPromise = new Promise<never>((_, reject) => {
    setTimeout(async () => {
      timedOut = true;
      if (options?.onTimeout) {
        await options.onTimeout();
      }
      reject(new Error(options?.message || `Operation timed out after ${duration}ms`));
    }, duration);
  });

  try {
    return await Promise.race([fn(), timeoutPromise]);
  } catch (error) {
    if (timedOut) {
      throw error;
    }
    throw error;
  }
}

/**
 * Check if an error is a timeout error
 * 
 * @param error Error to check
 * @returns true if the error is timeout-related
 */
export function isTimeoutError(error: unknown): boolean {
  if (!(error instanceof Error)) {
    return false;
  }

  return (
    error.message.includes("timed out") ||
    error.message.includes("timeout") ||
    error.name === "TimeoutError"
  );
}
