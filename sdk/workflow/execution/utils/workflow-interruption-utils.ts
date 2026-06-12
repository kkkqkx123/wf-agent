/**
 * Workflow-Specific Interruption Utilities
 *
 * Thin wrappers over Core interruption utilities that add workflow-specific
 * nodeId context. Uses Core's unified ExecutionInterruptionCheckResult
 * type to eliminate type duplication between Agent and Workflow layers.
 *
 * Design Principle:
 * - All core logic delegates to sdk/core/utils/interruption/
 * - Workflow adds only the nodeId context extraction
 * - Type aliases kept for backward compatibility
 */

import type { InterruptionType } from "../../../core/types/interruption-types.js";
import {
  checkExecutionInterruption as baseCheckInterruption,
  getExecutionInterruptionDescription as baseGetDescription,
  type ExecutionInterruptionCheckResult,
} from "../../../core/utils/interruption/index.js";
import { WorkflowExecutionInterruptedException } from "../types/workflow-interruption-types.js";

/**
 * Workflow interruption check result (type alias for backward compatibility)
 *
 * Uses Core's unified type which now includes optional nodeId field.
 */
export type WorkflowInterruptionCheckResult = ExecutionInterruptionCheckResult;

/**
 * Check interruption with workflow context extraction (nodeId)
 *
 * Delegates to Core's checkExecutionInterruption which parses the abort
 * reason for all context fields including nodeId.
 * Ensures nodeId defaults to "unknown" for backward compatibility.
 */
export function checkWorkflowInterruption(signal?: AbortSignal): WorkflowInterruptionCheckResult {
  const result = baseCheckInterruption(signal);

  // Ensure nodeId is always present for paused/stopped results (backward compat)
  if (result.type === "paused" || result.type === "stopped") {
    return { ...result, nodeId: result.nodeId || "unknown" };
  }

  return result;
}

/**
 * Get the workflow interrupt type
 */
export function getWorkflowInterruptionType(
  result: WorkflowInterruptionCheckResult,
): InterruptionType | null {
  if (result.type === "paused") {
    return "PAUSE";
  } else if (result.type === "stopped") {
    return "STOP";
  }
  return null;
}

/**
 * Get a user-friendly description for workflow interruptions
 *
 * Delegates to Core's unified getExecutionInterruptionDescription
 * which handles nodeId context automatically.
 */
export function getWorkflowInterruptionDescription(
  result: WorkflowInterruptionCheckResult,
): string {
  return baseGetDescription(result);
}

/**
 * Convert generic interruption result to workflow-specific result
 */
export function toWorkflowInterruptionResult(
  result: ExecutionInterruptionCheckResult,
  nodeId: string,
): WorkflowInterruptionCheckResult {
  if (result.type === "paused" || result.type === "stopped") {
    return {
      ...result,
      nodeId: nodeId || result.nodeId,
    };
  }
  return result;
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
