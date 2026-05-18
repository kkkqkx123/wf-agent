/**
 * Unified Interruption Handler
 * Provides a consistent abstraction for interruption handling across SDK
 *
 * Design Principles:
 * - Eliminate race conditions by enforcing signal collaboration
 * - Reduce code duplication with unified wrapper
 * - Ensure type safety and clear contracts
 * - Support both Result-based and status-based return patterns
 */

import {
  createNeverAbortSignal,
} from "./abort-signal-utils.js";
import { isAbortError } from "../error-utils.js";
import {
  checkExecutionInterruption,
  type ExecutionInterruptionCheckResult,
  shouldContinue as shouldContinueExecution,
} from "./execution-interruption-utils.js";

/**
 * Execute an operation with comprehensive interruption handling
 *
 * This function ensures proper interruption handling by:
 * 1. Checking signal before execution
 * 2. Passing signal to the operation for active collaboration
 * 3. Checking signal after execution completes
 * 4. Catching AbortError exceptions
 *
 * @param operation Async operation that accepts AbortSignal
 * @param signal AbortSignal for cancellation
 * @returns Result object indicating success or interruption
 *
 * @example
 * ```typescript
 * const result = await executeWithInterruptionHandling(
 *   async (signal) => {
 *     return fetchData(url, { signal });
 *   },
 *   controller.signal
 * );
 *
 * if (!result.success) {
 *   return result.interruption;
 * }
 *
 * const data = result.result;
 * ```
 */
export async function executeWithInterruptionHandling<T>(
  operation: (signal: AbortSignal) => Promise<T>,
  signal?: AbortSignal,
): Promise<
  | { success: true; result: T }
  | { success: false; interruption: ExecutionInterruptionCheckResult }
> {
  // Use never-abort signal if no signal provided
  const effectiveSignal = signal ?? createNeverAbortSignal();

  // Pre-execution check
  const preCheck = checkExecutionInterruption(effectiveSignal);
  if (!shouldContinueExecution(preCheck)) {
    return {
      success: false,
      interruption: preCheck,
    };
  }

  try {
    // Execute operation with signal
    const result = await operation(effectiveSignal);

    // Post-execution check
    const postCheck = checkExecutionInterruption(effectiveSignal);
    if (!shouldContinueExecution(postCheck)) {
      return {
        success: false,
        interruption: postCheck,
      };
    }

    return {
      success: true,
      result,
    };
  } catch (error) {
    // Handle InterruptionError (thrown from nested operations)
    if (error instanceof Error && error.name === "InterruptionError" && "interruption" in error) {
      return {
        success: false,
        interruption: (error as Error & { interruption?: ExecutionInterruptionCheckResult }).interruption!,
      };
    }

    // Handle AbortError
    if (isAbortError(error) && effectiveSignal.aborted) {
      const interruption = checkExecutionInterruption(effectiveSignal);
      return {
        success: false,
        interruption,
      };
    }

    // Re-throw non-abort errors
    throw error;
  }
}

/**
 * Execute an async iterator with interruption handling
 *
 * @param iterable Async iterable source
 * @param signal AbortSignal for cancellation
 * @returns Async generator yielding values or interruption result
 *
 * @example
 * ```typescript
 * for await (const item of iterateWithInterruption(dataStream, signal)) {
 *   if (item.type === "interrupted") {
 *     break;
 *   }
 *   process(item.value);
 * }
 * ```
 */
export async function* iterateWithInterruptionHandling<T>(
  iterable: AsyncIterable<T>,
  signal?: AbortSignal,
): AsyncGenerator<
  | { type: "value"; value: T }
  | { type: "interrupted"; interruption: ExecutionInterruptionCheckResult },
  void,
  unknown
> {
  const effectiveSignal = signal ?? createNeverAbortSignal();
  const iterator = iterable[Symbol.asyncIterator]();

  try {
    while (true) {
      // Check before fetching next value
      const preCheck = checkExecutionInterruption(effectiveSignal);
      if (!shouldContinueExecution(preCheck)) {
        yield { type: "interrupted", interruption: preCheck };
        return;
      }

      let nextResult: IteratorResult<T>;
      try {
        nextResult = await iterator.next();
      } catch (error) {
        // Handle AbortError from iterator.next()
        if (isAbortError(error) && effectiveSignal.aborted) {
          const interruption = checkExecutionInterruption(effectiveSignal);
          yield { type: "interrupted", interruption };
          return;
        }
        throw error;
      }

      if (nextResult.done) {
        break;
      }

      // Check after fetching value
      const postCheck = checkExecutionInterruption(effectiveSignal);
      if (!shouldContinueExecution(postCheck)) {
        yield { type: "interrupted", interruption: postCheck };
        return;
      }

      yield { type: "value", value: nextResult.value };
    }
  } finally {
    // Clean up iterator
    if (iterator.return) {
      await iterator.return();
    }
  }
}
