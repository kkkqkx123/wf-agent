/**
 * Abort Tool Functions
 * Provides basic wrapping functionality based on AbortSignal
 *
 * Design Principles:
 * - Harmonize the use of AbortSignal as the interrupt mechanism.
 * - Simplify interrupt handling for asynchronous operations
 * - Provide type-safe utility functions
 *
 * @deprecated These utilities have been moved to SDK internal implementation.
 * Use sdk/core/utils/interruption/ instead.
 */
import { TimeoutError } from "@wf-agent/types";

/**
 * Check AbortSignal and execute the function
 * @param fn Asynchronous function
 * @param signal AbortSignal
 * @returns Result of function execution
 * @throws Throws abort reason when signal has aborted.
 */
export function withAbortSignal<T>(fn: () => Promise<T>, signal?: AbortSignal): Promise<T> {
  if (signal?.aborted) {
    const reason = signal.reason;
    if (reason instanceof Error) {
      throw reason;
    }
    throw new Error("This operation was aborted");
  }
  return fn();
}

/**
 * Check AbortSignal and execute function (supports passing signal to function)
 * @param fn Asynchronous function (receives AbortSignal parameter)
 * @param signal AbortSignal
 * @returns Result of function execution
 * @throws Throws abort reason when signal has aborted.
 */
export function withAbortSignalArg<T>(
  fn: (signal: AbortSignal) => Promise<T>,
  signal?: AbortSignal,
): Promise<T> {
  if (signal?.aborted) {
    const reason = signal.reason;
    if (reason instanceof Error) {
      throw reason;
    }
    throw new Error("This operation was aborted");
  }
  if (!signal) {
    throw new Error("Signal is required for withAbortSignalArg");
  }
  return fn(signal);
}

/**
 * Creating an AbortSignal with a timeout
 * @param timeoutMs timeout in milliseconds
 * @returns AbortController and AbortSignal
 */
export function createTimeoutSignal(timeoutMs: number): {
  controller: AbortController;
  signal: AbortSignal;
} {
  const controller = new AbortController();
  const timeoutId = setTimeout(() => {
    // Use the SDK's TimeoutError instead of a normal Error.
    const timeoutError = new TimeoutError(`Operation timed out after ${timeoutMs}ms`, timeoutMs);
    controller.abort(timeoutError);
  }, timeoutMs);

  // Clear Timer
  const originalAbort = controller.abort.bind(controller);
  controller.abort = (reason?: unknown) => {
    clearTimeout(timeoutId);
    return originalAbort(reason);
  };

  return { controller, signal: controller.signal };
}

/**
 * Combining Multiple AbortSignals
 * When any of the signals aborts, the combined signals will also abort
 * @param signals AbortSignal array
 * @returns AbortController and the combined AbortSignal.
 */
export function combineAbortSignals(signals: AbortSignal[]): {
  controller: AbortController;
  signal: AbortSignal;
} {
  const controller = new AbortController();

  // Listen to all signals
  const abortHandlers = signals.map(signal => {
    if (signal.aborted) {
      // If already aborted, immediately abort the combined signal
      controller.abort(signal.reason);
      return () => {};
    }

    const handler = () => {
      controller.abort(signal.reason);
    };
    signal.addEventListener("abort", handler);
    return () => signal.removeEventListener("abort", handler);
  });

  // Clear event listeners
  const originalAbort = controller.abort.bind(controller);
  controller.abort = (reason?: unknown) => {
    abortHandlers.forEach(cleanup => cleanup());
    return originalAbort(reason);
  };

  return { controller, signal: controller.signal };
}

/**
 * Check if AbortSignal is aborted
 * @param signal AbortSignal
 * @returns if the signal has been aborted
 */
export function isAborted(signal?: AbortSignal): boolean {
  return signal?.aborted ?? false;
}

/**
 * Get the abort reason for AbortSignal
 * @param signal AbortSignal
 * @returns Abort reason (Error or other value)
 */
export function getAbortReason(signal?: AbortSignal): unknown {
  return signal?.reason;
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
 * Wrapping asynchronous operations with timeout and interrupt support
 * @param fn Asynchronous functions
 * @param options
 * @returns Result of function execution
 */
export async function withTimeoutAndAbort<T>(
  fn: (signal: AbortSignal) => Promise<T>,
  options: {
    signal?: AbortSignal;
    timeoutMs?: number;
  },
): Promise<T> {
  const { signal, timeoutMs } = options;

  // If there is no timeout and no signal, just execute
  if (!signal && !timeoutMs) {
    return await fn(createNeverAbortSignal());
  }

  // Create timeout signal
  let timeoutController: AbortController | undefined;
  if (timeoutMs) {
    const { controller } = createTimeoutSignal(timeoutMs);
    timeoutController = controller;
  }

  // Combined signal
  const signals: AbortSignal[] = [];
  if (signal) signals.push(signal);
  if (timeoutController) signals.push(timeoutController.signal);

  let combinedController: AbortController | undefined;
  let combinedSignal: AbortSignal;

  if (signals.length === 0) {
    combinedSignal = createNeverAbortSignal();
  } else if (signals.length === 1) {
    combinedSignal = signals[0]!;
  } else {
    const result = combineAbortSignals(signals);
    combinedController = result.controller;
    combinedSignal = result.signal;
  }

  // Check if the combination signal has been aborted
  if (combinedSignal.aborted) {
    throw combinedSignal.reason || new Error("This operation was aborted");
  }

  try {
    // Execute the function and return the result.
    // The function should listen for the signal and handle the termination correctly.
    return await fn(combinedSignal);
  } finally {
    // Ensure that resources are cleaned up, but actively suspend controllers only when not suspended
    if (timeoutController && !timeoutController.signal.aborted) {
      timeoutController.abort();
    }
    if (combinedController && !combinedController.signal.aborted) {
      combinedController.abort();
    }
  }
}
