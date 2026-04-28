/**
 * LLM Execution Coordinator
 * Coordinates LLM calls and tool calls execution flow
 *
 * Core Responsibilities:
 * 1. Coordinate LLM calls
 * 2. Coordinate tool calls
 * 3. Manage conversation state
 * 4. Monitor token usage
 * 5. Handle interruption (via AbortSignal)
 * 6. Trigger LLM-related events (message, token, conversation state)
 *
 * Design Principles:
 * - Pure LLM coordination logic
 * - Contains only LLM-call-inherent features (events, token tracking)
 * - No business-specific features (tool approval, checkpoint)
 * - Reusable across modules (Graph, Agent, etc.)
 */

import type { LLMMessage, ToolSchema, LLMUsage } from "@wf-agent/types";
import type { LLMExecutionConfig } from "@wf-agent/types";
import { MessageRole } from "@wf-agent/types";
import { ConversationSession } from "../messaging/conversation-session.js";
import { ExecutionError } from "@wf-agent/types";
import {
  checkInterruption,
  shouldContinue,
  getInterruptionDescription,
} from "@wf-agent/common-utils";
import type { InterruptionCheckResult } from "@wf-agent/common-utils";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import { LLMExecutor } from "../executors/llm-executor.js";
import { ToolCallExecutor } from "../executors/tool-call-executor.js";
import { prepareToolSchemasFromTools } from "../utils/tools/tool-schema-helper.js";
import type { EventRegistry } from "../registry/event-registry.js";
import {
  buildMessageAddedEvent,
  buildTokenUsageWarningEvent,
  buildConversationStateChangedEvent,
} from "../utils/event/builders/index.js";

const logger = createContextualLogger();

/**
 * LLM Execution Params
 */
export interface LLMExecutionParams {
  /** Execution context ID (thread ID, session ID, etc.) */
  contextId: string;
  /** Prompt text */
  prompt: string;
  /** LLM execution configuration */
  config: LLMExecutionConfig;
  /** Available tools */
  tools?: unknown[];
  /** Abort signal for cancellation */
  abortSignal?: AbortSignal;
  /** Event manager for triggering events */
  eventManager?: EventRegistry;
  /** Node ID (optional, for error tracking) */
  nodeId?: string;
}

/**
 * LLM Execution Response
 */
export interface LLMExecutionResponse {
  /** Whether execution succeeded */
  success: boolean;
  /** LLM response content (if successful) */
  content?: string;
  /** Error information (if failed) */
  error?: Error;
  /** Message history */
  messages?: LLMMessage[];
}

/**
 * LLM Execution Coordinator Class
 *
 * Responsibilities:
 * - Coordinate LLM calls and tool calls
 * - Manage conversation state
 * - Monitor token usage
 * - Handle interruption
 * - Trigger LLM-related events
 *
 * Design Principles:
 * - Pure coordination logic
 * - Contains only LLM-call-inherent features
 * - Reusable across modules
 */
export class LLMExecutionCoordinator {
  /**
   * Constructor
   *
   * @param llmExecutor LLM executor
   * @param toolCallExecutor Tool call executor
   */
  constructor(
    private llmExecutor: LLMExecutor,
    private toolCallExecutor: ToolCallExecutor,
  ) {}

  /**
   * Execute LLM call
   *
   * This method coordinates the complete LLM-tool call loop:
   * 1. Add user message to conversation
   * 2. Execute LLM call
   * 3. Execute tool calls (if any)
   * 4. Trigger events (message, token, conversation state)
   * 5. Return final result
   *
   * Note: Tool approval and checkpoint creation should be handled by the caller.
   *
   * @param params Execution parameters
   * @param conversationState Conversation manager
   * @returns Execution result
   */
  async executeLLM(
    params: LLMExecutionParams,
    conversationState: ConversationSession,
  ): Promise<LLMExecutionResponse> {
    // Execute complete LLM-tool call loop
    const result = await this.executeLLMLoop(params, conversationState);

    // Check if it's an interruption state
    if (typeof result !== "string") {
      // It's an interruption state
      return {
        success: false,
        error: new Error(getInterruptionDescription(result)),
      };
    }

    // Normal return
    return {
      success: true,
      content: result,
      messages: conversationState.getMessages(),
    };
  }

