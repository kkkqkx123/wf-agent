/**
 * EmbedStart Node Processing Function
 * Internal pass-through node generated during EMBED_GRAPH expansion.
 * Acts as an entry boundary marker — immediately passes execution through.
 */

import type { RuntimeNode } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";
import { now } from "@wf-agent/common-utils";

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
 * EmbedStart node processing function
 * Pass-through handler: records execution and returns immediately.
 * @param workflowExecutionEntity: WorkflowExecutionEntity instance
 * @param node: RuntimeNode definition
 * @returns: Execution result
 */
export async function embedStartHandler(
  workflowExecutionEntity: WorkflowExecutionEntity,
  node: RuntimeNode,
): Promise<unknown> {
  if (!canExecute(workflowExecutionEntity, node)) {
    return {
      nodeId: node.id,
      nodeType: node.type,
      status: "SKIPPED",
      step: workflowExecutionEntity.getNodeResults().length + 1,
      executionTime: 0,
    };
  }

  workflowExecutionEntity.setCurrentNodeId(node.id);

  workflowExecutionEntity.addNodeResult({
    step: workflowExecutionEntity.getNodeResults().length + 1,
    nodeId: node.id,
    nodeType: node.type,
    status: "COMPLETED",
    timestamp: now(),
  });

  return {
    nodeId: node.id,
    nodeType: node.type,
    status: "COMPLETED",
    message: "Embedded graph boundary (start) - pass through",
  };
}
