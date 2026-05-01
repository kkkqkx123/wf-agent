/**
 * Agent Stream Executor
 *
 * Responsible for executing Agent loop with streaming support.
 * Handles real-time event forwarding and stream-specific logic.
 *
 * Design Principles:
 * - Stateless design, all state managed through AgentLoopEntity
 * - Real-time event forwarding via AsyncGenerator
 * - Integrates with Hook mechanism
 * - Reuses ToolCallExecutor for consistent tool execution
 * - Uses ConversationSession for message management
 */

import type { LLMMessage, ToolSchema, AgentCustomEvent, AgentStreamEvent, MessageStreamEvent } from "@wf-agent/types";
import { AgentStreamEventType } from "@wf-agent/types";
import type { AgentLoopEntity } from "../../entities/agent-loop-entity.js";
import type { ConversationSession } from "../../../core/messaging/conversation-session.js";
import type { LLMExecutor } from "../../../core/executors/llm-executor.js";
import type { ToolCallExecutor } from "../../../core/executors/tool-call-executor.js";
import type { ToolRegistry } from "../../../core/registry/tool-registry.js";
import type { EventRegistry } from "../../../core/registry/event-registry.js";
import type { Event as RegistryEvent } from "@wf-agent/types";
import { isAbortError, checkInterruption } from "@wf-agent/common-utils";
import { executeAgentHook } from "../handlers/hook-handlers/index.js";
import {
  handleAgentError,
  handleAgentInterruption as handleAgentInterruptionHandler,
} from "../handlers/agent-error-handler.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "AgentStreamExecutor" });

/**
 * Agent Loop Stream Event
 *
 * Union type containing:
 * - LLM layer events (MessageStreamEvent): text delta, tool argument parsing, etc.
 * - Agent layer events (AgentStreamEvent): tool calls, iteration complete, etc.
 */
export type AgentLoopStreamEvent = MessageStreamEvent | AgentStreamEvent;

/**
 * Agent Stream Executor
 *
 * Executes Agent loop with streaming support:
 * 1. Stream LLM response with real-time event forwarding
 * 2. Execute tool calls using ToolCallExecutor
 * 3. Emit Agent-level events
 */
export class AgentStreamExecutor {
  constructor(
    private llmExecutor: LLMExecutor,
    private toolCallExecutor: ToolCallExecutor,
    private toolService: ToolRegistry,
    private emitAgentEvent: (event: AgentCustomEvent) => Promise<void>,
    private eventManager?: EventRegistry,
  ) {}

  /**
   * Emit event to both stream and EventRegistry
   *
   * This ensures events are available for:
   * 1. Real-time streaming consumers (via yield)
   * 2. Persistence and querying (via EventRegistry)
   */
  private async emitToRegistry<T extends AgentLoopStreamEvent>(
    event: T,
    entity: AgentLoopEntity,
  ): Promise<void> {
    // Also emit to EventRegistry if available
    if (this.eventManager) {
      try {
        const registryEvent = this.convertToRegistryEvent(event, entity);
        if (registryEvent) {
          await this.eventManager.emit(registryEvent);
        }
      } catch (error) {
        logger.warn("Failed to emit event to EventRegistry", {
          eventType: event.type,
          error: error instanceof Error ? error.message : String(error),
        });
      }
    }
  }

