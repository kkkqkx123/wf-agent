/**
 * Subgraph Processing Function
 * Responsible for handling the logic related to the entry and exit of subgraphs
 *
 * Responsibilities:
 * - Handle the logic for subgraphs to enter
 * - Handle the logic for subgraphs to exit
 * - Create metadata for the subgraph context
 *
 * Design Principles:
 * - Reuse existing path parsing functions
 * - Avoid duplicating the implementation of variable parsing logic
 * - Provide a clear interface for subgraph processing
 * - Use pure functions with no internal state
 */

import type { ThreadEntity } from "../../entities/workflow-execution-entity.js";
import { now } from "@wf-agent/common-utils";

/**
 * Enter the subgraph
 * @param threadEntity Thread entity
 * @param workflowId Subgraph workflow ID
 * @param parentWorkflowId Parent workflow ID
 * @param input Subgraph input
 */
export async function enterSubgraph(
  threadEntity: ThreadEntity,
  workflowId: string,
  parentWorkflowId: string,
  input: Record<string, unknown>,
): Promise<void> {
  await threadEntity.enterSubgraph(workflowId, parentWorkflowId, input);
}

/**
 * Exit the subgraph
 * @param threadEntity Thread entity
 */
export async function exitSubgraph(threadEntity: ThreadEntity): Promise<void> {
  await threadEntity.exitSubgraph();
}

/**
 * Get subgraph input
 * @param threadEntity Thread entity
 * @returns Subgraph input data (using the variable system)
 */
export function getSubgraphInput(threadEntity: ThreadEntity): Record<string, unknown> {
  // Using a variable system to retrieve input data
  return threadEntity.getAllVariables();
}

/**
 * Get the subgraph output
 * @param threadEntity Thread entity
 * @returns Subgraph output data
 */
export function getSubgraphOutput(threadEntity: ThreadEntity): Record<string, unknown> {
  const subgraphContext = threadEntity.getCurrentSubgraphContext();
  if (!subgraphContext) return {};

  // Get the output of the END node of the subgraph.
  const graph = threadEntity.getGraph();
  const endNodes = graph.endNodeIds;

  for (const endNodeId of endNodes) {
    const graphNode = graph.getNode(endNodeId);
    if (graphNode?.workflowId === subgraphContext.workflowId) {
      // Find the END node of the subgraph and obtain its output.
      const nodeResult = threadEntity.getNodeResults().find(r => r.nodeId === endNodeId);
      // Note: NodeExecutionResult doesn't have a 'data' property, so we return an empty object
      // This may need to be updated based on the actual implementation
      return {};
    }
  }

  return {};
}

/**
 * Create subgraph context metadata
 * @param triggerId Trigger ID (optional)
 * @param mainThreadId Main thread ID (optional)
 * @returns Subgraph context metadata
 */
export function createSubgraphMetadata(
  triggerId?: string,
  mainThreadId?: string,
): Record<string, unknown> {
  const metadata: Record<string, unknown> = {
    timestamp: now(),
  };

  if (triggerId || mainThreadId) {
    metadata["triggeredBy"] = {
      triggerId,
      mainThreadId,
      timestamp: now(),
    };
    metadata["isTriggeredSubgraph"] = true;
  }

  return metadata;
}
