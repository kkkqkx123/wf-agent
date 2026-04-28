/**
 * Start Node Processing Function
 * Responsible for executing the START node, marking the beginning of the workflow, and initializing the Thread status.
 */

import type { Node } from "@wf-agent/types";
import type { ThreadEntity } from "../../../entities/thread-entity.js";
import { now } from "@wf-agent/common-utils";

/**
 * Check whether the node can be executed.
 */
function canExecute(threadEntity: ThreadEntity, node: Node): boolean {
  // The START node can be executed in either the CREATED or RUNNING state (if it has not been executed before).
  const status = threadEntity.getStatus();
  if (status !== "CREATED" && status !== "RUNNING") {
    return false;
  }

  if (threadEntity.getNodeResults().some(result => result.nodeId === node.id)) {
    return false;
  }

  return true;
}

/**
 * Start node processing function
 * @param threadEntity: ThreadEntity instance
 * @param node: Node definition
 * @returns: Execution result
 */
export async function startHandler(
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

  // Initialize Thread state via ThreadEntity
  threadEntity.setStatus("RUNNING");
  threadEntity.setCurrentNodeId(node.id);
  threadEntity.state.start();

  // Initializing variables and results for the Thread
  const thread = workflowExecutionEntity.getThread();
  if (!thread.variables) {
    thread.variables = [];
  }
  if (!thread.errors) {
    thread.errors = [];
  }

  // Initialize Thread input
  if (!thread.input) {
    thread.input = {};
  }

  // Record execution history
  threadEntity.addNodeResult({
    step: threadEntity.getNodeResults().length + 1,
    nodeId: node.id,
    nodeType: node.type,
    status: "COMPLETED",
    timestamp: now(),
  });

  // Return the execution result
  return {
    message: "Workflow started",
    input: threadEntity.getInput(),
  };
}
