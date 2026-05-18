/**
 * Execution-Specific Interruption Utilities (Generic Layer)
 *
 * Design Principles:
 * - Provides domain-agnostic interruption checking and continuation logic
 * - Extracts basic interruption type from abort reasons
 * - Workflow and Agent modules should use their own specialized utilities
 *   for domain-specific context (nodeId for workflow, iteration for agent)
 */

import type { InterruptionType } from "../../types/interruption-types.js";
import {
  checkInterruption as baseCheckInterruption,
} from "./abort-signal-utils.js";

/**
 * Basic interruption check result (domain-agnostic)
 */
export type ExecutionInterruptionCheckResult =
  | { type: "continue" }
  | { type: "paused"; executionId?: string }
  | { type: "stopped"; executionId?: string }
  | { type: "aborted"; reason?: unknown };

/**
 * Check interruption and extract basic type
 * @param signal AbortSignal
 * @returns The result of the interruption check
 */
export function checkExecutionInterruption(signal?: AbortSignal): ExecutionInterruptionCheckResult {
  const baseResult = baseCheckInterruption(signal);

  // If not aborted or continue, return as-is
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
        return {
          type: "paused",
          executionId: executionId,
        };
      } else if (type === "STOP") {
        return {
          type: "stopped",
          executionId: executionId,
        };
      }
    }
  }

  // Return as generic aborted if no interruption context found
  return baseResult;
}

/**
 * Determine whether to continue execution
 * @param result The result of the interruption check
 * @returns Whether to continue
 */
export function shouldContinueExecution(result: ExecutionInterruptionCheckResult): boolean {
  return result.type === "continue";
}

/**
 * Get the execution interrupt type
 * @param result The result of the interrupt check
 * @returns The interrupt type (PAUSE/STOP/null)
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
 * Get a user-friendly description for execution interruptions (generic)
 * @param result Interrupt check result
 * @returns Interrupt description string
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


