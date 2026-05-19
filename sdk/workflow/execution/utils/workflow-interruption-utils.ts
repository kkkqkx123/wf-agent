/**
 * Workflow-Specific Interruption Utilities
 *
 * Extends generic interruption utilities with workflow-specific context (nodeId).
 */

import type { InterruptionType } from "../../../core/types/interruption-types.js";
import {
  checkExecutionInterruption as baseCheckInterruption,
} from "../../../core/utils/interruption/index.js";
import { WorkflowExecutionInterruptedException } from "../types/workflow-interruption-types.js";

/**
 * Workflow interruption check result with node context
 */
export type WorkflowInterruptionCheckResult =
  | { type: "continue" }
  | { type: "paused"; nodeId: string; executionId?: string }
  | { type: "stopped"; nodeId: string; executionId?: string }
  | { type: "aborted"; reason?: unknown };

/**
 * Check interruption with workflow context extraction (nodeId)
 */
export function checkWorkflowInterruption(signal?: AbortSignal): WorkflowInterruptionCheckResult {
  const baseResult = baseCheckInterruption(signal);

  // If not aborted, return as-is
  if (baseResult.type === "continue") {
    return baseResult;
  }

  // For aborted state, try to extract workflow-specific information
  if (baseResult.type === "aborted") {
    const reason = baseResult.reason;
    if (reason && typeof reason === "object" && "interruptionType" in reason) {
      const interruption = reason as Record<string, unknown>;
      const type = interruption["interruptionType"] as InterruptionType;
      const executionId = interruption["executionId"] as string | undefined;
      const nodeId = interruption["nodeId"] as string | undefined;

      if (type === "PAUSE") {
        return {
          type: "paused",
          executionId: executionId,
          nodeId: nodeId || "unknown",
        };
      } else if (type === "STOP") {
        return {
          type: "stopped",
          executionId: executionId,
          nodeId: nodeId || "unknown",
        };
      }
    }
  }

  // Return as generic aborted if no workflow context found
  return baseResult as WorkflowInterruptionCheckResult;
}

/**
 * Get the workflow interrupt type
 */
export function getWorkflowInterruptionType(result: WorkflowInterruptionCheckResult): InterruptionType {
  if (result.type === "paused") {
    return "PAUSE";
  } else if (result.type === "stopped") {
    return "STOP";
  }
  return null;
}

/**
 * Get a user-friendly description for workflow interruptions
 */
export function getWorkflowInterruptionDescription(result: WorkflowInterruptionCheckResult): string {
  switch (result.type) {
    case "continue":
      return "Workflow execution continuing";
    case "paused":
      return `Workflow execution paused at node: ${result.nodeId}`;
    case "stopped":
      return `Workflow execution stopped at node: ${result.nodeId}`;
    case "aborted":
      return result.reason ? String(result.reason) : "Workflow execution operation aborted";
    default:
      return "Unknown workflow interruption state";
  }
}

/**
 * Convert generic interruption result to workflow-specific result
 */
export function toWorkflowInterruptionResult(
  result: import("../../../core/utils/interruption/index.js").ExecutionInterruptionCheckResult,
  nodeId: string,
): WorkflowInterruptionCheckResult {
  if (result.type === "paused" || result.type === "stopped") {
    return {
      ...result,
      nodeId,
    };
  }
  return result as WorkflowInterruptionCheckResult;
}

/**
 * Create an abort reason object for workflow interruption with node context
 */
export function createWorkflowInterruptionAbortReason(
  type: "PAUSE" | "STOP",
  executionId: string,
  nodeId: string,
): WorkflowExecutionInterruptedException {
  return new WorkflowExecutionInterruptedException(
    type === "PAUSE" ? "Workflow execution paused" : "Workflow execution stopped",
    type,
    executionId,
    nodeId,
  );
}

