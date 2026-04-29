/**
 * Workflow Execution Interruption Utility Functions
 * Use a return value labeling system to handle control flow interruptions in place of an error system.
 *
 * Design Principles:
 * - Use return values to indicate the control flow state (continue/paused/stopped).
 * - Avoid using exception handling for interruptions that are expected to occur.
 * - Provide type-safe utility functions.
 * - Maintain compatibility with AbortSignal.
 */

import type { InterruptionType } from "@wf-agent/types";
import type { InterruptionCheckResult, InterruptionInfo } from "./interruption-types.js";
import { getAbortReason } from "./abort-utils.js";

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

  // Check if it is a workflow execution interruption.
  if (reason && typeof reason === "object" && "interruptionType" in reason) {
    const interruption = reason as Record<string, unknown>;
    const type = interruption["interruptionType"] as InterruptionType;
    const executionId = interruption["executionId"] as string | undefined;
    const nodeId = interruption["nodeId"] as string | undefined;

    if (type === "PAUSE") {
      return {
        type: "paused",
        threadId: executionId, // Keep threadId for backward compatibility
        nodeId: nodeId || "unknown",
      };
    } else if (type === "STOP") {
      return {
        type: "stopped",
        threadId: executionId, // Keep threadId for backward compatibility
        nodeId: nodeId || "unknown",
      };
    }
  }

  // Normal termination
  return {
    type: "aborted",
    reason,
  };
}

/**
 * Create workflow execution interruption information
 * @param type: Type of interruption
 * @param executionId: Execution ID
 * @param nodeId: Node ID
 * @returns: Interruption information
 */
export function createInterruptionInfo(
  type: Exclude<InterruptionType, null>,
  executionId: string,
  nodeId: string,
): InterruptionInfo {
  return {
    type,
    threadId: executionId, // Keep threadId for backward compatibility
    nodeId,
    timestamp: Date.now(),
  };
}

/**
 * Determine whether to continue execution
 * @param result: The result of the interruption check
 * @returns: Whether to continue
 */
export function shouldContinue(result: InterruptionCheckResult): boolean {
  return result.type === "continue";
}

/**
 * Check if an interruption has occurred
 * @param result: The result of the interruption check
 * @returns: Whether an interruption has occurred
 */
export function isInterrupted(result: InterruptionCheckResult): boolean {
  return result.type !== "continue";
}

/**
 * Get the interrupt type
 * @param result: The result of the interrupt check
 * @returns: The interrupt type or null
 */
export function getInterruptionType(result: InterruptionCheckResult): InterruptionType {
  if (result.type === "paused") {
    return "PAUSE";
  } else if (result.type === "stopped") {
    return "STOP";
  }
  return null;
}

/**
 * Get node ID
 * @param result: The result of the interruption check
 * @returns: The node ID or undefined
 */
export function getNodeId(result: InterruptionCheckResult): string | undefined {
  if (result.type === "paused" || result.type === "stopped") {
    return result.nodeId;
  }
  return undefined;
}

/**
 * Get execution ID
 * @param result: Interrupt check result
 * @returns: Execution ID or undefined
 */
export function getExecutionId(result: InterruptionCheckResult): string | undefined {
  if (result.type === "paused" || result.type === "stopped") {
    return result.threadId; // Use threadId which now stores executionId
  }
  return undefined;
}

/**
 * Get a friendly description of the interrupt
 * @param result: Interrupt check result
 * @returns: Interrupt description string
 */
export function getInterruptionDescription(result: InterruptionCheckResult): string {
  switch (result.type) {
    case "continue":
      return "Execution continuing";
    case "paused":
      return `Workflow execution paused at node: ${result.nodeId}`;
    case "stopped":
      return `Workflow execution stopped at node: ${result.nodeId}`;
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
    // If an interrupt error is thrown inside the function, it should be converted into a return value.
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
 * @param iterable: An asynchronously iterable object
 * @param signal: An AbortSignal
 * @returns: The wrapped asynchronous iterator
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
    // Make sure to clean up the iterator.
    if (iterator.return) {
      await iterator.return();
    }
  }
}
