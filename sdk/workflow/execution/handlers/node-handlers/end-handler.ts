/**
 * End Node Processing Function
 * Responsible for executing the END node, marking the end of the workflow, and collecting the execution results.
 */

import type { Node } from "@wf-agent/types";
import type { ThreadEntity } from "../../../entities/thread-entity.js";
import { now, diffTimestamp } from "@wf-agent/common-utils";

/**
 * Check whether the node can be executed.
 */
function canExecute(threadEntity: ThreadEntity, node: Node): boolean {
  if (threadEntity.getStatus() !== "RUNNING") {
    return false;
  }

  if (threadEntity.getNodeResults().some(result => result.nodeId === node.id)) {
    return false;
  }

  return true;
}

/**
 * End Node Processing Function
 * @param threadEntity: ThreadEntity instance
 * @param node: Node definition
 * @returns: Execution result
 */
export async function endHandler(
  threadEntity: ThreadEntity,
  node: Node,
  _context?: unknown,
): Promise<unknown> {
  // Check if it is possible to execute.
  if (!canExecute(threadEntity, node)) {
    return {
      nodeId: node.id,
      nodeType: node.type,
      status: "SKIPPED",
      step: threadEntity.getNodeResults().length + 1,
      executionTime: 0,
    };
  }

  // Collect Thread output
  // Use thread.output as the final output of the workflow (explicitly set by a node or the END node).
  const output = threadEntity.getOutput() || {};

  // Set the Thread output (without modifying the status; the status is managed by the ThreadLifecycleCoordinator).
  threadEntity.setOutput(output);

  // Record the execution history.
  threadEntity.addNodeResult({
    step: threadEntity.getNodeResults().length + 1,
    nodeId: node.id,
    nodeType: node.type,
    status: "COMPLETED",
    timestamp: now(),
  });

  // Calculate execution time using ThreadEntity's time methods
  const startTime = threadEntity.getStartTime();
  const executionTime = startTime ? diffTimestamp(startTime, now()) : 0;

  // Return the execution results
  return {
    message: "Workflow completed",
    output,
    executionTime,
  };
}
