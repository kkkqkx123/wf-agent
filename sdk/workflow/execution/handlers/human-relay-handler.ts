/**
 * HumanRelay Processing Function
 * Provides a stateless, human-relay execution service
 *
 * Responsibilities:
 * - Parse node configurations and create HumanRelay requests
 * - Trigger HumanRelay events
 * - Call the application layer processor to obtain human input
 * - Convert human input into LLM messages
 * - Trigger events indicating the completion or failure of the processing
 *
 * Design Principles:
 * - Stateless, functional design
 * - Each function performs a single task
 * - Dependencies are passed through parameters
 * - Consistency with other handlers
 *
 * Note:
 * HumanRelay is a substitute for the LLM client, intended to replace LLM API calls with human input, and is unrelated to human review processes.
 */

import type { LLMMessage } from "@wf-agent/types";
import type {
  HumanRelayRequest,
  HumanRelayResponse,
  HumanRelayExecutionResult,
  HumanRelayHandler,
  HumanRelayContext,
} from "@wf-agent/types";
import type { EventRegistry } from "../../../core/registry/event-registry.js";
import type { WorkflowExecutionEntity } from "../../entities/workflow-execution-entity.js";
import { MessageRole } from "@wf-agent/types";
import {
  generateId,
  now,
  diffTimestamp,
  getErrorMessage,
  getErrorOrNew,
} from "@wf-agent/common-utils";
import {
  buildHumanRelayRequestedEvent,
  buildHumanRelayRespondedEvent,
  buildHumanRelayProcessedEvent,
  buildHumanRelayFailedEvent,
} from "../../../core/utils/event/builders/interaction-events.js";

/**
 * HumanRelay Task Interface
 */
export interface HumanRelayTask {
  /** Message array (containing the conversation history) */
  messages: LLMMessage[];
  /** Prompt message */
  prompt: string;
  /** Timeout period (in milliseconds) */
  timeout: number;
  /** Thread Entity */
  threadEntity: WorkflowExecutionEntity;
  /** Request ID */
  requestId: string;
  /** Node ID */
  nodeId: string;
}

/**
 * Create a HumanRelay request
 * @param task The HumanRelay task
 * @returns The HumanRelay request
 */
export function createHumanRelayRequest(task: HumanRelayTask): HumanRelayRequest {
  return {
    requestId: task.requestId,
    messages: task.messages,
    prompt: task.prompt,
    timeout: task.timeout,
    metadata: {
      workflowId: task.threadEntity.getWorkflowId(),
      threadId: task.workflowExecutionEntity.id,
      nodeId: task.nodeId,
    },
  };
}

/**
 * Create a HumanRelay context
 * @param task The HumanRelay task
 * @returns The HumanRelay context
 */
export function createHumanRelayContext(task: HumanRelayTask): HumanRelayContext {
  const cancelToken = {
    cancelled: false,
    cancel: () => {
      cancelToken.cancelled = true;
    },
  };

  return {
    threadId: task.workflowExecutionEntity.id,
    workflowId: task.threadEntity.getWorkflowId(),
    nodeId: task.nodeId,
    getVariable: (variableName: string) => {
      return task.threadEntity.getVariable(variableName);
    },
    setVariable: async (variableName: string, value: unknown) => {
      task.threadEntity.setVariable(variableName, value);
    },
    getVariables: () => {
      return task.threadEntity.getAllVariables();
    },
    timeout: task.timeout,
    cancelToken,
  };
}

/**
 * Trigger the HUMAN_RELAY_REQUESTED event
 * @param task: HumanRelay task
 * @param request: HumanRelay request
 * @param eventManager: Event manager
 */
export async function emitHumanRelayRequestedEvent(
  task: HumanRelayTask,
  request: HumanRelayRequest,
  eventManager: EventRegistry,
): Promise<void> {
  await eventManager.emit(
    buildHumanRelayRequestedEvent({
      workflowId: task.threadEntity.getWorkflowId(),
      threadId: task.workflowExecutionEntity.id,
      requestId: request.requestId,
      prompt: request.prompt,
      messageCount: request.messages.length,
      timeout: request.timeout,
    }),
  );
}

/**
 * Trigger the HUMAN_RELAY_RESPONDED event
 * @param task: HumanRelay task
 * @param response: HumanRelay response
 * @param eventManager: event manager
 */
export async function emitHumanRelayRespondedEvent(
  task: HumanRelayTask,
  response: HumanRelayResponse,
  eventManager: EventRegistry,
): Promise<void> {
  await eventManager.emit(
    buildHumanRelayRespondedEvent({
      workflowId: task.threadEntity.getWorkflowId(),
      threadId: task.workflowExecutionEntity.id,
      requestId: response.requestId,
      content: response.content,
    }),
  );
}

/**
 * Trigger the HUMAN_RELAY_PROCESSED event
 * @param task: The HumanRelay task
 * @param message: The LLM message
 * @param executionTime: The execution time
 * @param eventManager: The event manager
 */
