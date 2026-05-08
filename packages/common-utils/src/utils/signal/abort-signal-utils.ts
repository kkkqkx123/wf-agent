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

import { getAbortReason } from "./abort-utils.js";

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
 * @returns Whether an interruption has occurred
 */
export function isInterrupted(result: InterruptionCheckResult): boolean {
  return result.type !== "continue";
}

/**
 * Get a friendly description of the interrupt
 * @param result Interrupt check result
 * @returns Interrupt description string
 */
export function getInterruptionDescription(result: InterruptionCheckResult): string {
  switch (result.type) {
    case "continue":
      return "Execution continuing";
    case "aborted":
      return result.reason ? String(result.reason) : "Operation aborted";
    default:
      return "Unknown interruption state";
  }
}

/**
 * Wrap an asynchronous function to automatically handle interruption checks
 * @param fn The asynchronous function
 * @param signal The AbortSignal
 * @returns The function execution result and status
 */
export async function withInterruptionCheck<T>(
  fn: () => Promise<T>,
  signal?: AbortSignal,
): Promise<
  | { result: T; status: "completed" }
  | { status: "interrupted"; interruption: InterruptionCheckResult }
> {
  const interruption = checkInterruption(signal);

  if (!shouldContinue(interruption)) {
    return {
      status: "interrupted",
      interruption,
    };
  }

  try {
    const result = await fn();
    return {
      result,
      status: "completed",
    };
  } catch (error) {
    // If an abort error is thrown inside the function, convert it into a return value
    if (error instanceof Error && error.name === "AbortError") {
      const interruption = checkInterruption(signal);
      return {
        status: "interrupted",
        interruption,
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
      // Check for interrupts
      const interruption = checkInterruption(signal);
      if (!shouldContinue(interruption)) {
        yield interruption;
        return;
      }

      const { value, done } = await iterator.next();
      if (done) break;

      yield value;
    }
  } finally {
    // Make sure to clean up the iterator
    if (iterator.return) {
      await iterator.return();
    }
  }
}
