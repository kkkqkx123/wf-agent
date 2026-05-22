/**
 * End Node Processing Function
 * Responsible for executing the END node, marking the end of the workflow, and collecting the execution results.
 */

import type { RuntimeNode } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";
import { now, diffTimestamp } from "@wf-agent/common-utils";

/**
 * Check whether the node can be executed.
 */
function canExecute(workflowExecutionEntity: WorkflowExecutionEntity, node: RuntimeNode): boolean {
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
 * @param node: RuntimeNode definition
 * @returns: Execution result
 */
export async function endHandler(
  workflowExecutionEntity: WorkflowExecutionEntity,
  node: RuntimeNode,
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

  // Process dataOutputs from END node config: map internal variables to output keys
  const endConfig = node.config as { dataOutputs?: Array<{ internalName: string; outputKey: string; description?: string }> } | undefined;
  if (endConfig?.dataOutputs && endConfig.dataOutputs.length > 0) {
    for (const dataOutput of endConfig.dataOutputs) {
      const value = workflowExecutionEntity.variableStateManager.getVariable(dataOutput.internalName);
      if (value !== undefined) {
        (output as Record<string, unknown>)[dataOutput.outputKey] = value;
      }
    }
  }

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
