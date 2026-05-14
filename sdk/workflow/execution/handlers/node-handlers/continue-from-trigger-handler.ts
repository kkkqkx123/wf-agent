/**
 * The ContinueFromTrigger node processing function is responsible for invoking the results back to the main workflow after the execution of the sub-workflows is completed.
 *
 * Note: Message context filtering/truncation has been migrated to the unified reference architecture.
 * This handler now handles both variable callbacks and message context outputs.
 */

import type { RuntimeNode, MessageContextRegistry, WorkflowExecution } from "@wf-agent/types";
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
 * ContinueFromTrigger node processing function
 * @param workflowExecutionEntity: WorkflowExecutionEntity instance
 * @param node: RuntimeNode definition
 * @param context: Processor context (optional)
 * @returns: Execution result
 */
export async function continueFromTriggerHandler(
  workflowExecutionEntity: WorkflowExecutionEntity,
  node: RuntimeNode,
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

  // Handle message context outputs if configured
  if (config.messageOutputs && config.messageOutputs.length > 0) {
    const workflowExecution = workflowExecutionEntity.getExecution();
    const mainWorkflowExecution = mainWorkflowExecutionEntity.getExecution();
    
    // Access message context registries
    const registry = (workflowExecution as WorkflowExecution & { messageContextRegistry?: MessageContextRegistry }).messageContextRegistry;
    const parentRegistry = (mainWorkflowExecution as WorkflowExecution & { messageContextRegistry?: MessageContextRegistry }).messageContextRegistry;
    
    if (registry && parentRegistry) {
      for (const outputDef of config.messageOutputs) {
        const { internalName, externalName } = outputDef;
        
        // Get the message context from subworkflow's registry
        const context = registry.get(internalName);
        
        if (context) {
          // Copy to parent workflow's registry with external name
          parentRegistry.register({
            id: externalName,
            messages: [...context.messages],  // Shallow copy
            createdAt: Date.now(),
            updatedAt: Date.now(),
            metadata: {
              ...context.metadata,
              sourceWorkflow: workflowExecutionEntity.getWorkflowId(),
            } as Record<string, unknown>,
          });
        }
      }
    }
  }

  // Handling variable outputs using new VariableManager architecture
  if (config.variableOutputs && config.variableOutputs.length > 0) {
    for (const outputDef of config.variableOutputs) {
      const { internalName, externalName } = outputDef;
      
      // Get the variable value from subworkflow's VariableManager
      const value = workflowExecutionEntity.variableStateManager.getVariable(internalName);
      
      if (value !== undefined) {
        // Set the variable in main workflow's VariableManager with external name
        mainWorkflowExecutionEntity.setVariable(externalName, value);
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
