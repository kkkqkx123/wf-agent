/**
 * The ContinueFromTrigger node processing function is responsible for invoking the results back to the main workflow after the execution of the sub-workflows is completed.
 *
 * Note: Message context filtering/truncation has been migrated to the unified reference architecture.
 * This handler now only handles variable callbacks.
 */

import type { Node } from "@wf-agent/types";
import type { ContinueFromTriggerNodeConfig } from "@wf-agent/types";
import { now } from "@wf-agent/common-utils";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";

/**
 * ContinueFromTrigger handler context
 */
export interface ContinueFromTriggerHandlerContext {
  /** Main workflow execution entity */
  mainWorkflowExecutionEntity?: WorkflowExecutionEntity;
}

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
 * ContinueFromTrigger node processing function
 * @param workflowExecutionEntity: WorkflowExecutionEntity instance
 * @param node: Node definition
 * @param context: Processor context (optional)
 * @returns: Execution result
 */
export async function continueFromTriggerHandler(
  workflowExecutionEntity: WorkflowExecutionEntity,
  node: Node,
  context?: ContinueFromTriggerHandlerContext,
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

  const config = node.config as ContinueFromTriggerNodeConfig;

  // Retrieve the main workflow execution entity (from the context).
  const mainWorkflowExecutionEntity = context?.mainWorkflowExecutionEntity;
  if (!mainWorkflowExecutionEntity) {
    throw new Error("Main workflow execution entity is required for CONTINUE_FROM_TRIGGER node");
  }

  // Handle messageOutputs if configured (currently just validates declaration)
  if (config.messageOutputs && config.messageOutputs.length > 0) {
    // TODO: Integrate with unified message reference architecture
    // For now, just validate that outputs are declared
    console.log('Message outputs declared:', config.messageOutputs.map(o => o.externalName));
  }

  // Handling variable callbacks
  const workflowExecution = workflowExecutionEntity.getExecution();
  if (config.variableCallback) {
    if (config.variableCallback.includeAll) {
      // Return all variables
      const allVariables = workflowExecution.variables || [];
      for (const v of allVariables) {
        mainWorkflowExecutionEntity.setVariable(v.name, v.value);
      }
    } else if (config.variableCallback.includeVariables) {
      // Selective variable return
      const variablesToCallback = (workflowExecution.variables || []).filter(v =>
        config.variableCallback?.includeVariables?.includes(v.name),
      );
      for (const v of variablesToCallback) {
        mainWorkflowExecutionEntity.setVariable(v.name, v.value);
      }
    }
  }

  // Note: Message context handling has been migrated to the unified reference architecture.
  // The conversationHistoryCallback and callbackOptions fields have been removed from ContinueFromTriggerNodeConfig.

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
    message: "Triggered subgraph completed and data callback executed",
    callbackExecuted: true,
  };
}