  /**
   * Convert stream event to registry-compatible event
   */
  private convertToRegistryEvent(
    event: AgentLoopStreamEvent,
    entity: AgentLoopEntity,
  ): RegistryEvent | null {
    const baseData = {
      id: crypto.randomUUID(),
      timestamp: Date.now(),
      workflowId: entity.id, // AgentLoopEntity.id is the execution ID
      executionId: entity.id,
      agentLoopId: entity.id,
      nodeId: entity.nodeId,
      metadata: {},
    };

    switch (event.type) {
      case AgentStreamEventType.AGENT_START:
        return {
          ...baseData,
          type: "AGENT_STARTED",
          maxIterations: event.maxIterations,
          initialMessageCount: event.initialMessageCount,
        };

      case AgentStreamEventType.AGENT_END:
        return {
          ...baseData,
          type: "AGENT_COMPLETED",
          iterations: event.iterations,
          toolCallCount: event.toolCallCount,
          success: event.success,
        };

      case AgentStreamEventType.ITERATION_COMPLETE:
        return {
          ...baseData,
          type: "AGENT_ITERATION_COMPLETED",
          iteration: event.iteration,
          toolCallCount: 0, // Will be updated by caller
          shouldContinue: event.shouldContinue,
        };

      case AgentStreamEventType.TOOL_EXECUTION_START:
        return {
          ...baseData,
          type: "AGENT_TOOL_EXECUTION_STARTED",
          toolCallId: event.toolCallId,
          toolName: event.toolName,
          iteration: event.iteration,
        };

      case AgentStreamEventType.TOOL_EXECUTION_END:
        return {
          ...baseData,
          type: "AGENT_TOOL_EXECUTION_COMPLETED",
          toolCallId: event.toolCallId,
          toolName: event.toolName,
          success: event.result.success,
          duration: event.duration,
          error: event.result.success ? undefined : event.result.error,
          iteration: entity.state.currentIteration,
        };

      case AgentStreamEventType.ERROR:
        // Don't emit ERROR events to registry - they're already handled by error handlers
        return null;

      default:
        // For MessageStreamEvent types (text, message, etc.), don't emit to registry
        // These are too granular and would overwhelm storage
        return null;
    }
  }

  /**
   * Execute Agent loop with streaming
   *
   * @param entity Agent loop entity
   * @param conversationManager Conversation manager (unified message management)
   * @param toolSchemas Tool schemas for LLM
   * @param profileId LLM profile ID
   * @param maxIterations Maximum iterations
   * @returns Stream event generator
   */
  async *execute(
    entity: AgentLoopEntity,
    conversationManager: ConversationSession,
    toolSchemas: unknown[] | undefined,
    profileId: string,
    maxIterations: number,
  ): AsyncGenerator<AgentLoopStreamEvent> {
    const agentLoopId = entity.id;

    yield {
      type: AgentStreamEventType.AGENT_START,
      timestamp: Date.now(),
      agentLoopId,
      maxIterations,
      initialMessageCount: conversationManager.getMessageCount(),
    } as const;
    await this.emitToRegistry(
      {
        type: AgentStreamEventType.AGENT_START,
        timestamp: Date.now(),
        agentLoopId,
        maxIterations,
        initialMessageCount: conversationManager.getMessageCount(),
      },
      entity,
    );

    try {
      while (entity.state.currentIteration < maxIterations) {
        logger.debug("Starting new stream iteration", {
          agentLoopId,
          iteration: entity.state.currentIteration + 1,
          maxIterations,
        });

        // Check interruption signals
        if (entity.isAborted() || entity.shouldStop()) {
          logger.info("Agent Loop stream execution cancelled", {
            agentLoopId,
            iteration: entity.state.currentIteration,
          });
          entity.state.cancel();
          yield {
            type: AgentStreamEventType.ERROR,
            timestamp: Date.now(),
            agentLoopId,
            error: "Execution cancelled",
            iteration: entity.state.currentIteration,
            context: "execution_cancelled",
          } as const;
          await this.emitToRegistry(
            {
              type: AgentStreamEventType.ERROR,
              timestamp: Date.now(),
              agentLoopId,
              error: "Execution cancelled",
              iteration: entity.state.currentIteration,
              context: "execution_cancelled",
            },
            entity,
          );
          return;
        }

        // Check pause signal
        if (entity.shouldPause()) {
          logger.info("Agent Loop stream execution paused", {
            agentLoopId,
            iteration: entity.state.currentIteration,
          });
          entity.state.pause();
          yield {
            type: AgentStreamEventType.ERROR,
            timestamp: Date.now(),
            agentLoopId,
            error: "Execution paused",
            iteration: entity.state.currentIteration,
            context: "execution_paused",
          } as const;
          await this.emitToRegistry(
            {
              type: AgentStreamEventType.ERROR,
              timestamp: Date.now(),
              agentLoopId,
              error: "Execution paused",
              iteration: entity.state.currentIteration,
              context: "execution_paused",
            },
            entity,
          );
          return;
        }

        // Execute one iteration with streaming
        const shouldContinue = yield* this.executeIterationStream(
          entity,
          conversationManager,
          toolSchemas,
          profileId,
        );

        if (!shouldContinue) {
          return;
        }
      }

      // Reached max iterations
      logger.info("Agent Loop stream reached maximum iterations", {
        agentLoopId,
        maxIterations,
        toolCallCount: entity.state.toolCallCount,
      });

      entity.state.complete();
      yield {
        type: AgentStreamEventType.AGENT_END,
        timestamp: Date.now(),
        agentLoopId,
        messages: conversationManager.getMessages(),
        iterations: entity.state.currentIteration,
        toolCallCount: entity.state.toolCallCount,
        success: true,
      };
    } catch (error) {
      const standardizedError = await handleAgentError(
        entity,
        error as Error,
        "agent_loop_stream_execution",
      );

      yield {
        type: AgentStreamEventType.ERROR,
        timestamp: Date.now(),
        agentLoopId,
        error: standardizedError,
        iteration: entity.state.currentIteration,
        context: "agent_loop_stream_execution",
      } as const;
      await this.emitToRegistry(
        {
          type: AgentStreamEventType.ERROR,
          timestamp: Date.now(),
          agentLoopId,
          error: standardizedError,
          iteration: entity.state.currentIteration,
          context: "agent_loop_stream_execution",
        },
        entity,
      );
    }
  }

