/**
 * Join node processing function
 * The Join node serves as a placeholder; the actual Join operation is handled by the ThreadExecutor invoking the ThreadCoordinator.
 */

import type { Node } from "@wf-agent/types";
import type { ThreadEntity } from "../../../entities/thread-entity.js";

/**
 * Check if the node can be executed.
 */
function canExecute(threadEntity: ThreadEntity): boolean {
  if (threadEntity.getStatus() !== "RUNNING") {
    return false;
  }
  return true;
}

/**
 * Join Node Processing Function
 * The Join node serves as a placeholder; the actual Join operation is handled by the ThreadExecutor calling the ThreadCoordinator.
 *
 * Explanation:
 * - The sub-thread ID is dynamically obtained from the execution context, not from the node configuration.
 * - The default timeout is 0 (no timeout), which can be overridden in the configuration.
 *
 * @param threadEntity: ThreadEntity instance
 * @param node: Node definition
 * @returns: Execution result
 */
export async function joinHandler(threadEntity: ThreadEntity, node: Node): Promise<unknown> {
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

  // The `Join` node serves as a placeholder, and the actual `Join` operation is handled by the `ThreadExecutor` invoking the `ThreadOperationCoordinator`.
  // The configuration parameters are read from `node.config`, and there is no need to return the configuration information.
  return {};
}
