/**
 * Agent Iteration Executor
 *
 * Responsible for executing a single iteration of the Agent loop.
 * Handles LLM call and tool calls for one iteration.
 *
 * Design Principles:
 * - Stateless design, all state managed through AgentLoopEntity
 * - Single responsibility: one iteration execution
 * - Reusable by both sync and stream executors
 * - Integrates with Hook mechanism
 * - Uses ConversationSession for message management
 */

import type { LLMMessage, AgentHookTriggeredEvent, LLMToolCall, ToolSchema } from "@wf-agent/types";
import type { AgentLoopEntity } from "../../entities/agent-loop-entity.js";
import type { ConversationSession } from "../../../core/messaging/conversation-session.js";
import type { LLMExecutor } from "../../../core/executors/llm-executor.js";
import type { ToolCallExecutor } from "../../../core/executors/tool-call-executor.js";
import { executeAgentHook } from "../handlers/hook-handlers/index.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "AgentIterationExecutor" });

/**
 * Iteration execution result
 */
export interface IterationResult {
  /** Whether iteration completed successfully */
  success: boolean;
  /** Whether loop should continue */
  shouldContinue: boolean;
  /** Response content (if no tool calls) */
  content?: string;
  /** Error message (if failed) */
  error?: string;
  /** Interruption type (if interrupted) */
  interruption?: "paused" | "stopped" | "aborted";
}

/**
 * Agent Iteration Executor
 *
 * Executes a single iteration of the Agent loop:
 * 1. Call LLM
 * 2. Process response
 * 3. Execute tool calls if any
 * 4. Return iteration result
 */
export class AgentIterationExecutor {
  constructor(
    private llmExecutor: LLMExecutor,
    private toolCallExecutor: ToolCallExecutor,
    private emitAgentEvent: (event: AgentHookTriggeredEvent) => Promise<void>,
  ) {}

  /**
   * Execute a single iteration
   *
   * @param entity Agent loop entity
   * @param conversationManager Conversation manager (unified message management)
   * @param toolSchemas Tool schemas for LLM
   * @param profileId LLM profile ID
   * @returns Iteration result
   */
  async execute(
    entity: AgentLoopEntity,
    conversationManager: ConversationSession,
    toolSchemas: ToolSchema[] | undefined,
    profileId: string,
  ): Promise<IterationResult> {
    const agentLoopId = entity.id;
    const iteration = entity.state.currentIteration;

    // ========== BEFORE_ITERATION Hook ==========
    await executeAgentHook(entity, "BEFORE_ITERATION", this.emitAgentEvent);

    // Start new iteration
    entity.state.startIteration();

    // ========== BEFORE_LLM_CALL Hook ==========
    await executeAgentHook(entity, "BEFORE_LLM_CALL", this.emitAgentEvent);

    logger.debug("Calling LLM", {
      agentLoopId,
      iteration: entity.state.currentIteration,
      messageCount: conversationManager.getMessageCount(),
    });

    // Execute LLM call
    const llmResult = await this.llmExecutor.executeLLMCall(
      conversationManager.getMessages(),
      {
        prompt: "",
        profileId,
        parameters: {},
        tools: toolSchemas,
        stream: false,
      },
      {
        abortSignal: entity.getAbortSignal(),
        executionId: entity.id,
        nodeId: entity.nodeId,
      },
    );

    logger.debug("LLM call completed", {
      agentLoopId,
      iteration: entity.state.currentIteration,
      success: llmResult.success,
      hasToolCalls: llmResult.success ? !!llmResult.result?.toolCalls?.length : false,
    });

    // Handle interruption
    if (!llmResult.success) {
      const interruption = llmResult.interruption;
      if (interruption.type === "paused") {
        logger.info("LLM call paused", { agentLoopId, iteration });
        entity.state.pause();
        return {
          success: false,
          shouldContinue: false,
          interruption: "paused",
        };
      }
      if (interruption.type === "stopped" || interruption.type === "aborted") {
        logger.info("LLM call stopped", { agentLoopId, iteration });
        entity.state.cancel();
        return {
          success: false,
          shouldContinue: false,
          interruption: interruption.type,
        };
      }
      throw new Error("LLM execution failed with unknown error");
    }

    const response = llmResult.result;

    // ========== AFTER_LLM_CALL Hook ==========
    await executeAgentHook(entity, "AFTER_LLM_CALL", this.emitAgentEvent, undefined, {
      content: response.content,
      toolCalls: response.toolCalls,
    });

    // Convert toolCalls to LLMToolCall format
    const toolCalls: LLMToolCall[] | undefined = response.toolCalls?.map(tc => ({
      id: tc.id,
      type: "function" as const,
      function: {
        name: tc.name,
        arguments: tc.arguments,
      },
    }));

    // Record assistant message
    const assistantMessage: LLMMessage = {
      role: "assistant",
      content: response.content,
      toolCalls,
    };
    conversationManager.addAssistantMessage(response.content, toolCalls);
    entity.addMessage(assistantMessage);

    // Check if tool calls are needed
    if (!response.toolCalls || response.toolCalls.length === 0) {
      logger.debug("No tool calls required, completing execution", {
        agentLoopId,
        iteration: entity.state.currentIteration,
        contentLength: response.content.length,
      });

      entity.state.endIteration(response.content);

      // ========== AFTER_ITERATION Hook ==========
      await executeAgentHook(entity, "AFTER_ITERATION", this.emitAgentEvent);

      entity.state.complete();
      return {
        success: true,
        shouldContinue: false,
        content: response.content,
      };
    }

    // Execute tool calls
    await this.executeToolCalls(entity, conversationManager, response.toolCalls);

    entity.state.endIteration(response.content);

    // ========== AFTER_ITERATION Hook ==========
    await executeAgentHook(entity, "AFTER_ITERATION", this.emitAgentEvent);

    return {
      success: true,
      shouldContinue: true,
      content: response.content,
    };
  }