export async function emitHumanRelayProcessedEvent(
  task: HumanRelayTask,
  message: LLMMessage,
  executionTime: number,
  eventManager: EventRegistry,
): Promise<void> {
  await eventManager.emit(
    buildHumanRelayProcessedEvent({
      workflowId: task.threadEntity.getWorkflowId(),
      threadId: task.workflowExecutionEntity.id,
      requestId: task.requestId,
      message: {
        role: message.role,
        content:
          typeof message.content === "string" ? message.content : JSON.stringify(message.content),
      },
      executionTime,
    }),
  );
}

/**
 * Trigger the HUMAN_RELAY_FAILED event
 * @param task: The HumanRelay task
 * @param error: The error message
 * @param eventManager: The event manager
 */
export async function emitHumanRelayFailedEvent(
  task: HumanRelayTask,
  error: Error | string,
  eventManager: EventRegistry,
): Promise<void> {
  await eventManager.emit(
    buildHumanRelayFailedEvent({
      workflowId: task.threadEntity.getWorkflowId(),
      threadId: task.workflowExecutionEntity.id,
      requestId: task.requestId,
      reason: getErrorMessage(error),
    }),
  );
}

/**
 * Get human input
 * @param task HumanRelay task
 * @param request HumanRelay request
 * @param context HumanRelay context
 * @param handler HumanRelay handler
 * @returns HumanRelay response
 */
export async function getHumanInput(
  task: HumanRelayTask,
  request: HumanRelayRequest,
  context: HumanRelayContext,
  handler: HumanRelayHandler,
): Promise<HumanRelayResponse> {
  // Implement timeout control
  const timeoutPromise = new Promise<never>((_, reject) => {
    setTimeout(() => {
      reject(new Error(`HumanRelay timeout after ${request.timeout}ms`));
    }, request.timeout);
  });

  // Cancel control
  const cancelPromise = new Promise<never>((_, reject) => {
    const checkCancel = setInterval(() => {
      if (context.cancelToken.cancelled) {
        clearInterval(checkCancel);
        reject(new Error("HumanRelay cancelled"));
      }
    }, 100);
  });

  try {
    // Competition: Manual input, timeout, cancellation
    return await Promise.race([handler.handle(request, context), timeoutPromise, cancelPromise]);
  } finally {
    // Clean up the cancellation checks.
    context.cancelToken.cancel();
  }
}

/**
 * Convert human input into LLM messages
 * @param task HumanRelay task containing context information
 * @param response HumanRelay response containing user input
 * @returns LLM message with assistant role (since HumanRelay replaces LLM API calls)
 */
export function convertToLLMMessage(
  task: HumanRelayTask,
  response: HumanRelayResponse,
): LLMMessage {
  return {
    role: "assistant" as MessageRole,
    content: response.content,
    id: task.requestId,
    timestamp: response.timestamp,
    metadata: {
      source: "human-relay",
      nodeId: task.nodeId,
      workflowId: task.threadEntity.getWorkflowId(),
      threadId: task.workflowExecutionEntity.id,
    },
  };
}

/**
 * Execute HumanRelay
 * @param messages Array of messages (containing the conversation history)
 * @param prompt Prompt message
 * @param timeout Timeout period in milliseconds
 * @param threadContext Thread context
 * @param eventManager Event manager
 * @param humanRelayHandler HumanRelay handler
 * @param nodeId Node ID
 * @returns Execution result
 */
export async function executeHumanRelay(
  messages: LLMMessage[],
  prompt: string,
  timeout: number,
  threadEntity: WorkflowExecutionEntity,
  eventManager: EventRegistry,
  humanRelayHandler: HumanRelayHandler,
  nodeId: string,
): Promise<HumanRelayExecutionResult> {
  const requestId = generateId();
  const startTime = now();

  const task: HumanRelayTask = {
    messages,
    prompt,
    timeout,
    threadEntity,
    requestId,
    nodeId,
  };

  try {
    // 1. Create a HumanRelay request
    const request = createHumanRelayRequest(task);

    // 2. Trigger the HUMAN_RELAY_REQUESTED event
    await emitHumanRelayRequestedEvent(task, request, eventManager);

    // 3. Create the HumanRelay context
    const context = createHumanRelayContext(task);

    // 4. Call the application layer processor to obtain human input.
    const response = await getHumanInput(task, request, context, humanRelayHandler);

    // 5. Trigger the HUMAN_RELAY_RESPONDED event
    await emitHumanRelayRespondedEvent(task, response, eventManager);

    // 6. Convert human input into LLM messages
    const message = convertToLLMMessage(task, response);

    // 7. Trigger the HUMAN_RELAY_PROCESSED event
    const executionTime = diffTimestamp(startTime, now());
    await emitHumanRelayProcessedEvent(task, message, executionTime, eventManager);

    return {
      requestId,
      message,
      executionTime,
    };
  } catch (error) {
    // Trigger the HUMAN_RELAY_FAILED event.
    await emitHumanRelayFailedEvent(task, getErrorOrNew(error), eventManager);
    throw error;
  }
}
