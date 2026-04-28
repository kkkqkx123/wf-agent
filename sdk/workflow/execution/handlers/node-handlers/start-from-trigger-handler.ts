/**
 * StartFromTrigger node function
 * Responsible for initializing the trigger sub-workflow and receiving input data from the main thread
 */

import type { Node, LLMMessage } from "@wf-agent/types";
import type { ThreadEntity } from "../../../entities/thread-entity.js";
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
function canExecute(threadEntity: ThreadEntity, node: Node): boolean {
  // The START_FROM_TRIGGER node can execute in either the CREATED or RUNNING state.
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
 * StartFromTrigger node processing function
 * @param threadEntity ThreadEntity instance
 * @param node Node definition
 * @param context Processor context (optional)
 * @returns Execution result
 */
export async function startFromTriggerHandler(
  threadEntity: ThreadEntity,
  node: Node,
  context?: StartFromTriggerHandlerContext,
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

  // Variables and results for initializing a Thread
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

  // Get the input data passed from the trigger from the context.
  const triggerInput = context?.triggerInput || {};

  // Set input data to thread.input
  const updatedInput = {
    ...threadEntity.getInput(),
    ...triggerInput,
  };
  thread.input = updatedInput;

  // If there are variables passed, initialize them in the thread.
  if (triggerInput.variables) {
    thread.variables = triggerInput.variables as typeof thread.variables;
  }

  // If there is any passed conversation history, initialize it into the conversationManager.
  if (triggerInput.conversationHistory && context?.conversationManager) {
    context.conversationManager.addMessages(...triggerInput.conversationHistory);
  }

  // Record execution history
  threadEntity.addNodeResult({
    step: threadEntity.getNodeResults().length + 1,
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
