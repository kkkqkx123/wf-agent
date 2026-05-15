/**
 * LLM Node Processor
 * Responsible for executing LLM nodes and handling LLM API calls
 *
 * Design Principles:
 * - Only includes the core execution logic
 * - Relyes on the LLMExecutionCoordinator for actual LLM calls
 * - Business logic such as tool approval is handled by the Graph module
 * - Returns the execution results
 * - Supports referencing predefined prompt templates via templateId
 */

import type { RuntimeNode, LLMNodeConfig, MessageContextRegistry } from "@wf-agent/types";
import type { WorkflowExecution, LLMMessage } from "@wf-agent/types";
import type { HumanRelayHandler } from "@wf-agent/types";
import { ExecutionError, RuntimeValidationError } from "@wf-agent/types";
import { now, diffTimestamp, getErrorOrNew } from "@wf-agent/common-utils";
import { LLMExecutionCoordinator } from "../../coordinators/llm-execution-coordinator.js";
import { LLMWrapper } from "../../../../core/llm/wrapper.js";
import { executeHumanRelay } from "../human-relay-handler.js";
import type { EventRegistry } from "../../../../core/registry/event-registry.js";
import type { ConversationSession } from "../../../../core/messaging/conversation-session.js";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";

/**
 * LLM node execution results
 */
export interface LLMExecutionResult {
  /** Execution Status */
  status: "COMPLETED" | "FAILED";
  /** LLM response content */
  content?: string;
  /** Error message (in case of failure) */
  error?: Error;
  /** Execution time (in milliseconds) */
  executionTime: number;
}

/**
 * LLM processor context
 */
export interface LLMHandlerContext {
  /** LLM Execution Coordinator */
  llmCoordinator: LLMExecutionCoordinator;
  /** LLM Wrapper */
  llmWrapper: LLMWrapper;
  /** Event Manager */
  eventManager: EventRegistry;
  /** Dialogue Manager */
  conversationManager: ConversationSession;
  /** HumanRelay processor (optional) */
  humanRelayHandler?: unknown;
}

/**
 * Collect messages from named contexts
 */
function collectMessagesFromContexts(
  config: LLMNodeConfig,
  workflowExecution: WorkflowExecution,
): LLMMessage[] {
  // Get MessageContextRegistry from execution context
  const registry = (workflowExecution as WorkflowExecution & { messageContextRegistry?: MessageContextRegistry }).messageContextRegistry;
  
  if (!registry) {
    throw new RuntimeValidationError("MessageContextRegistry not found in execution context", {
      operation: "collectMessagesFromContexts",
      field: "messageContextRegistry",
    });
  }

  // Use default context if contextRefs not specified
  const contextRefs = config.contextRefs && config.contextRefs.length > 0 
    ? config.contextRefs 
    : ['current'];

  const allMessages: LLMMessage[] = [];
  
  for (const contextId of contextRefs) {
    const namedContext = registry.get(contextId);
    
    if (!namedContext) {
      throw new RuntimeValidationError(`Context '${contextId}' not found`, {
        operation: "collectMessagesFromContexts",
        field: "contextRefs",
        value: contextId,
      });
    }
    
    allMessages.push(...namedContext.messages);
  }
  
  return allMessages;
}

/**
 * LLM Node Processor
 * @param workflowExecution Workflow execution instance
 * @param node Node definition
 * @param context Processor context
 * @returns Execution result
 */
