/**
 * Generic AbortSignal Utility Functions
 * Provides domain-agnostic interruption handling based on AbortSignal
 *
 * Design Principles:
 * - Use return values to indicate control flow state (continue/aborted)
 * - Avoid using exception handling for expected interruptions
 * - Provide type-safe utility functions
 * - No dependency on workflow or agent-specific concepts
 * - Reusable across all modules (SDK, Agent, Workflow, etc.)
 */

import { getGlobalLogger } from "../../logger/global-logger.js";

// Get logger for abort signal utils
const logger = getGlobalLogger().child("abort-signal-utils", { pkg: "common-utils" });

/**
 * Get the abort reason for AbortSignal
 * @param signal AbortSignal
 * @returns Abort reason (Error or other value)
 */
function getAbortReason(signal?: AbortSignal): unknown {
  return signal?.reason;
}

/**
 * Creating an AbortSignal that Never Stops
 * @returns AbortSignal
 */
function createNeverAbortSignal(): AbortSignal {
  const controller = new AbortController();
  // Returns a signal that never aborts without calling abort.
  return controller.signal;
}

/**
 * Generic interruption check result
 * Only contains basic interruption states without domain-specific context
 */
export type InterruptionCheckResult =
  | { type: "continue" }
  | { type: "aborted"; reason?: unknown };

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

/**
 * Determine whether to continue execution
 * @param result The result of the interruption check
 * @returns Whether to continue
 */
export function shouldContinue(result: InterruptionCheckResult): boolean {
  return result.type === "continue";
}

/**
 * Check if an interruption has occurred
 * @param result The result of the interruption check
 * @returns Whether an interruption has occurred (with type narrowing)
 */
export function isInterrupted(
  result: InterruptionCheckResult,
): result is Extract<InterruptionCheckResult, { type: "aborted" }> {
  return result.type === "aborted";
}

/**
 * Get a friendly description of the interrupt
 * @param result Interrupt check result
 * @returns Interrupt description string
 */
export function getInterruptionDescription(result: InterruptionCheckResult): string {
  if (result.type === "continue") {
    return "Execution continuing";
  }
  // result.type === "aborted"
  return result.reason ? String(result.reason) : "Operation aborted";
}

/**
 * Wrap an asynchronous function with proper interruption support.
 *
 * This function ensures that the provided function can be properly interrupted
 * by passing the AbortSignal to it. The function should respect the signal
 * and throw AbortError or check signal.aborted when appropriate.
 *
 * @param fn The asynchronous function that accepts an AbortSignal
 * @param signal The AbortSignal for cancellation
 * @returns Object indicating completion status or interruption
 *
 * @example
 * ```typescript
 * const result = await withInterruptionCheck(
 *   (signal) => fetchData(url, { signal }),
 *   controller.signal
 * );
 * ```
 */
export async function withInterruptionCheck<T>(
  fn: (signal: AbortSignal) => Promise<T>,
  signal?: AbortSignal,
): Promise<
  | { result: T; status: "completed" }
  | { status: "interrupted"; interruption: InterruptionCheckResult }
> {
  // Create a never-abort signal if none provided
  const effectiveSignal = signal ?? createNeverAbortSignal();
  
  // Check before execution
  const preCheck = checkInterruption(effectiveSignal);
  if (!shouldContinue(preCheck)) {
    return {
      status: "interrupted",
      interruption: preCheck,
    };
  }

  try {
    // Pass signal to fn so it can respond to cancellation
    const result = await fn(effectiveSignal);
    
    // Final check after execution
    const postCheck = checkInterruption(effectiveSignal);
    if (!shouldContinue(postCheck)) {
      return {
        status: "interrupted",
        interruption: postCheck,
      };
    }
    
    return {
      result,
      status: "completed",
    };
  } catch (error) {
    // Handle abort errors thrown by fn
    if (
      effectiveSignal.aborted &&
      (error instanceof DOMException || (error instanceof Error && error.name === "AbortError"))
    ) {
      return {
        status: "interrupted",
        interruption: checkInterruption(effectiveSignal),
      };
    }
    throw error;
  }
}

/**
 * Create an asynchronous iterator wrapper with an interrupt check
 * @param iterable An asynchronously iterable object
 * @param signal An AbortSignal
 * @returns The wrapped asynchronous iterator
 */
export async function* withInterruptionCheckIter<T>(
  iterable: AsyncIterable<T>,
  signal?: AbortSignal,
): AsyncGenerator<T | InterruptionCheckResult, void, unknown> {
  const iterator = iterable[Symbol.asyncIterator]();

  try {
    while (true) {
      // Check for interrupts before fetching next value
      const interruption = checkInterruption(signal);
      if (!shouldContinue(interruption)) {
        yield interruption;
        return;
      }

      let nextResult: IteratorResult<T>;
      try {
        nextResult = await iterator.next();
      } catch (error) {
        // If iterator.next() throws an abort error, handle it
        if (
          signal?.aborted &&
          (error instanceof DOMException || (error instanceof Error && error.name === "AbortError"))
        ) {
          yield checkInterruption(signal);
          return;
        }
        throw error;
      }

      const { value, done } = nextResult;
      if (done) break;

      // Check for interrupts after fetching value (in case signal was aborted during async operation)
      const postFetchInterruption = checkInterruption(signal);
      if (!shouldContinue(postFetchInterruption)) {
        yield postFetchInterruption;
        return;
      }

      yield value;
    }
  } finally {
    // Make sure to clean up the iterator
    try {
      if (iterator.return) {
        await iterator.return();
      }
    } catch (cleanupError) {
      // Log cleanup error but don't throw during cleanup phase
      logger.warn("Failed to cleanup iterator", { error: cleanupError });
    }
  }
}
