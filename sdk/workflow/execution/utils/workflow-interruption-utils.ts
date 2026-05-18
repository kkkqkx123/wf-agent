/**
 * Workflow-Specific Interruption Utilities
 *
 * Design Principles:
 * - Extends generic interruption utilities with workflow-specific context (nodeId)
 * - Provides workflow-aware helper functions for PAUSE/STOP handling
 * - Used by workflow execution coordinators and node handlers
 */

import type { InterruptionType } from "../../../core/types/interruption-types.js";
import {
  checkExecutionInterruption as baseCheckInterruption,
} from "../../../core/utils/interruption/index.js";

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
 * @param signal AbortSignal
 * @returns The result of the interruption check with workflow context
 */
export function checkWorkflowInterruption(signal?: AbortSignal): WorkflowInterruptionCheckResult {
  const baseResult = baseCheckInterruption(signal);

  // If not aborted or continue, return as-is
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
 * @param result The result of the interrupt check
 * @returns The interrupt type (PAUSE/STOP/null)
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
 * @param result Interrupt check result
 * @returns Interrupt description string
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
 * @param result Generic interruption check result
 * @param nodeId Node ID for workflow context
 * @returns Workflow-specific interruption check result
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
 * Create an abort reason object for workflow interruption
 * Used to properly structure abort signals with workflow context
 *
 * @param type Interruption type (PAUSE or STOP)
 * @param executionId Workflow execution ID
 * @param nodeId Node ID where interruption occurred
 * @returns Error object with workflow interruption context
 *
 * @example
 * ```typescript
 * const reason = createWorkflowInterruptionAbortReason("PAUSE", executionId, nodeId);
 * abortController.abort(reason);
 * ```
 */
export function createWorkflowInterruptionAbortReason(
  type: "PAUSE" | "STOP",
  executionId: string,
  nodeId: string,
): Error & { interruptionType: "PAUSE" | "STOP"; executionId: string; nodeId: string } {
  const error = new Error(type === "PAUSE" ? "Workflow execution paused" : "Workflow execution stopped") as Error & {
    interruptionType: "PAUSE" | "STOP";
    executionId: string;
    nodeId: string;
  };
  error.interruptionType = type;
  error.executionId = executionId;
  error.nodeId = nodeId;
  return error;
}