  /**
   * Execute complete LLM-tool call loop
   *
   * Core Responsibilities:
   * 1. Execute complete LLM-tool call loop
   * 2. Control loop iteration count
   * 3. Manage Token usage monitoring
   * 4. Handle conversation state
   * 5. Trigger events
   *
   * @param params Execution parameters
   * @param conversationState Conversation manager
   * @returns LLM response content or interruption state
   */
  private async executeLLMLoop(
    params: LLMExecutionParams,
    conversationState: ConversationSession,
  ): Promise<string | InterruptionCheckResult> {
    const { contextId, prompt, config, tools, abortSignal, eventManager, nodeId } = params;

    const {
      profileId,
      parameters,
      maxToolCallsPerRequest,
      enableTokenTracking,
      tokenWarningThreshold,
      tokenLimit,
    } = config;

    // Check interruption
    if (abortSignal) {
      const interruption = checkInterruption(abortSignal);
      if (!shouldContinue(interruption)) {
        return interruption;
      }
    }

    // Step 1: Add user message
    const userMessage = {
      role: "user" as MessageRole,
      content: prompt,
    };
    conversationState.addMessage(userMessage);

    // Trigger message added event
    if (eventManager) {
      await this.triggerMessageAddedEvent(eventManager, contextId, userMessage, nodeId);
    }

    // Check Token usage
    if (enableTokenTracking !== false) {
      await conversationState.checkTokenUsage();

      // Check Token usage warning
      const tokenUsage = conversationState.getTokenUsage();
      if (tokenUsage && eventManager) {
        const limit = tokenLimit || 100000;
        const threshold = tokenWarningThreshold || 80;
        const usagePercentage = (tokenUsage.totalTokens / limit) * 100;

        // Trigger warning when usage exceeds threshold
        if (usagePercentage > threshold) {
          await this.triggerTokenWarningEvent(
            eventManager,
            contextId,
            tokenUsage.totalTokens,
            limit,
            usagePercentage,
          );
        }
      }
    }

    // Prepare tool schemas
    let availableToolSchemas = tools;
    if (tools && tools.length > 0) {
      availableToolSchemas = prepareToolSchemasFromTools(
        tools as Array<{ id: string; description: string; parameters: unknown }>,
      );
    }

    // Check interruption again before LLM call
    if (abortSignal) {
      const interruption = checkInterruption(abortSignal);
      if (!shouldContinue(interruption)) {
        return interruption;
      }
    }

    // Execute LLM call
    const llmResult = await this.llmExecutor.executeLLMCall(
      conversationState.getMessages(),
      {
        prompt,
        profileId: profileId || "DEFAULT",
        parameters: parameters || {},
        tools: availableToolSchemas as ToolSchema[],
      },
      { abortSignal, threadId: contextId, nodeId },
    );

    // Check if it's an interruption state
    if (!llmResult.success) {
      return (llmResult as { success: false; interruption: InterruptionCheckResult }).interruption;
    }

    const result = llmResult.result;

    // Update Token usage statistics
    if (result.usage) {
      conversationState.updateTokenUsage(result.usage as LLMUsage);
    }

    // Finalize current request Token statistics
    conversationState.finalizeCurrentRequest();

    // Add LLM response to conversation history
    const assistantMessage = {
      role: "assistant" as MessageRole,
      content: result.content,
      toolCalls: result.toolCalls?.map((tc: { id: string; name: string; arguments: string }) => ({
        id: tc.id,
        type: "function" as const,
        function: {
          name: tc.name,
          arguments: tc.arguments,
        },
      })),
    };
    conversationState.addMessage(assistantMessage);

    // Trigger message added event
    if (eventManager) {
      await this.triggerMessageAddedEvent(eventManager, contextId, assistantMessage, nodeId);
    }

    // Check if there are tool calls
    if (result.toolCalls && result.toolCalls.length > 0) {
      // Validate single response tool call count
      const maxToolsPerResponse = maxToolCallsPerRequest ?? 3;
      if (result.toolCalls.length > maxToolsPerResponse) {
        throw new ExecutionError(
          `LLM returned ${result.toolCalls.length} tool calls, ` +
            `exceeds limit of ${maxToolsPerResponse}. ` +
            `Configure maxToolCallsPerRequest to adjust this limit.`,
          nodeId,
        );
      }

      // Check interruption before tool call execution
      if (abortSignal) {
        const interruption = checkInterruption(abortSignal);
        if (!shouldContinue(interruption)) {
          return interruption;
        }
      }

      // Execute tool calls (pass AbortSignal)
      await this.toolCallExecutor.executeToolCalls(
        result.toolCalls,
        conversationState,
        contextId,
        nodeId || "",
        { abortSignal },
      );
    }

    // Trigger conversation state changed event
    if (eventManager) {
      const finalTokenUsage = conversationState.getTokenUsage();
      await this.triggerConversationStateChangedEvent(
        eventManager,
        contextId,
        conversationState.getMessages().length,
        finalTokenUsage?.totalTokens || 0,
        nodeId,
      );
    }

    // Return final content
    return result.content;
  }

  /**
   * Trigger message added event
   */
  private async triggerMessageAddedEvent(
    eventManager: EventRegistry,
    contextId: string,
    message: { role: string; content: string },
    nodeId?: string,
  ): Promise<void> {
    try {
      const event = buildMessageAddedEvent({
        threadId: contextId,
        role: message.role,
        content: message.content,
        nodeId,
      });
      await eventManager.emit(event);
    } catch (error) {
      logger.warn("Failed to trigger message added event", { contextId, error });
    }
  }

  /**
   * Trigger token usage warning event
   */
  private async triggerTokenWarningEvent(
    eventManager: EventRegistry,
    contextId: string,
    tokensUsed: number,
    tokenLimit: number,
    usagePercentage: number,
  ): Promise<void> {
    try {
      const event = buildTokenUsageWarningEvent({
        threadId: contextId,
        tokensUsed,
        tokenLimit,
        usagePercentage,
      });
      await eventManager.emit(event);
    } catch (error) {
      logger.warn("Failed to trigger token warning event", { contextId, error });
    }
  }

  /**
   * Trigger conversation state changed event
   */
  private async triggerConversationStateChangedEvent(
    eventManager: EventRegistry,
    contextId: string,
    messageCount: number,
    tokenUsage: number,
    nodeId?: string,
  ): Promise<void> {
    try {
      const event = buildConversationStateChangedEvent({
        threadId: contextId,
        messageCount,
        tokenUsage,
        nodeId,
      });
      await eventManager.emit(event);
    } catch (error) {
      logger.warn("Failed to trigger conversation state changed event", { contextId, error });
    }
  }
}