  /**
   * Execute one iteration with streaming
   */
  private async *executeIterationStream(
    entity: AgentLoopEntity,
    conversationManager: ConversationSession,
    toolSchemas: unknown[] | undefined,
    profileId: string,
  ): AsyncGenerator<AgentLoopStreamEvent, boolean> {
    const agentLoopId = entity.id;

    // ========== BEFORE_ITERATION Hook ==========
    await executeAgentHook(entity, "BEFORE_ITERATION", this.emitAgentEvent);

    // Start new iteration
    entity.state.startIteration();

    // ========== BEFORE_LLM_CALL Hook ==========
    await executeAgentHook(entity, "BEFORE_LLM_CALL", this.emitAgentEvent);

    logger.debug("Calling LLM for stream", {
      agentLoopId,
      iteration: entity.state.currentIteration,
      messageCount: conversationManager.getMessageCount(),
    });

    // Get LLM wrapper for streaming
    const llmWrapperResult = await this.llmExecutor["llmWrapper"].generateStream({
      profileId,
      messages: conversationManager.getMessages(),
      tools: toolSchemas as ToolSchema[],
      stream: true,
      signal: entity.getAbortSignal(),
    });

    if (llmWrapperResult.isErr()) {
      const error = llmWrapperResult.error;

      if (isAbortError(error)) {
        const isInterruption = await handleAgentInterruptionHandler(
          entity,
          error,
          "llm_stream_call",
          this.eventManager,
        );
        if (isInterruption) {
          yield {
            type: AgentStreamEventType.ERROR,
            timestamp: Date.now(),
            agentLoopId,
            error: (entity.state.error as Error | undefined)?.message || "Execution interrupted",
            iteration: entity.state.currentIteration,
            context: "llm_stream_call",
          } as const;
          await this.emitToRegistry(
            {
              type: AgentStreamEventType.ERROR,
              timestamp: Date.now(),
              agentLoopId,
              error: (entity.state.error as Error | undefined)?.message || "Execution interrupted",
              iteration: entity.state.currentIteration,
              context: "llm_stream_call",
            },
            entity,
          );
          return false;
        }
      }

      const standardizedError = await handleAgentError(
        entity,
        error,
        "llm_stream_call",
        undefined,
        this.eventManager,
      );

      yield {
        type: AgentStreamEventType.ERROR,
        timestamp: Date.now(),
        agentLoopId,
        error: standardizedError,
        iteration: entity.state.currentIteration,
        context: "agent_loop_stream_execution",
      } as const;
      await this.emitToRegistry(
        {
          type: AgentStreamEventType.ERROR,
          timestamp: Date.now(),
          agentLoopId,
          error: standardizedError,
          iteration: entity.state.currentIteration,
          context: "agent_loop_stream_execution",
        },
        entity,
      );
      return false;
    }

    const messageStream = llmWrapperResult.value;

    // Real-time event forwarding
    const eventQueue: MessageStreamEvent[] = [];
    let streamDone = false;
    let streamError: Error | null = null;

    // Subscribe to MessageStream events
    messageStream
      .on("text", ((delta: string, snapshot: string) => {
        eventQueue.push({ type: "text", delta, snapshot });
      }) as unknown as (event: MessageStreamEvent) => void)
      .on("inputJson", ((partialJson: string, parsedSnapshot: unknown, snapshot: LLMMessage) => {
        eventQueue.push({ type: "inputJson", partialJson, parsedSnapshot, snapshot });
      }) as unknown as (event: MessageStreamEvent) => void)
      .on("message", ((message: LLMMessage) => {
        eventQueue.push({ type: "message", message });
      }) as unknown as (event: MessageStreamEvent) => void)
      .on("error", ((error: Error) => {
        eventQueue.push({ type: "error", error });
      }) as unknown as (event: MessageStreamEvent) => void);

    // Start stream completion wait
    messageStream
      .done()
      .then(() => {
        streamDone = true;
      })
      .catch(error => {
        streamError = error;
        streamDone = true;
      });

    // Forward events in real-time
    while (!streamDone || eventQueue.length > 0) {
      // Check interruption
      if (entity.isAborted() || entity.shouldStop()) {
        const result = checkInterruption(entity.getAbortSignal());
        entity.state.cancel();
        yield {
          type: AgentStreamEventType.ERROR,
          timestamp: Date.now(),
          agentLoopId,
          error: result.type === "paused" ? "Execution paused" : "Execution cancelled",
          iteration: entity.state.currentIteration,
          context: "stream_interruption",
        } as const;
        await this.emitToRegistry(
          {
            type: AgentStreamEventType.ERROR,
            timestamp: Date.now(),
            agentLoopId,
            error: result.type === "paused" ? "Execution paused" : "Execution cancelled",
            iteration: entity.state.currentIteration,
            context: "stream_interruption",
          },
          entity,
        );
        return false;
      }

      // Forward queued events
      while (eventQueue.length > 0) {
        const event = eventQueue.shift()!;
        yield event;
        // Note: MessageStreamEvent types are not emitted to EventRegistry
        // They are too granular and would overwhelm storage
      }

      if (!streamDone) {
        await new Promise(resolve => setTimeout(resolve, 0));
      }
    }

    // Handle stream completion error
    if (streamError) {
      const error = streamError;
      if (isAbortError(error)) {
        const isInterruption = await handleAgentInterruptionHandler(
          entity,
          error,
          "message_stream_done",
          this.eventManager,
        );
        if (isInterruption) {
          yield {
            type: AgentStreamEventType.ERROR,
            timestamp: Date.now(),
            agentLoopId,
            error: (entity.state.error as Error | undefined)?.message || "Execution interrupted",
            iteration: entity.state.currentIteration,
            context: "llm_stream_call",
          } as const;
          await this.emitToRegistry(
            {
              type: AgentStreamEventType.ERROR,
              timestamp: Date.now(),
              agentLoopId,
              error: (entity.state.error as Error | undefined)?.message || "Execution interrupted",
              iteration: entity.state.currentIteration,
              context: "llm_stream_call",
            },
            entity,
          );
          return false;
        }
      }

      const standardizedError = await handleAgentError(
        entity,
        error,
        "message_stream_done",
        undefined,
        this.eventManager,
      );

      yield {
        type: AgentStreamEventType.ERROR,
        timestamp: Date.now(),
        agentLoopId,
        error: standardizedError,
        iteration: entity.state.currentIteration,
        context: "agent_loop_stream_execution",
      } as const;
      await this.emitToRegistry(
        {
          type: AgentStreamEventType.ERROR,
          timestamp: Date.now(),
          agentLoopId,
          error: standardizedError,
          iteration: entity.state.currentIteration,
          context: "agent_loop_stream_execution",
        },
        entity,
      );
      return false;
    }

    const finalResult = await messageStream.getFinalResult();

    // ========== AFTER_LLM_CALL Hook ==========
    await executeAgentHook(entity, "AFTER_LLM_CALL", this.emitAgentEvent, undefined, {
      content: finalResult.content,
      toolCalls: finalResult.toolCalls,
    });

    // Record assistant message
    const assistantMessage: LLMMessage = {
      role: "assistant",
      content: finalResult.content,
      toolCalls: finalResult.toolCalls?.map(
        (tc: { id: string; function: { name: string; arguments: string } }) => ({
          id: tc.id,
          type: "function" as const,
          function: {
            name: tc.function.name,
            arguments: tc.function.arguments,
          },
        }),
      ),
    };
    conversationManager.addAssistantMessage(
      finalResult.content,
      finalResult.toolCalls?.map(
        (tc: { id: string; function: { name: string; arguments: string } }) => ({
          id: tc.id,
          type: "function" as const,
          function: {
            name: tc.function.name,
            arguments: tc.function.arguments,
          },
        }),
      ),
    );
    entity.addMessage(assistantMessage);

    // Check if tool calls are needed
    if (!finalResult.toolCalls || finalResult.toolCalls.length === 0) {
      logger.debug("No tool calls required in stream, completing execution", {
        agentLoopId,
        iteration: entity.state.currentIteration,
        contentLength: finalResult.content.length,
      });

      entity.state.endIteration(finalResult.content);

      // ========== AFTER_ITERATION Hook ==========
      await executeAgentHook(entity, "AFTER_ITERATION", this.emitAgentEvent);

      entity.state.complete();
      yield {
        type: AgentStreamEventType.AGENT_END,
        timestamp: Date.now(),
        agentLoopId,
        messages: conversationManager.getMessages(),
        iterations: entity.state.currentIteration,
        toolCallCount: entity.state.toolCallCount,
        success: true,
      } as const;
      await this.emitToRegistry(
        {
          type: AgentStreamEventType.AGENT_END,
          timestamp: Date.now(),
          agentLoopId,
          messages: conversationManager.getMessages(),
          iterations: entity.state.currentIteration,
          toolCallCount: entity.state.toolCallCount,
          success: true,
        },
        entity,
      );
      return false;
    }

    // Execute tool calls using ToolCallExecutor
    yield* this.executeToolCallsStream(entity, conversationManager, finalResult.toolCalls);

    yield {
      type: AgentStreamEventType.ITERATION_COMPLETE,
      timestamp: Date.now(),
      agentLoopId,
      iteration: entity.state.currentIteration,
      shouldContinue: true,
    } as const;
    await this.emitToRegistry(
      {
        type: AgentStreamEventType.ITERATION_COMPLETE,
        timestamp: Date.now(),
        agentLoopId,
        iteration: entity.state.currentIteration,
        shouldContinue: true,
      },
      entity,
    );

    logger.debug("Stream iteration completed", {
      agentLoopId,
      iteration: entity.state.currentIteration,
    });

    entity.state.endIteration(finalResult.content);

    // ========== AFTER_ITERATION Hook ==========
    await executeAgentHook(entity, "AFTER_ITERATION", this.emitAgentEvent);

    return true;
  }