  /**
   * Execute tool calls
   *
   * @param entity Agent loop entity
   * @param conversationManager Conversation manager
   * @param toolCalls Tool calls from LLM response
   */
  private async executeToolCalls(
    entity: AgentLoopEntity,
    conversationManager: ConversationSession,
    toolCalls: Array<{ id: string; name: string; arguments: string }>,
  ): Promise<void> {
    const agentLoopId = entity.id;
    const iteration = entity.state.currentIteration;

    logger.debug("Executing tool calls", {
      agentLoopId,
      iteration,
      toolCallCount: toolCalls.length,
    });

    // Save original tool calls for Hook
    const originalToolCalls = toolCalls;

    // Use ToolCallExecutor to execute tool calls
    const toolResults = await this.toolCallExecutor.executeToolCalls(
      toolCalls.map(tc => ({
        id: tc.id,
        name: tc.name,
        arguments: tc.arguments,
      })),
      conversationManager,
      entity.id,
      entity.nodeId,
      { abortSignal: entity.getAbortSignal() },
    );

    logger.debug("Tool calls execution completed", {
      agentLoopId,
      iteration,
      successCount: toolResults.filter(r => r.success).length,
      failureCount: toolResults.filter(r => !r.success).length,
    });

    // Process results and trigger hooks
    for (const result of toolResults) {
      const originalToolCall = originalToolCalls.find(tc => tc.id === result.toolCallId);
      const toolCallInfo = {
        id: result.toolCallId,
        name: originalToolCall?.name || "",
        arguments: originalToolCall?.arguments ? JSON.parse(originalToolCall.arguments) : {},
      };

      // ========== BEFORE_TOOL_CALL Hook ==========
      await executeAgentHook(entity, "BEFORE_TOOL_CALL", this.emitAgentEvent, toolCallInfo);

      if (result.success) {
        logger.debug("Tool call succeeded", {
          agentLoopId,
          toolCallId: result.toolCallId,
          toolName: toolCallInfo.name,
        });

        entity.state.recordToolCallEnd(result.toolCallId, result.result);

        // ========== AFTER_TOOL_CALL Hook ==========
        await executeAgentHook(entity, "AFTER_TOOL_CALL", this.emitAgentEvent, {
          ...toolCallInfo,
          result: result.result,
        });
      } else {
        logger.warn("Tool call failed", {
          agentLoopId,
          toolCallId: result.toolCallId,
          toolName: toolCallInfo.name,
          error: result.error,
        });

        entity.state.recordToolCallEnd(result.toolCallId, undefined, result.error);

        // ========== AFTER_TOOL_CALL Hook (with error) ==========
        await executeAgentHook(entity, "AFTER_TOOL_CALL", this.emitAgentEvent, {
          ...toolCallInfo,
          error: result.error,
        });
      }
    }
  }
}
