/**
 * The ContinueFromTrigger node processing function (batch-aware) is responsible for invoking the results back to the main workflow after the execution of the sub-workflows is completed.
 *
 */

import type { Node } from "@wf-agent/types";
import type { ContinueFromTriggerNodeConfig } from "@wf-agent/types";
import { now } from "@wf-agent/common-utils";
import { executeOperation } from "../../../../core/utils/messages/message-operation-utils.js";
import { getVisibleMessages } from "../../../../core/utils/messages/visible-range-calculator.js";
import type { MessageOperationContext } from "@wf-agent/types";
import type { ConversationSession } from "../../../../core/messaging/conversation-session.js";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";

/**
 * ContinueFromTrigger handler context
 */
export interface ContinueFromTriggerHandlerContext {
  /** Main workflow execution entity */
  mainWorkflowExecutionEntity?: WorkflowExecutionEntity;
  /** Main thread entity */
  mainThreadEntity?: WorkflowExecutionEntity;
  /** Conversation manager */
  conversationManager?: ConversationSession;
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

  // Retrieve the main thread entity (from the context).
  const mainThreadEntity = context?.mainThreadEntity;
  if (!mainThreadEntity) {
    throw new Error("Main thread entity is required for CONTINUE_FROM_TRIGGER node");
  }

  // Handling variable callbacks
  const thread = workflowExecutionEntity.getExecution();
  if (config.variableCallback) {
    if (config.variableCallback.includeAll) {
      // Return all variables
      const allVariables = thread.variables || [];
      for (const v of allVariables) {
        mainThreadEntity.setVariable(v.name, v.value);
      }
    } else if (config.variableCallback.includeVariables) {
      // Selective variable return
      const variablesToCallback = (thread.variables || []).filter(v =>
        config.variableCallback?.includeVariables?.includes(v.name),
      );
      for (const v of variablesToCallback) {
        mainThreadEntity.setVariable(v.name, v.value);
      }
    }
  }

  // Handling conversation history callbacks (batch awareness)
  if (config.conversationHistoryCallback) {
    const conversationManager = context?.conversationManager;
    if (conversationManager) {
      const allMessages = conversationManager.getAllMessages();
      const markMap = conversationManager.getMarkMap();

      // Constructing the operation context
      const operationContext: MessageOperationContext = {
        messages: allMessages,
        markMap: markMap,
        options: config.callbackOptions || {},
      };

      // Execute message operations
      const result = await executeOperation(operationContext, config.conversationHistoryCallback);

      // Get the visible messages and send them back to the main thread.
      const visibleMessages = getVisibleMessages(result.messages, result.markMap);

      // Send the message back to the main workflow execution.
      for (const msg of visibleMessages) {
        mainWorkflowExecutionEntity?.addMessage(msg);
      }
    }
  }

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
