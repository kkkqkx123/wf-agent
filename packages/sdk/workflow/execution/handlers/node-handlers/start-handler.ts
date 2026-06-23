/**
 * Start Node Processing Function
 * Responsible for executing the START node, marking the beginning of the workflow, and initializing the WorkflowExecution status.
 */

import type { RuntimeNode } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";
import { now } from "@wf-agent/common-utils";

/**
 * Check whether the node can be executed (idempotency check — status check is handled centrally).
 */
function canExecute(workflowExecutionEntity: WorkflowExecutionEntity, node: RuntimeNode): boolean {
  if (workflowExecutionEntity.getNodeResults().some(result => result.nodeId === node.id)) {
    return false;
  }

  return true;
}

/**
 * Start node processing function
 * @param workflowExecutionEntity: WorkflowExecutionEntity instance
 * @param node: RuntimeNode definition
 * @returns: Execution result
 */
export async function startHandler(
  workflowExecutionEntity: WorkflowExecutionEntity,
  node: RuntimeNode,
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

  // Initialize WorkflowExecution state via WorkflowExecutionEntity
  workflowExecutionEntity.state.start();
  workflowExecutionEntity.setCurrentNodeId(node.id);

  // Initializing variables and results for the WorkflowExecution
  const workflowExecution = workflowExecutionEntity.getWorkflowExecutionData();
  if (!workflowExecution.variables) {
    workflowExecution.variables = [];
  }
  if (!workflowExecution.errors) {
    workflowExecution.errors = [];
  }

  // Initialize WorkflowExecution input
  if (!workflowExecution.input) {
    workflowExecution.input = {};
  }

  // Record execution history
  workflowExecutionEntity.addNodeResult({
    step: workflowExecutionEntity.getNodeResults().length + 1,
    nodeId: node.id,
    nodeType: node.type,
    status: "COMPLETED",
    timestamp: now(),
  });

  // Return the execution result
  return {
    message: "Workflow started",
    input: workflowExecutionEntity.getInput(),
  };
}
