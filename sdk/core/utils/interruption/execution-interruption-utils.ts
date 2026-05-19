/**
 * Execution-Specific Interruption Utilities
 *
 * Provides domain-agnostic interruption checking and continuation logic.
 */

import type { InterruptionType } from "../../types/interruption-types.js";
import {
  checkInterruption as baseCheckInterruption,
} from "./abort-signal-utils.js";

/**
 * Basic interruption check result
 */
export type ExecutionInterruptionCheckResult =
  | { type: "continue" }
  | { type: "paused"; executionId?: string }
  | { type: "stopped"; executionId?: string }
  | { type: "aborted"; reason?: unknown };

/**
 * Check interruption and extract basic type
 */
export function checkExecutionInterruption(signal?: AbortSignal): ExecutionInterruptionCheckResult {
  const baseResult = baseCheckInterruption(signal);

  // If not aborted, return as-is
  if (baseResult.type === "continue") {
    return baseResult;
  }

  // For aborted state, try to extract interruption type
  if (baseResult.type === "aborted") {
    const reason = baseResult.reason;
    if (reason && typeof reason === "object" && "interruptionType" in reason) {
      const interruption = reason as Record<string, unknown>;
      const type = interruption["interruptionType"] as InterruptionType;
      const executionId = interruption["executionId"] as string | undefined;

      if (type === "PAUSE") {
        return { type: "paused", executionId };
      } else if (type === "STOP") {
        return { type: "stopped", executionId };
      }
    }
  }

  return baseResult;
}

/**
 * Determine whether to continue execution
 */
export function shouldContinueExecution(result: ExecutionInterruptionCheckResult): boolean {
  return result.type === "continue";
}

/**
 * Get the execution interrupt type
 */
export function getExecutionInterruptionType(result: ExecutionInterruptionCheckResult): InterruptionType {
  if (result.type === "paused") {
    return "PAUSE";
  } else if (result.type === "stopped") {
    return "STOP";
  }
  return null;
}

/**
 * Get a user-friendly description for execution interruptions
 */
export function getExecutionInterruptionDescription(result: ExecutionInterruptionCheckResult): string {
  switch (result.type) {
    case "continue":
      return "Execution continuing";
    case "paused":
      return `Execution paused${result.executionId ? ` (ID: ${result.executionId})` : ""}`;
    case "stopped":
      return `Execution stopped${result.executionId ? ` (ID: ${result.executionId})` : ""}`;
    case "aborted":
      return result.reason ? String(result.reason) : "Execution operation aborted";
    default:
      return "Unknown execution interruption state";
  }
}


