/**
 * End Node Processing Function
 * Responsible for executing the END node, marking the end of the workflow, and collecting the execution results.
 */

import type { Node } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";
import { now, diffTimestamp } from "@wf-agent/common-utils";

/**
 * Check whether the node can be executed.
 */
function canExecute(workflowExecutionEntity: WorkflowExecutionEntity, node: Node): boolean {
  if (workflowExecutionEntity.getStatus() !== "RUNNING") {
    return false;
  }

  if (workflowExecutionEntity.getNodeResults().some(result => result.nodeId === node.id)) {
    return false;
  }

  return true;
}

/**
 * End Node Processing Function
 * @param workflowExecutionEntity: WorkflowExecutionEntity instance
 * @param node: Node definition
 * @returns: Execution result
 */
export async function endHandler(
  workflowExecutionEntity: WorkflowExecutionEntity,
  node: Node,
  _context?: unknown,
): Promise<unknown> {
  // Check if it is possible to execute.
  if (!canExecute(workflowExecutionEntity, node)) {
    return {
      nodeId: node.id,
      nodeType: node.type,
      status: "SKIPPED",
      step: workflowExecutionEntity.getNodeResults().length + 1,
      executionTime: 0,
    };
  }

  // Collect WorkflowExecution output
  // Use workflowExecution.output as the final output of the workflow (explicitly set by a node or the END node).
  const output = workflowExecutionEntity.getOutput() || {};

  // Set the WorkflowExecution output (without modifying the status; the status is managed by the WorkflowLifecycleCoordinator).
  workflowExecutionEntity.setOutput(output);

  // Record the execution history.
  workflowExecutionEntity.addNodeResult({
    step: workflowExecutionEntity.getNodeResults().length + 1,
    nodeId: node.id,
    nodeType: node.type,
    status: "COMPLETED",
    timestamp: now(),
    executionTime: diffTimestamp(now(), now()),
  });

  // Calculate execution time using WorkflowExecutionEntity's time methods
  const executionTime = workflowExecutionEntity.getExecutionTime();

  // Return the execution result.
  return {
    nodeId: node.id,
    nodeType: node.type,
    status: "COMPLETED",
    output,
    executionTime,
  };
}
