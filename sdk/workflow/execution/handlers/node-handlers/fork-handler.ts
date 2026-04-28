/**
 * Fork Node Handling Function
 * The Fork node serves as a placeholder; the actual Fork operation is handled by the ThreadExecutor invoking the ThreadCoordinator.
 */

import type { Node } from "@wf-agent/types";
import type { ThreadEntity } from "../../../entities/thread-entity.js";

/**
 * Check whether the node can be executed.
 */
function canExecute(threadEntity: ThreadEntity): boolean {
  if (threadEntity.getStatus() !== "RUNNING") {
    return false;
  }
  return true;
}

/**
 * Fork Node Processing Function
 * The Fork node serves as a placeholder; the actual Fork operation is handled by the ThreadExecutor, which in turn calls the ThreadCoordinator.
 * @param threadEntity: ThreadEntity instance
 * @param node: Node definition
 * @returns: Execution result
 */
export async function forkHandler(threadEntity: ThreadEntity, node: Node): Promise<unknown> {
  // Check if it is possible to execute.
  if (!canExecute(threadEntity)) {
    return {
      nodeId: node.id,
      nodeType: node.type,
      status: "SKIPPED",
      step: threadEntity.getNodeResults().length + 1,
      executionTime: 0,
    };
  }

  // The Fork node serves as a placeholder; the actual Fork operation is handled by the ThreadExecutor, which in turn calls the ThreadOperationCoordinator.
  // Configuration parameters are read from node.config and no need to return configuration information
  return {};
}
