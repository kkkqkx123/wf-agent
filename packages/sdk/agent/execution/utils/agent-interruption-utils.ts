/**
 * Agent-Specific Interruption Utilities
 *
 * Thin wrappers over Core interruption utilities that add agent-specific
 * iteration context. Uses Core's unified ExecutionInterruptionCheckResult
 * type to eliminate type duplication between Agent and Workflow layers.
 *
 * Design Principle:
 * - All core logic delegates to sdk/shared/utils/interruption/
 * - Agent adds only the iteration context extraction
 * - Type aliases kept for backward compatibility
 */

import type { InterruptionType } from "../../../shared/types/interruption-types.js";
import {
  checkExecutionInterruption as baseCheckInterruption,
  getExecutionInterruptionDescription as baseGetDescription,
  type ExecutionInterruptionCheckResult,
} from "../../../shared/utils/interruption/index.js";
import { AgentExecutionInterruptedException } from "../types/agent-interruption-types.js";

/**
 * Agent interruption check result (type alias for backward compatibility)
 *
 * Uses Core's unified type which now includes optional iteration field.
 */
export type AgentInterruptionCheckResult = ExecutionInterruptionCheckResult;

/**
 * Check interruption with agent context extraction (iteration)
 *
 * Delegates to Core's checkExecutionInterruption which parses the abort
 * reason for all context fields including iteration.
 */
export function checkAgentInterruption(
  signal?: AbortSignal,
  currentIteration?: number,
): AgentInterruptionCheckResult {
  const result = baseCheckInterruption(signal);

  // If Core extracted iteration from abort reason, prefer that
  if (
    (result.type === "paused" || result.type === "stopped") &&
    currentIteration !== undefined &&
    result.iteration === undefined
  ) {
    return { ...result, iteration: currentIteration };
  }

  return result;
}

/**
 * Get the agent interrupt type
 */
export function getAgentInterruptionType(
  result: AgentInterruptionCheckResult,
): InterruptionType | null {
  if (result.type === "paused") {
    return "PAUSE";
  } else if (result.type === "stopped") {
    return "STOP";
  }
  return null;
}

/**
 * Get a user-friendly description for agent interruptions
 *
 * Delegates to Core's unified getExecutionInterruptionDescription
 * which handles iteration context automatically.
 */
export function getAgentInterruptionDescription(result: AgentInterruptionCheckResult): string {
  return baseGetDescription(result);
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
