/**
 * Execution-Specific Interruption Utilities
 * Extends generic AbortSignal utilities with execution context (workflow/agent)
 *
 * Design Principles:
 * - Build on top of domain-agnostic interruption utilities from common-utils
 * - Extract execution-specific information (executionId/conversationId, nodeId) from abort reasons
 * - Support both workflow execution and agent loop interruption patterns
 * - Provide execution-aware helper functions for PAUSE/STOP handling
 * - Used by shared core executors and both workflow/agent modules
 */

import type { InterruptionType } from "@wf-agent/types";
import {
  checkInterruption as baseCheckInterruption,
} from "@wf-agent/common-utils";

/**
 * Extended interruption check result with execution context
 * Supports both workflow execution and agent loop interruption patterns
 */
export type ExecutionInterruptionCheckResult =
  | { type: "continue" }
  | { type: "paused"; nodeId: string; executionId?: string }
  | { type: "stopped"; nodeId: string; executionId?: string }
  | { type: "aborted"; reason?: unknown };

/**
 * Check interruption with workflow context extraction
 * @param signal AbortSignal
 * @returns The result of the interruption check with workflow context
 */
export function checkWorkflowInterruption(signal?: AbortSignal): ExecutionInterruptionCheckResult {
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
  return baseResult;
}

/**
 * Determine whether to continue workflow execution
 * @param result The result of the interruption check
 * @returns Whether to continue
 */
export function shouldContinue(result: ExecutionInterruptionCheckResult): boolean {
  return result.type === "continue";
}

/**
 * Get the workflow interrupt type
 * @param result The result of the interrupt check
 * @returns The interrupt type (PAUSE/STOP/null)
 */
export function getWorkflowInterruptionType(result: ExecutionInterruptionCheckResult): InterruptionType {
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
export function getWorkflowInterruptionDescription(result: ExecutionInterruptionCheckResult): string {
  switch (result.type) {
    case "continue":
      return "Workflow execution continuing";
    case "paused":
      return `Workflow execution paused at node: ${result.nodeId}`;
    case "stopped":
      return `Workflow execution stopped at node: ${result.nodeId}`;
    case "aborted":
      return result.reason ? String(result.reason) : "Workflow operation aborted";
    default:
      return "Unknown workflow interruption state";
  }
}

/**
 * Create an abort reason object for execution interruption
 * Used to properly structure abort signals with interruption context
 *
 * @param type Interruption type (PAUSE or STOP)
 * @param executionId Execution/conversation ID
 * @param nodeId Node ID (optional, for workflow) or agent loop ID
 * @returns Error object with interruption context
 *
 * @example
 * ```typescript
 * // In workflow execution
 * const reason = createInterruptionAbortReason("PAUSE", executionId, nodeId);
 * abortController.abort(reason);
 *
 * // In agent loop
 * const reason = createInterruptionAbortReason("STOP", conversationId, agentLoopId);
 * abortController.abort(reason);
 * ```
 */
export function createInterruptionAbortReason(
  type: "PAUSE" | "STOP",
  executionId: string,
  nodeId?: string,
): Error & { interruptionType: "PAUSE" | "STOP"; executionId: string; nodeId?: string } {
  const error = new Error(type === "PAUSE" ? "Execution paused" : "Execution stopped") as Error & {
    interruptionType: "PAUSE" | "STOP";
    executionId: string;
    nodeId?: string;
  };
  error.interruptionType = type;
  error.executionId = executionId;
  if (nodeId) {
    error.nodeId = nodeId;
  }
  return error;
}