export async function llmHandler(
  workflowExecution: WorkflowExecution,
  node: RuntimeNode,
  context: LLMHandlerContext,
): Promise<LLMExecutionResult> {
  const config = node.config as LLMNodeConfig;
  const startTime = now();

  try {
    // 1. Collect messages from named contexts
    const messages = collectMessagesFromContexts(config, workflowExecution);
    
    // 2. Add messages to conversation manager for this execution
    for (const message of messages) {
      context.conversationManager.addMessage(message);
    }

    // 3. Convert the configuration into executable data
    const executionData = {
      prompt: "", // Empty prompt - messages come from context references
      profileId: config.profileId,
      parameters: config.parameters || {},
      maxToolCallsPerRequest: config.maxToolCallsPerRequest,
      stream: false,
    };

    // 4. Check if it is a HumanRelay provider.
    const profile = context.llmWrapper.getProfile(executionData.profileId || "DEFAULT");
    if (profile?.provider === "HUMAN_RELAY") {
      return await executeHumanRelayLLMNode(workflowExecution, node, executionData, context, startTime);
    }

    // 5. Call LLMExecutionCoordinator
    const result = await context.llmCoordinator.executeLLM(
      {
        executionId: workflowExecution.id,
        nodeId: node.id,
        prompt: executionData.prompt,
        profileId: executionData.profileId,
        parameters: executionData.parameters,
        maxToolCallsPerRequest: executionData.maxToolCallsPerRequest,
      },
      context.conversationManager,
    );

    const endTime = now();

    // 6. Append response to output context if specified
    if (result.success && config.outputContext) {
      const registry = (workflowExecution as WorkflowExecution & { messageContextRegistry?: MessageContextRegistry }).messageContextRegistry;
      if (registry) {
        const outputContextId = config.outputContext;
        const existingContext = registry.get(outputContextId);
        
        if (existingContext) {
          // Update existing context
          registry.update(outputContextId, [
            ...existingContext.messages,
            { role: "assistant", content: result.content || "" },
          ]);
        } else {
          // Create new context
          registry.register({
            id: outputContextId,
            messages: [{ role: "assistant", content: result.content || "" }],
            createdAt: Date.now(),
            updatedAt: Date.now(),
            metadata: { description: `Output from LLM node ${node.id}` },
          });
        }
      }
    }

    if (result.success) {
      return {
        status: "COMPLETED",
        content: result.content,
        executionTime: diffTimestamp(startTime, endTime),
      };
    } else {
      return {
        status: "FAILED",
        error: result.error,
        executionTime: diffTimestamp(startTime, endTime),
      };
    }
  } catch (error) {
    const endTime = now();
    return {
      status: "FAILED",
      error: getErrorOrNew(error),
      executionTime: diffTimestamp(startTime, endTime),
    };
  }
}

/**
 * Execute the HumanRelay LLM node
 */
async function executeHumanRelayLLMNode(
  workflowExecution: WorkflowExecution,
  node: RuntimeNode,
  requestData: { prompt?: string; parameters?: { timeout?: number } },
  context: LLMHandlerContext,
  startTime: number,
): Promise<LLMExecutionResult> {
  if (!context.humanRelayHandler) {
    throw new ExecutionError("HumanRelayHandler is not provided", node.id);
  }

  try {
    // Get the current conversation message
    const messages = context.conversationManager.getMessages() as unknown[];

    // Call the executeHumanRelay function.
    const result = await executeHumanRelay(
      messages as LLMMessage[],
      requestData.prompt || "Please provide your input:",
      requestData.parameters?.timeout || 300000,
      { workflowExecutionEntity: workflowExecution, conversationManager: context.conversationManager } as unknown as WorkflowExecutionEntity, // Simplify the processing; in reality, the complete WorkflowExecutionContext should be passed in.
      context.eventManager,
      context.humanRelayHandler as HumanRelayHandler,
      node.id,
    );

    const endTime = now();

    // HumanRelay executed successfully, and the results have been returned.
    return {
      status: "COMPLETED",
      content:
        typeof result.message.content === "string"
          ? result.message.content
          : JSON.stringify(result.message.content),
      executionTime: diffTimestamp(startTime, endTime),
    };
  } catch (error) {
    const endTime = now();
    return {
      status: "FAILED",
      error: error instanceof Error ? error : new Error(String(error)),
      executionTime: diffTimestamp(startTime, endTime),
    };
  }
}
