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
  executeWithInterruptionHandling,
  getWorkflowInterruptionDescription,
} from "../utils/interruption/index.js";
import type { ExecutionInterruptionCheckResult } from "../utils/interruption/index.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import { LLMExecutor } from "../executors/llm-executor.js";
import { ToolCallExecutor } from "../executors/tool-call-executor.js";
import { prepareToolSchemasFromTools } from "../utils/tools/tool-schema-helper.js";
import type { EventRegistry } from "../registry/event-registry.js";
import type { TokenMetricsCollector } from "../metrics/token-collector.js";
import {
  buildMessageAddedEvent,
  buildTokenUsageWarningEvent,
  buildConversationStateChangedEvent,
} from "../utils/event/builders/index.js";

const logger = createContextualLogger();

/**
 * Custom error to carry interruption information through the call stack
 */
class InterruptionError extends Error {
  constructor(public interruption: ExecutionInterruptionCheckResult) {
    super("Operation interrupted");
    this.name = "InterruptionError";
  }
}

/**
 * LLM Execution Params
 */
export interface LLMExecutionParams {
  /** Execution context ID (execution ID, session ID, etc.) */
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
  /** Whether to execute tool calls automatically (default: true) */
  executeTools?: boolean;
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
   * @param tokenMetricsCollector Optional token metrics collector
   */
  constructor(
    private llmExecutor: LLMExecutor,
    private toolCallExecutor: ToolCallExecutor,
    private tokenMetricsCollector?: TokenMetricsCollector,
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
    // Execute single LLM call with optional tool execution
    const result = await this.executeSingleLLMCall(params, conversationState);

    // Check if it's an interruption state
    if (typeof result !== "string") {
      // It's an interruption state
      const description = getWorkflowInterruptionDescription(result);
      const error = new Error(description);
      // Preserve original interruption info as cause if available
      if (result && typeof result === 'object' && 'reason' in result) {
        (error as any).cause = result;
      }
      return {
        success: false,
        error,
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
   * Execute a single LLM call with optional tool execution
   *
   * This method performs ONE complete LLM interaction:
   * 1. Add user message to conversation
   * 2. Execute single LLM call
   * 3. Execute tool calls if present (when executeTools=true)
   * 4. Update token usage and trigger warnings
   * 5. Trigger events (message, token, conversation state)
   *
   * Note: This is NOT a loop. If callers need multiple iterations
   * (LLM -> Tool -> LLM), they should call executeLLM() repeatedly.
   *
   * @param params Execution parameters
   * @param conversationState Conversation manager
   * @returns LLM response content or interruption state
   */
  private async executeSingleLLMCall(
    params: LLMExecutionParams,
    conversationState: ConversationSession,
  ): Promise<string | ExecutionInterruptionCheckResult> {
    const { contextId, prompt, config, tools, abortSignal, eventManager, nodeId, executeTools = true } = params;

    const {
      profileId,
      parameters,
      maxToolCallsPerRequest,
      enableTokenTracking,
      tokenWarningThreshold,
      tokenLimit,
    } = config;

    // Use unified interruption handler for the entire LLM execution flow
    const result = await executeWithInterruptionHandling(
      async (signal) => {
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

        // Prepare tool schemas
        let availableToolSchemas = tools;
        if (tools && tools.length > 0) {
          availableToolSchemas = prepareToolSchemasFromTools(
            tools as Array<{ id: string; description: string; parameters: unknown }>,
          );
        }

        // Execute LLM call with signal
        const llmResult = await this.llmExecutor.executeLLMCall(
          conversationState.getMessages(),
          {
            prompt,
            profileId: profileId || "DEFAULT",
            parameters: parameters || {},
            tools: availableToolSchemas as ToolSchema[],
          },
          { abortSignal: signal, executionId: contextId, nodeId },
        );

        // LLM Executor now throws errors directly (including AbortError)
        // The executeWithInterruptionHandling wrapper will catch and handle interruptions
        const llmResponse = llmResult;

        // Update Token usage statistics
        if (llmResponse.usage) {
          conversationState.updateTokenUsage(llmResponse.usage as LLMUsage);
          
          // Record token metrics
          if (this.tokenMetricsCollector && llmResponse.usage) {
            const usage = llmResponse.usage as any;
            this.tokenMetricsCollector.recordTokenUsage({
              profileId: params.config.profileId || "DEFAULT",
              executionId: params.contextId,
              nodeId: params.nodeId,
              totalTokens: usage.totalTokens || 0,
              promptTokens: usage.promptTokens || 0,
              completionTokens: usage.completionTokens || 0,
              cost: usage.totalCost,
            });
          }
        }

        // Finalize current request Token statistics
        conversationState.finalizeCurrentRequest();

        // Check Token usage warning AFTER updating with new usage
        if (enableTokenTracking !== false && eventManager) {
          const tokenUsage = conversationState.getTokenUsage();
          if (tokenUsage) {
            const limit = tokenLimit || 100000;
            const threshold = tokenWarningThreshold || 80;
            const usagePercentage = (tokenUsage.totalTokens / limit) * 100;

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

        // Add LLM response to conversation history
        const assistantMessage = {
          role: "assistant" as MessageRole,
          content: llmResponse.content,
          toolCalls: llmResponse.toolCalls?.map((tc: { id: string; name: string; arguments: string }) => ({
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

        // Check if there are tool calls and should execute them
        if (llmResponse.toolCalls && llmResponse.toolCalls.length > 0 && executeTools) {
          // Validate single response tool call count
          const maxToolsPerResponse = maxToolCallsPerRequest ?? 3;
          if (llmResponse.toolCalls.length > maxToolsPerResponse) {
            throw new ExecutionError(
              `LLM returned ${llmResponse.toolCalls.length} tool calls, ` +
                `exceeds limit of ${maxToolsPerResponse}. ` +
                `Configure maxToolCallsPerRequest to adjust this limit.`,
              nodeId,
            );
          }

          // Execute tool calls with signal
          await this.toolCallExecutor.executeToolCalls(
            llmResponse.toolCalls,
            conversationState,
            contextId,
            nodeId || "",
            { abortSignal: signal },
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
        return llmResponse.content;
      },
      abortSignal,
    );

    // Handle result
    if (!result.success) {
      return result.interruption;
    }

    return result.result;
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
        executionId: contextId,
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
        executionId: contextId,
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
        executionId: contextId,
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
