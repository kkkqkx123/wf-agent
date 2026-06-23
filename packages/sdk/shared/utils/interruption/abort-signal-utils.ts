/**
 * AbortSignal Utility Functions
 * Provides domain-agnostic interruption handling based on AbortSignal
 *
 * Design Principles:
 * - Use return values to indicate control flow state (continue/aborted)
 * - Avoid using exception handling for expected interruptions
 * - Provide type-safe utility functions
 * - No dependency on workflow or agent-specific concepts
 * - Reusable across all SDK modules
 * - Leverage Node.js 20+ native AbortSignal features (AbortSignal.any)
 */

/**
 * Basic interruption check result (domain-agnostic)
 * Only contains basic interruption states without domain-specific context
 */
export type InterruptionCheckResult = { type: "continue" } | { type: "aborted"; reason?: unknown };

/**
 * Comprehensive result type for withAbortSignal
 * Distinguishes between interruption and business errors
 */
export type WithAbortSignalResult<T> =
  | { ok: true; value: T }
  | { ok: false; error: Error; isAborted: true }
  | { ok: false; error: Error; isAborted: false };

/**
 * Check if signal is aborted
 * @param signal AbortSignal
 * @returns Whether the signal is aborted
 */
export function isAborted(signal?: AbortSignal): boolean {
  return signal?.aborted ?? false;
}

/**
 * Creating an AbortSignal that Never Stops
 * Uses a singleton pattern to minimize memory overhead and GC pressure.
 * @returns AbortSignal that will never abort
 */
const NEVER_ABORT_CONTROLLER = new AbortController();

export function createNeverAbortSignal(): AbortSignal {
  return NEVER_ABORT_CONTROLLER.signal;
}

/**
 * Combine multiple AbortSignals into one
 * Returns a new signal that aborts when any of the input signals abort
 *
 * Implementation: Uses native AbortSignal.any() (Node.js 20.3+) with fallback
 * for older versions.
 *
 * @param signals Array of AbortSignals
 * @returns Combined AbortSignal
 */
export function combineAbortSignals(signals: (AbortSignal | undefined)[]): AbortSignal {
  const validSignals = signals.filter((s): s is AbortSignal => s !== undefined);

  if (validSignals.length === 0) {
    return createNeverAbortSignal();
  }

  if (validSignals.length === 1) {
    return validSignals[0]!;
  }

  // Use native AbortSignal.any() (Node.js 20.3+) for optimal performance
  if (typeof AbortSignal.any === "function") {
    return AbortSignal.any(validSignals);
  }

  // Fallback implementation for older Node.js versions
  return combineAbortSignalsFallback(validSignals);
}

/**
 * Fallback implementation for combineAbortSignals
 * Used when AbortSignal.any() is not available (legacy Node.js versions)
 */
function combineAbortSignalsFallback(validSignals: AbortSignal[]): AbortSignal {
  const controller = new AbortController();

  // If any signal is already aborted, abort immediately
  for (const signal of validSignals) {
    if (signal.aborted) {
      controller.abort(signal.reason);
      return controller.signal;
    }
  }

  // Listen to all signals and track listeners for cleanup
  const abortListeners: Array<[AbortSignal, () => void]> = [];

  for (const signal of validSignals) {
    const handler = () => {
      if (!controller.signal.aborted) {
        controller.abort(signal.reason);
      }
    };
    signal.addEventListener("abort", handler, { once: true });
    abortListeners.push([signal, handler]);
  }

  // Cleanup: when the combined signal itself is aborted,
  // remove all listeners from input signals to prevent memory leaks
  controller.signal.addEventListener(
    "abort",
    () => {
      for (const [signal, handler] of abortListeners) {
        signal.removeEventListener("abort", handler);
      }
    },
    { once: true },
  );

  return controller.signal;
}

/**
 * Wrap async function with AbortSignal support
 * Automatically handles abort errors and returns Result type
 *
 * Improvements:
 * - Function receives signal parameter for periodic interruption checks
 * - Returns WithAbortSignalResult with isAborted flag to distinguish interruption from errors
 * - Enables zero-overhead interruption detection within long-running operations
 *
 * Usage:
 * ```typescript
 * const result = await withAbortSignal(
 *   async (signal) => {
 *     for (let i = 0; i < 1000; i++) {
 *       if (signal?.aborted) break;  // Check interruption periodically
 *       await doWork(i);
 *     }
 *     return "done";
 *   },
 *   abortSignal
 * );
 *
 * if (!result.ok) {
 *   if (result.isAborted) {
 *     console.log("Operation interrupted by user");
 *   } else {
 *     console.log("Operation failed:", result.error);
 *   }
 * }
 * ```
 *
 * @param fn Async function that receives the signal and can check it periodically
 * @param signal AbortSignal
 * @returns Result with ok flag, value, error, and isAborted distinction
 */
export async function withAbortSignal<T>(
  fn: (signal?: AbortSignal) => Promise<T>,
  signal?: AbortSignal,
): Promise<WithAbortSignalResult<T>> {
  try {
    // Pre-execution check: signal already aborted
    if (signal?.aborted) {
      const originalError = signal.reason as Error;
      const newError = new Error(originalError?.message || "Operation aborted");
      if (originalError) {
        newError.cause = originalError;
      }
      return {
        ok: false,
        error: newError,
        isAborted: true,
      };
    }

    // Execute with signal passed to function for periodic checks
    const result = await fn(signal);

    // Post-execution check: signal aborted during execution
    if (signal?.aborted) {
      const originalError = signal.reason as Error;
      const newError = new Error(originalError?.message || "Operation aborted");
      if (originalError) {
        newError.cause = originalError;
      }
      return {
        ok: false,
        error: newError,
        isAborted: true,
      };
    }

    return { ok: true, value: result };
  } catch (error) {
    // Distinguish between abort errors and other errors
    const isAbortError =
      signal?.aborted && (error instanceof DOMException || (error instanceof Error && error.name === "AbortError"));

    if (isAbortError) {
      const originalError = signal?.reason as Error;
      const newError = new Error(originalError?.message || "Operation aborted");
      if (originalError) {
        newError.cause = originalError;
      }
      return {
        ok: false,
        error: newError,
        isAborted: true,
      };
    }

    return {
      ok: false,
      error: error instanceof Error ? error : new Error(String(error)),
      isAborted: false,
    };
  }
}

/**
 * Get the abort reason from a signal
 * @param signal AbortSignal
 * @returns The abort reason or undefined
 */
function getAbortReason(signal: AbortSignal): unknown {
  return signal.reason;
}

/**
 * Check the AbortSignal and return the interruption status
 * @param signal AbortSignal
 * @returns The result of the interruption check
 */
export function checkInterruption(signal?: AbortSignal): InterruptionCheckResult {
  if (!signal) {
    return { type: "continue" };
  }

  if (!signal.aborted) {
    return { type: "continue" };
  }

  const reason = getAbortReason(signal);

  // Return aborted state with reason
  return {
    type: "aborted",
    reason,
  };
}