  /**
   * Execute tool calls with streaming events
   */
  private async *executeToolCallsStream(
    entity: AgentLoopEntity,
    conversationManager: ConversationSession,
    toolCalls: Array<{ id: string; function: { name: string; arguments: string } }>,
  ): AsyncGenerator<AgentLoopStreamEvent> {
    const agentLoopId = entity.id;

    logger.debug("Executing tool calls in stream", {
      agentLoopId,
      iteration: entity.state.currentIteration,
      toolCallCount: toolCalls.length,
    });

    // Convert tool calls to executor format
    const executorToolCalls = toolCalls.map(tc => ({
      id: tc.id,
      name: tc.function.name,
      arguments: tc.function.arguments,
    }));

    // Use ToolCallExecutor for consistent execution
    const toolResults = await this.toolCallExecutor.executeToolCalls(
      executorToolCalls,
      conversationManager,
      entity.id,
      entity.nodeId,
      { abortSignal: entity.getAbortSignal() },
    );

    // Emit events for each tool call result
    for (const result of toolResults) {
      const toolCall = toolCalls.find(tc => tc.id === result.toolCallId);
      const args = toolCall ? JSON.parse(toolCall.function.arguments) : {};

      // ========== BEFORE_TOOL_CALL Hook ==========
      const toolCallInfo = {
        id: result.toolCallId,
        name: toolCall?.function.name || "",
        arguments: args,
      };
      await executeAgentHook(entity, "BEFORE_TOOL_CALL", this.emitAgentEvent, toolCallInfo);

      yield {
        type: AgentStreamEventType.TOOL_EXECUTION_START,
        timestamp: Date.now(),
        agentLoopId,
        toolCallId: result.toolCallId,
        toolName: toolCallInfo.name,
        args: args,
        iteration: entity.state.currentIteration,
      };

      // Also emit to EventRegistry
      if (this.eventManager) {
        const registryEvent = this.convertToRegistryEvent(
          {
            type: AgentStreamEventType.TOOL_EXECUTION_START,
            timestamp: Date.now(),
            agentLoopId,
            toolCallId: result.toolCallId,
            toolName: toolCallInfo.name,
            args: args,
            iteration: entity.state.currentIteration,
          },
          entity,
        );
        if (registryEvent) {
          await this.eventManager.emit(registryEvent);
        }
      }

      entity.state.recordToolCallStart(result.toolCallId, toolCallInfo.name, args);

      if (result.success) {
        entity.state.recordToolCallEnd(result.toolCallId, result.result);

        // ========== AFTER_TOOL_CALL Hook ==========
        await executeAgentHook(entity, "AFTER_TOOL_CALL", this.emitAgentEvent, {
          ...toolCallInfo,
          result: result.result,
        });

        yield {
          type: AgentStreamEventType.TOOL_EXECUTION_END,
          timestamp: Date.now(),
          agentLoopId,
          toolCallId: result.toolCallId,
          toolName: toolCallInfo.name,
          result: {
            success: true,
            result: result.result,
            executionTime: result.executionTime,
            retryCount: 0,
          },
          duration: result.executionTime,
        };

        // Also emit to EventRegistry
        if (this.eventManager) {
          const registryEvent = this.convertToRegistryEvent(
            {
              type: AgentStreamEventType.TOOL_EXECUTION_END,
              timestamp: Date.now(),
              agentLoopId,
              toolCallId: result.toolCallId,
              toolName: toolCallInfo.name,
              result: {
                success: true,
                result: result.result,
                executionTime: result.executionTime,
                retryCount: 0,
              },
              duration: result.executionTime,
            },
            entity,
          );
          if (registryEvent) {
            await this.eventManager.emit(registryEvent);
          }
        }
      } else {
        entity.state.recordToolCallEnd(result.toolCallId, undefined, result.error);

        // ========== AFTER_TOOL_CALL Hook (with error) ==========
        await executeAgentHook(entity, "AFTER_TOOL_CALL", this.emitAgentEvent, {
          ...toolCallInfo,
          error: result.error,
        });

        yield {
          type: AgentStreamEventType.TOOL_EXECUTION_END,
          timestamp: Date.now(),
          agentLoopId,
          toolCallId: result.toolCallId,
          toolName: toolCallInfo.name,
          result: {
            success: false,
            error: result.error,
            executionTime: result.executionTime,
            retryCount: 0,
          },
          duration: result.executionTime,
        };

        // Also emit to EventRegistry
        if (this.eventManager) {
          const registryEvent = this.convertToRegistryEvent(
            {
              type: AgentStreamEventType.TOOL_EXECUTION_END,
              timestamp: Date.now(),
              agentLoopId,
              toolCallId: result.toolCallId,
              toolName: toolCallInfo.name,
              result: {
                success: false,
                error: result.error,
                executionTime: result.executionTime,
                retryCount: 0,
              },
              duration: result.executionTime,
            },
            entity,
          );
          if (registryEvent) {
            await this.eventManager.emit(registryEvent);
          }
        }
      }
    }
  }
}
