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

import type { WorkflowExecutionEntity } from "../../entities/workflow-execution-entity.js";
import { now, checkInterruption, shouldContinue, getInterruptionDescription } from "@wf-agent/common-utils";
import type { InterruptionCheckResult } from "@wf-agent/common-utils";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "subgraph-handler" });

/**
 * Enter the subgraph
 * @param executionEntity WorkflowExecution entity
 * @param workflowId Subgraph workflow ID
 * @param parentWorkflowId Parent workflow ID
 * @param input Subgraph input
 */
export async function enterSubgraph(
  executionEntity: WorkflowExecutionEntity,
  workflowId: string,
  parentWorkflowId: string,
  input: Record<string, unknown>,
): Promise<void> {
  // Check for interruption before entering subgraph
  const abortSignal = executionEntity.getAbortSignal();
  const interruption = checkInterruption(abortSignal);
  
  if (!shouldContinue(interruption)) {
    logger.info("Subgraph entry interrupted", {
      executionId: executionEntity.id,
      workflowId,
      interruptionType: interruption.type,
    });
    throw new Error(`Subgraph entry interrupted: ${getInterruptionDescription(interruption)}`);
  }

  await executionEntity.enterSubgraph(workflowId, parentWorkflowId, input);
}

/**
 * Exit the subgraph
 * @param executionEntity WorkflowExecution entity
 */
export async function exitSubgraph(executionEntity: WorkflowExecutionEntity): Promise<void> {
  // Check for interruption before exiting subgraph
  const abortSignal = executionEntity.getAbortSignal();
  const interruption = checkInterruption(abortSignal);
  
  if (!shouldContinue(interruption)) {
    logger.info("Subgraph exit interrupted", {
      executionId: executionEntity.id,
      interruptionType: interruption.type,
    });
    throw new Error(`Subgraph exit interrupted: ${getInterruptionDescription(interruption)}`);
  }

  await executionEntity.exitSubgraph();
}

/**
 * Get subgraph input
 * @param executionEntity WorkflowExecution entity
 * @returns Subgraph input data (using the variable system)
 */
export function getSubgraphInput(executionEntity: WorkflowExecutionEntity): Record<string, unknown> {
  // Using a variable system to retrieve input data
  return executionEntity.getAllVariables();
}

/**
 * Get the subgraph output
 * @param executionEntity WorkflowExecution entity
 * @returns Subgraph output data
 */
export function getSubgraphOutput(executionEntity: WorkflowExecutionEntity): Record<string, unknown> {
  const subgraphContext = executionEntity.getCurrentSubgraphContext();
  if (!subgraphContext) return {};

  // Get the output of the END node of the subgraph.
  const graph = executionEntity.getGraph();
  const endNodes = graph.endNodeIds;

  for (const endNodeId of endNodes) {
    const graphNode = graph.getNode(endNodeId);
    if (graphNode?.workflowId === subgraphContext.workflowId) {
      // Find the END node of the subgraph and obtain its output.
      const nodeResult = executionEntity.getNodeResults().find(r => r.nodeId === endNodeId);
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
 * @param mainExecutionId Main execution ID (optional)
 * @returns Subgraph context metadata
 */
export function createSubgraphMetadata(
  triggerId?: string,
  mainExecutionId?: string,
): Record<string, unknown> {
  const metadata: Record<string, unknown> = {
    timestamp: now(),
  };

  if (triggerId || mainExecutionId) {
    metadata["triggeredBy"] = {
      triggerId,
      mainExecutionId,
      timestamp: now(),
    };
    metadata["isTriggeredSubgraph"] = true;
  }

  return metadata;
}
