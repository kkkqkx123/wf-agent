/**
 * StartFromTrigger node function
 * Responsible for initializing the trigger sub-workflow and receiving input data from the main workflow execution
 */

import type { Node, LLMMessage } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";
import { now } from "@wf-agent/common-utils";
import type { ConversationSession } from "../../../../core/messaging/conversation-session.js";

/**
 * StartFromTrigger handler context
 */
export interface StartFromTriggerHandlerContext {
  /** Trigger input data */
  triggerInput?: {
    variables?: Array<{ name: string; value: unknown }>;
    conversationHistory?: LLMMessage[];
  };
  /** Conversation manager */
  conversationManager?: ConversationSession;
}

/**
 * Check whether the node can be executed.
 */
function canExecute(workflowExecutionEntity: WorkflowExecutionEntity, node: Node): boolean {
  // The START_FROM_TRIGGER node can execute in either the CREATED or RUNNING state.
  const status = workflowExecutionEntity.getStatus();
  if (status !== "CREATED" && status !== "RUNNING") {
    return false;
  }

  if (workflowExecutionEntity.getNodeResults().some(result => result.nodeId === node.id)) {
    return false;
  }

  return true;
}

/**
 * StartFromTrigger node processing function
 * @param workflowExecutionEntity WorkflowExecutionEntity instance
 * @param node Node definition
 * @param context Processor context (optional)
 * @returns Execution result
 */
export async function startFromTriggerHandler(
  workflowExecutionEntity: WorkflowExecutionEntity,
  node: Node,
  context?: StartFromTriggerHandlerContext,
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

  // Initialize WorkflowExecution state via WorkflowExecutionEntity
  workflowExecutionEntity.setStatus("RUNNING");
  workflowExecutionEntity.setCurrentNodeId(node.id);
  workflowExecutionEntity.state.start();

  // Variables and results for initializing a WorkflowExecution
  const workflowExecution = workflowExecutionEntity.getExecution();
  if (!workflowExecution.variables) {
    workflowExecution.variables = [];
  }
  if (!workflowExecution.errors) {
    workflowExecution.errors = [];
  }

  // Initialize WorkflowExecution input
  if (!workflowExecution.input) {
    workflowExecution.input = {};
  }

  // Get the input data passed from the trigger from the context.
  const triggerInput = context?.triggerInput || {};

  // Set input data to workflowExecution.input
  const updatedInput = {
    ...workflowExecutionEntity.getInput(),
    ...triggerInput,
  };
  workflowExecution.input = updatedInput;

  // If there are variables passed, initialize them in the workflowExecution.
  if (triggerInput.variables) {
    workflowExecution.variables = triggerInput.variables as typeof workflowExecution.variables;
  }

  // If there is any passed conversation history, initialize it into the conversationManager.
  if (triggerInput.conversationHistory && context?.conversationManager) {
    context.conversationManager.addMessages(...triggerInput.conversationHistory);
  }

  // Record execution history
  workflowExecutionEntity.addNodeResult({
    step: workflowExecutionEntity.getNodeResults().length + 1,
    nodeId: node.id,
    nodeType: node.type,
    status: "COMPLETED",
    timestamp: now(),
  });

  // Return the execution results
  return {
    message: "Triggered subgraph started",
    input: updatedInput,
  };
}
