/**
 * Agent-Specific Interruption Utilities
 *
 * Extends generic interruption utilities with agent-specific context (iteration).
 */

import type { InterruptionType } from "../../../core/types/interruption-types.js";
import {
  checkExecutionInterruption as baseCheckInterruption,
} from "../../../core/utils/interruption/index.js";
import { AgentExecutionInterruptedException } from "../types/agent-interruption-types.js";

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
        return { type: "paused", executionId, iteration: currentIteration ?? 0 };
      } else if (type === "STOP") {
        return { type: "stopped", executionId, iteration: currentIteration ?? 0 };
      }
    }
  }

  return baseResult as AgentInterruptionCheckResult;
}

/**
 * Get the agent interrupt type
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
 * Create an abort reason object for agent interruption with context
 */
export function createAgentInterruptionAbortReason(
  type: "PAUSE" | "STOP",
  executionId: string,
  iteration: number,
): AgentExecutionInterruptedException {
  return new AgentExecutionInterruptedException(
    type === "PAUSE" ? "Agent loop paused" : "Agent loop stopped",
    type,
    executionId,
    iteration,
  );
}
