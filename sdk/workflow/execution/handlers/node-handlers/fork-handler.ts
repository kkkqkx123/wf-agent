/**
 * Fork Node Handling Function
 * The Fork node serves as a placeholder; the actual Fork operation is handled by the WorkflowExecutor invoking the WorkflowCoordinator.
 */

import type { Node } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";

/**
 * Check whether the node can be executed.
 */
function canExecute(workflowExecutionEntity: WorkflowExecutionEntity): boolean {
  if (workflowExecutionEntity.getStatus() !== "RUNNING") {
    return false;
  }
  return true;
}

/**
 * Fork Node Processing Function
 * The Fork node serves as a placeholder; the actual Fork operation is handled by the WorkflowExecutor, which in turn calls the WorkflowCoordinator.
 * @param workflowExecutionEntity: WorkflowExecutionEntity instance
 * @param node: Node definition
 * @returns: Execution result
 */
export async function forkHandler(
  workflowExecutionEntity: WorkflowExecutionEntity,
  node: Node,
): Promise<unknown> {
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

  // The Fork node serves as a placeholder; the actual Fork operation is handled by the WorkflowExecutor, which in turn calls the WorkflowOperationCoordinator.
  // Configuration parameters are read from node.config and no need to return configuration information
  return {};
}
