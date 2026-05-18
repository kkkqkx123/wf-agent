/**
 * Agent-Specific Interruption Utilities
 *
 * Design Principles:
 * - Extends generic interruption utilities with agent-specific context (iteration)
 * - Provides agent-aware helper functions for PAUSE/STOP handling
 * - Used by agent loop coordinators and executors
 */

import type { InterruptionType } from "../../../core/types/interruption-types.js";
import {
  checkExecutionInterruption as baseCheckInterruption,
} from "../../../core/utils/interruption/index.js";

/**
 * Agent interruption check result with iteration context
 */
export type AgentInterruptionCheckResult =
  | { type: "continue" }
  | { type: "paused"; iteration: number; executionId?: string }
  | { type: "stopped"; iteration: number; executionId?: string }
  | { type: "aborted"; reason?: unknown };

/**
 * Check interruption with agent context extraction (iteration)
 * @param signal AbortSignal
 * @returns The result of the interruption check with agent context
 */
export function checkAgentInterruption(
  signal?: AbortSignal,
  currentIteration?: number,
): AgentInterruptionCheckResult {
  const baseResult = baseCheckInterruption(signal);

  // If not aborted or continue, return as-is
  if (baseResult.type === "continue") {
    return baseResult;
  }

  // For aborted state, try to extract agent-specific information
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
          iteration: currentIteration ?? 0,
        };
      } else if (type === "STOP") {
        return {
          type: "stopped",
          executionId: executionId,
          iteration: currentIteration ?? 0,
        };
      }
    }
  }

  // Return as generic aborted if no agent context found
  return baseResult as AgentInterruptionCheckResult;
}

/**
 * Get the agent interrupt type
 * @param result The result of the interrupt check
 * @returns The interrupt type (PAUSE/STOP/null)
 */
export function getAgentInterruptionType(result: AgentInterruptionCheckResult): InterruptionType {
  if (result.type === "paused") {
    return "PAUSE";
  } else if (result.type === "stopped") {
    return "STOP";
  }
  return null;
}

/**
 * Get a user-friendly description for agent interruptions
 * @param result Interrupt check result
 * @returns Interrupt description string
 */
export function getAgentInterruptionDescription(result: AgentInterruptionCheckResult): string {
  switch (result.type) {
    case "continue":
      return "Agent loop continuing";
    case "paused":
      return `Agent loop paused at iteration ${result.iteration}`;
    case "stopped":
      return `Agent loop stopped at iteration ${result.iteration}`;
    case "aborted":
      return result.reason ? String(result.reason) : "Agent loop operation aborted";
    default:
      return "Unknown agent interruption state";
  }
}

/**
 * Create an abort reason object for agent interruption
 * Used to properly structure abort signals with agent context
 *
 * @param type Interruption type (PAUSE or STOP)
 * @param executionId Agent loop ID (conversation ID)
 * @param iteration Current iteration number
 * @returns Error object with agent interruption context
 *
 * @example
 * ```typescript
 * const reason = createAgentInterruptionAbortReason("PAUSE", agentLoopId, currentIteration);
 * abortController.abort(reason);
 * ```
 */
export function createAgentInterruptionAbortReason(
  type: "PAUSE" | "STOP",
  executionId: string,
  iteration: number,
): Error & { interruptionType: "PAUSE" | "STOP"; executionId: string; iteration: number } {
  const error = new Error(type === "PAUSE" ? "Agent loop paused" : "Agent loop stopped") as Error & {
    interruptionType: "PAUSE" | "STOP";
    executionId: string;
    iteration: number;
  };
  error.interruptionType = type;
  error.executionId = executionId;
  error.iteration = iteration;
  return error;
}
