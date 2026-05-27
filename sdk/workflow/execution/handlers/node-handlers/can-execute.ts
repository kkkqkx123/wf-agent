/**
 * Execution Guard Utilities
 *
 * Provides a shared `getSkippedResult` function that all node handlers
 * can use to check whether the workflow execution is still in RUNNING
 * status before proceeding with execution.
 *
 * Usage:
 *   import { getSkippedResult } from "./can-execute.js";
 *
 *   const skipped = getSkippedResult(workflowExecutionEntity, node);
 *   if (skipped) return skipped;
 *
 * This replaces the inline `if (getStatus() !== "RUNNING") { return {...} }`
 * pattern with a single source of truth, ensuring consistent SKIPPED
 * result shape across all handlers.
 */

import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";
import type { RuntimeNode } from "@wf-agent/types";

/**
 * Standardized "SKIPPED" result shape returned when the workflow execution
 * is not in RUNNING status.
 */
export interface SkippedResult {
  nodeId: string;
  nodeType: string;
  status: "SKIPPED";
  step: number;
  executionTime: number;
}

/**
 * Check whether the node can be executed based on workflow execution status.
 *
 * Returns a `SkippedResult` if the status is not "RUNNING", or `null` if
 * execution should proceed. Handlers that have additional validation
 * (e.g., idempotency checks) can combine this with their own logic.
 */
export function getSkippedResult(
  workflowExecutionEntity: WorkflowExecutionEntity,
  node: RuntimeNode,
): SkippedResult | null {
  if (workflowExecutionEntity.getStatus() !== "RUNNING") {
    return {
      nodeId: node.id,
      nodeType: node.type,
      status: "SKIPPED",
      step: workflowExecutionEntity.getNodeResults().length + 1,
      executionTime: 0,
    };
  }
  return null;
}
