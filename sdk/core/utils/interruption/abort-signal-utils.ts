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
 */

/**
 * Basic interruption check result (domain-agnostic)
 * Only contains basic interruption states without domain-specific context
 */
export type InterruptionCheckResult = { type: "continue" } | { type: "aborted"; reason?: unknown };

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
 * @returns AbortSignal
 */
export function createNeverAbortSignal(): AbortSignal {
  const controller = new AbortController();
  // Returns a signal that never aborts without calling abort.
  return controller.signal;
}

/**
 * Combine multiple AbortSignals into one
 * Returns a new signal that aborts when any of the input signals abort
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

  // Cleanup: when the combined signal itself is aborted (e.g. by caller),
  // remove all listeners from input signals to prevent memory leaks.
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
 * @param fn Async function to execute
 * @param signal AbortSignal
 * @returns Result containing either the function result or an error
 */
export async function withAbortSignal<T>(
  fn: () => Promise<T>,
  signal?: AbortSignal,
): Promise<{ ok: true; value: T } | { ok: false; error: Error }> {
  try {
    if (signal?.aborted) {
      const originalError = signal.reason as Error;
      const newError = new Error(originalError?.message || "Operation aborted");
      // Preserve the original error in the cause chain for debugging
      if (originalError) {
        newError.cause = originalError;
      }
      return {
        ok: false,
        error: newError,
      };
    }

    const result = await fn();

    if (signal?.aborted) {
      const originalError = signal.reason as Error;
      const newError = new Error(originalError?.message || "Operation aborted");
      if (originalError) {
        newError.cause = originalError;
      }
      return {
        ok: false,
        error: newError,
      };
    }

    return { ok: true, value: result };
  } catch (error) {
    if (
      signal?.aborted &&
      (error instanceof DOMException || (error instanceof Error && error.name === "AbortError"))
    ) {
      const originalError = signal.reason as Error;
      const newError = new Error(originalError?.message || "Operation aborted");
      if (originalError) {
        newError.cause = originalError;
      }
      return {
        ok: false,
        error: newError,
      };
    }
    return {
      ok: false,
      error: error instanceof Error ? error : new Error(String(error)),
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
