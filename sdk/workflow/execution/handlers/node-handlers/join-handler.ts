/**
 * Join node processing function
 * The Join node serves as a placeholder; the actual Join operation is handled by the WorkflowExecutor invoking the WorkflowCoordinator.
 */

import type { Node } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";

/**
 * Check if the node can be executed.
 */
function canExecute(workflowExecutionEntity: WorkflowExecutionEntity): boolean {
  if (workflowExecutionEntity.getStatus() !== "RUNNING") {
    return false;
  }
  return true;
}

/**
 * Join Node Processing Function
 * The Join node serves as a placeholder; the actual Join operation is handled by the WorkflowExecutor calling the WorkflowCoordinator.
 *
 * Explanation:
 * - The sub-workflow execution ID is dynamically obtained from the execution context, not from the node configuration.
 * - The default timeout is 0 (no timeout), which can be overridden in the configuration.
 *
 * @param workflowExecutionEntity: WorkflowExecutionEntity instance
 * @param node: Node definition
 * @returns: Execution result
 */
export async function joinHandler(workflowExecutionEntity: WorkflowExecutionEntity, node: Node): Promise<unknown> {
  // Check if it is possible to execute.
  if (!canExecute(workflowExecutionEntity)) {
    return {
      nodeId: node.id,
      nodeType: node.type,
      status: "SKIPPED",
      step: workflowExecutionEntity.getNodeResults().length + 1,
      executionTime: 0,
    };
  }

  // The Join node serves as a placeholder; the actual Join operation is handled by the WorkflowExecutor calling the WorkflowOperationCoordinator.
  // Configuration parameters are read from node.config and no need to return configuration information
  return {};
}
