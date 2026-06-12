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
 * Unified interruption check result
 *
 * Contains all context fields needed by agent (iteration) and workflow (nodeId).
 * Use this single type across Core, Agent, and Workflow layers to
 * eliminate type duplication and enable seamless interruption handling.
 */
export type ExecutionInterruptionCheckResult =
  | { type: "continue" }
  | { type: "paused"; executionId?: string; iteration?: number; nodeId?: string }
  | { type: "stopped"; executionId?: string; iteration?: number; nodeId?: string }
  | { type: "aborted"; reason?: unknown };

/**
 * Check interruption and extract all context fields
 *
 * Parses the abort reason to extract interruption type, executionId, iteration, and nodeId.
 * This is the single source of truth; agent/workflow-specific wrappers delegate to this.
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
      const iteration = interruption["iteration"] as number | undefined;
      const nodeId = interruption["nodeId"] as string | undefined;

      if (type === "PAUSE") {
        return { type: "paused", executionId, iteration, nodeId };
      } else if (type === "STOP") {
        return { type: "stopped", executionId, iteration, nodeId };
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
 *
 * Produces a context-aware description including iteration and nodeId when available.
 * Replaces agent-specific getAgentInterruptionDescription() and workflow-specific
 * getWorkflowInterruptionDescription() with a single unified function.
 */
export function getExecutionInterruptionDescription(result: ExecutionInterruptionCheckResult): string {
  switch (result.type) {
    case "continue":
      return "Execution continuing";
    case "paused": {
      let msg = "Execution paused";
      if (result.iteration !== undefined) msg += ` at iteration ${result.iteration}`;
      if (result.nodeId) msg += ` at node: ${result.nodeId}`;
      if (result.executionId) msg += ` (ID: ${result.executionId})`;
      return msg;
    }
    case "stopped": {
      let msg = "Execution stopped";
      if (result.iteration !== undefined) msg += ` at iteration ${result.iteration}`;
      if (result.nodeId) msg += ` at node: ${result.nodeId}`;
      if (result.executionId) msg += ` (ID: ${result.executionId})`;
      return msg;
    }
    case "aborted":
      return result.reason ? String(result.reason) : "Execution operation aborted";
    default:
      return "Unknown execution interruption state";
  }
}


