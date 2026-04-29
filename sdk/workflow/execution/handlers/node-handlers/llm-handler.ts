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

import type { Node, LLMNodeConfig } from "@wf-agent/types";
import type { GraphLLMExecutionConfig } from "@wf-agent/types";
import type { WorkflowExecution, LLMMessage } from "@wf-agent/types";
import type { HumanRelayHandler } from "@wf-agent/types";
import { ExecutionError } from "@wf-agent/types";
import { now, diffTimestamp, getErrorOrNew } from "@wf-agent/common-utils";
import { templateRegistry } from "../../../../resources/predefined/template-registry.js";
import { graphLogger as logger } from "../../../../utils/logger.js";
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
 * Parse Prompt Words
 * Supports referencing templates via templateId or specifying the prompt directly
 * @param config LLM node configuration
 * @returns The parsed prompt words
 */
function resolvePrompt(config: LLMNodeConfig): string {
  // "Use templateId as a priority."
  if (config.promptTemplateId) {
    const template = templateRegistry.get(config.promptTemplateId);
    if (template) {
      return (
        templateRegistry.render(config.promptTemplateId, config.promptTemplateVariables || {}) ??
        template.content
      );
    }
    // Fall back to the direct prompt when the template does not exist.
    logger.warn(
      `Prompt template '${config.promptTemplateId}' not found, falling back to direct prompt`,
      { templateId: config.promptTemplateId },
    );
  }

  // Use the directly specified prompt
  return config.prompt || "";
}

/**
 * LLM Node Processor
 * @param thread Thread instance
 * @param node Node definition
 * @param context Processor context
 * @returns Execution result
 */
export async function llmHandler(
  thread: WorkflowExecution,
  node: Node,
  context: LLMHandlerContext,
): Promise<LLMExecutionResult> {
  const config = node.config as LLMNodeConfig;
  const startTime = now();

  try {
    // 1. Convert the configuration into executable data (the configuration has already passed static validation during workflow registration).
    const executionData = {
      prompt: resolvePrompt(config),
      profileId: config.profileId,
      parameters: config.parameters || {},
      maxToolCallsPerRequest: config.maxToolCallsPerRequest,
      stream: false,
    };

    // 2. Check if it is a HumanRelay provider.
    const profile = context.llmWrapper.getProfile(executionData.profileId || "DEFAULT");
    if (profile?.provider === "HUMAN_RELAY") {
      return await executeHumanRelayLLMNode(thread, node, executionData, context, startTime);
    }

    // 3. Create execution configuration
    const executionConfig: GraphLLMExecutionConfig = {
      profileId: executionData.profileId,
      parameters: executionData.parameters,
      maxToolCallsPerRequest: executionData.maxToolCallsPerRequest,
      workflowId: thread.workflowId,
      nodeId: node.id,
      executionId: thread.id,
    };

    // 4. Call LLMExecutionCoordinator
    const result = await context.llmCoordinator.executeLLM(
      {
        executionId: thread.id,
        nodeId: node.id,
        prompt: executionData.prompt,
        profileId: executionData.profileId,
        parameters: executionData.parameters,
        maxToolCallsPerRequest: executionData.maxToolCallsPerRequest,
      },
      context.conversationManager,
    );

    const endTime = now();

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
  thread: WorkflowExecution,
  node: Node,
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
      { thread, conversationManager: context.conversationManager } as unknown as WorkflowExecutionEntity, // Simplify the processing; in reality, the complete ThreadContext should be passed in.
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
