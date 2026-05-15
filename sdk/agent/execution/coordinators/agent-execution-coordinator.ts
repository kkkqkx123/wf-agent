/**
 * AgentExecutionCoordinator - Agent Execution Coordinator
 *
 * Coordinates the execution flow of an agent loop and orchestrates the execution of each component.
 * Consolidates loop logic from AgentLoopExecutor and AgentStreamExecutor.
 *
 * Design Principles:
 * - Coordinator pattern: orchestrates execution flow
 * - Stateless design, all state managed through AgentLoopEntity
 * - Delegates to specialized coordinators (ToolExecutionCoordinator)
 * - Directly uses LLMExecutor for LLM calls (agent-specific message management via hooks)
 * - Supports both sync and streaming execution modes
 * - Integrates with Hook mechanism
 */

import type {
  AgentLoopResult,
  AgentHookTriggeredEvent,
  AgentStreamEvent,
  MessageStreamEvent,
  ToolSchema,
  LLMMessage,
  Event as RegistryEvent,
  LLMResult,
} from "@wf-agent/types";
import type { AgentLoopEntity } from "../../entities/agent-loop-entity.js";
import type { ConversationSession } from "../../../core/messaging/conversation-session.js";
import type { LLMExecutor } from "../../../core/executors/llm-executor.js";
import type { EventRegistry } from "../../../core/registry/event-registry.js";
import type { MessageStream } from "../../../core/llm/message-stream.js";
import type { MetricsRegistry } from "../../../core/metrics/metrics-registry.js";
import {
  executeWithInterruptionHandling,
  iterateWithInterruptionHandling,
} from "../../../core/utils/interruption/index.js";
import { executeAgentHook } from "../handlers/hook-handlers/index.js";
import { handleAgentError } from "../handlers/agent-error-handler.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import {
  buildAgentStartedEvent,
  buildAgentCompletedEvent,
  buildAgentIterationCompletedEvent,
  buildAgentToolExecutionStartedEvent,
  buildAgentToolExecutionCompletedEvent,
} from "../../../core/utils/event/builders/agent-events.js";
import { ToolExecutionCoordinator } from "./tool-execution-coordinator.js";

const logger = createContextualLogger({ component: "AgentExecutionCoordinator" });

/**
 * Agent Loop Stream Event
 */
export type AgentLoopStreamEvent = MessageStreamEvent | AgentStreamEvent;

/**
 * AgentExecutionCoordinator Dependencies
 */
export interface AgentExecutionCoordinatorDependencies {
  /** LLM Executor */
  llmExecutor: LLMExecutor;
  /** Tool Execution Coordinator */
  toolExecutionCoordinator: ToolExecutionCoordinator;
  /** Event emitter for agent events */
  emitAgentEvent: (event: AgentHookTriggeredEvent) => Promise<void>;
  /** Event Registry (optional) */
  eventManager?: EventRegistry;
  /** Metrics Registry (optional) */
  metricsRegistry?: MetricsRegistry;
}

/**
 * AgentExecutionCoordinator
 *
 * Coordinates the execution flow of agent loop:
 * - Manages iteration loop control
 * - Handles interruption signals (pause/stop/abort)
 * - Uses LLMExecutor directly for LLM calls
 * - Delegates to ToolExecutionCoordinator for tool execution
 * - Supports streaming with real-time event forwarding
 */
export class AgentExecutionCoordinator {
  private readonly llmExecutor: LLMExecutor;
  private readonly toolExecutionCoordinator: ToolExecutionCoordinator;
  private readonly emitAgentEvent: (event: AgentHookTriggeredEvent) => Promise<void>;
  private readonly eventManager?: EventRegistry;
  private readonly metricsRegistry?: MetricsRegistry;

  constructor(deps: AgentExecutionCoordinatorDependencies) {
    this.llmExecutor = deps.llmExecutor;
    this.toolExecutionCoordinator = deps.toolExecutionCoordinator;
    this.emitAgentEvent = deps.emitAgentEvent;
    this.eventManager = deps.eventManager;
    this.metricsRegistry = deps.metricsRegistry;
  }

  /**
   * Execute agent loop (synchronous mode)
   *
   * @param entity Agent loop entity
   * @param conversationManager Conversation manager
   * @param toolSchemas Tool schemas for LLM
   * @param profileId LLM profile ID
   * @param maxIterations Maximum iterations
   * @returns Execution result
   */
  async execute(
    entity: AgentLoopEntity,
    conversationManager: ConversationSession,
    toolSchemas: ToolSchema[] | undefined,
    profileId: string,
    maxIterations: number,
  ): Promise<AgentLoopResult> {
    const agentLoopId = entity.id;
    const startTime = Date.now();

    // Record agent loop execution start in metrics
    if (this.metricsRegistry) {
      this.metricsRegistry.getCollectors().agent.recordExecutionStart(
        profileId,
        entity.config.agentConfigId || 'unknown',
        agentLoopId
      );
    }

    try {
      // Use unified interruption handling wrapper for the entire execution loop
      const result = await executeWithInterruptionHandling(
        async (signal) => {
          while (entity.state.currentIteration < maxIterations) {
            logger.debug("Starting new iteration", {
              agentLoopId,
              iteration: entity.state.currentIteration + 1,
              maxIterations,
            });

            const iterationResult = await this.executeIteration(
              entity,
              conversationManager,
              toolSchemas,
              profileId,
              signal, // Pass abort signal to iteration
            );

            if (iterationResult.interruption) {
              return {
                success: false,
                iterations: entity.state.currentIteration,
                toolCallCount: entity.state.toolCallCount,
                error: `Execution ${iterationResult.interruption}`,
              };
            }

            if (!iterationResult.shouldContinue) {
              logger.info("Agent Loop execution completed successfully", {
                agentLoopId,
                iterations: entity.state.currentIteration,
                toolCallCount: entity.state.toolCallCount,
              });
              
              // Record agent loop completion in metrics
              if (this.metricsRegistry) {
                const duration = Date.now() - startTime;
                this.metricsRegistry.getCollectors().agent.recordExecutionComplete(
                  profileId,
                  {
                    iterations: entity.state.currentIteration,
                    toolCallCount: entity.state.toolCallCount,
                    duration,
                    success: true,
                  }
                );
              }
              
              return {
                success: true,
                content: iterationResult.content,
                iterations: entity.state.currentIteration,
                toolCallCount: entity.state.toolCallCount,
              };
            }

            logger.debug("Iteration completed, continuing", {
              agentLoopId,
              iteration: entity.state.currentIteration,
            });
          }

          logger.info("Agent Loop reached maximum iterations", {
            agentLoopId,
            maxIterations,
            toolCallCount: entity.state.toolCallCount,
          });

          entity.state.complete();
          
          // Record agent loop completion in metrics (max iterations reached)
          if (this.metricsRegistry) {
            const duration = Date.now() - startTime;
            this.metricsRegistry.getCollectors().agent.recordExecutionComplete(
              profileId,
              {
                iterations: entity.state.currentIteration,
                toolCallCount: entity.state.toolCallCount,
                duration,
                success: true,
              }
            );
          }
          
          return {
            success: true,
            iterations: entity.state.currentIteration,
            toolCallCount: entity.state.toolCallCount,
            content: "Reached maximum iterations without final answer.",
          };
        },
        entity.getAbortSignal(),
      );

      // Handle interruption gracefully if needed
      if (!result.success) {
        const interruption = result.interruption;
        const type = interruption.type === "paused" ? "PAUSE" : "STOP";
        
        // Update entity status based on interruption type
        if (type === "PAUSE") {
          entity.state.pause();
        } else {
          entity.state.cancel();
        }
        
        // Record agent loop completion in metrics (interrupted)
        if (this.metricsRegistry) {
          const duration = Date.now() - startTime;
          this.metricsRegistry.getCollectors().agent.recordExecutionComplete(
            profileId,
            {
              iterations: entity.state.currentIteration,
              toolCallCount: entity.state.toolCallCount,
              duration,
              success: false,
            }
          );
        }
        
        return {
          success: false,
          iterations: entity.state.currentIteration,
          toolCallCount: entity.state.toolCallCount,
          error: `Execution ${interruption.type}`,
        };
      }

      return result.result;
    } catch (error) {
      const standardizedError = await handleAgentError(
        entity,
        error as Error,
        "agent_loop_execution",
        undefined,
        this.eventManager,
      );

      // Record agent loop failure in metrics
      if (this.metricsRegistry) {
        const duration = Date.now() - startTime;
        this.metricsRegistry.getCollectors().agent.recordExecutionComplete(
          profileId,
          {
            iterations: entity.state.currentIteration,
            toolCallCount: entity.state.toolCallCount,
            duration,
            success: false,
          }
        );
      }

      return {
        success: false,
        iterations: entity.state.currentIteration,
        toolCallCount: entity.state.toolCallCount,
        error: standardizedError,
      };
    }
  }

  /**
   * Execute agent loop (streaming mode)
   *
   * @param entity Agent loop entity
   * @param conversationManager Conversation manager
   * @param toolSchemas Tool schemas for LLM
   * @param profileId LLM profile ID
   * @param maxIterations Maximum iterations
   * @returns Stream event generator
   */
  async *executeStream(
    entity: AgentLoopEntity,
    conversationManager: ConversationSession,
    toolSchemas: ToolSchema[] | undefined,
    profileId: string,
    maxIterations: number,
  ): AsyncGenerator<AgentLoopStreamEvent> {
    const agentLoopId = entity.id;

    yield this.createAgentStartEvent(
      agentLoopId,
      maxIterations,
      conversationManager.getMessageCount(),
    );
    await this.emitToRegistry(
      this.createAgentStartEvent(agentLoopId, maxIterations, conversationManager.getMessageCount()),
      entity,
    );

    try {
      // Create main loop iterator
      const mainLoop = this.createMainLoopIterator(
        entity,
        conversationManager,
        toolSchemas,
        profileId,
        maxIterations,
      );

      // Use unified interruption handling iterator
      for await (const item of iterateWithInterruptionHandling(mainLoop, entity.getAbortSignal())) {
        if (item.type === "interrupted") {
          // Handle interruption
          const interruption = item.interruption;
          const type = interruption.type === "paused" ? "PAUSE" : "STOP";
          
          if (type === "PAUSE") {
            entity.state.pause();
          } else {
            entity.state.cancel();
          }
          
          yield this.createErrorEvent(
            agentLoopId,
            `Execution ${interruption.type}`,
            entity.state.currentIteration,
            "stream_interruption",
          );
          await this.emitToRegistry(
            this.createErrorEvent(
              agentLoopId,
              `Execution ${interruption.type}`,
              entity.state.currentIteration,
              "stream_interruption",
            ),
            entity,
          );
          return;
        }

        // Yield the event
        yield item.value;
      }
    } catch (error) {
      const standardizedError = await handleAgentError(
        entity,
        error as Error,
        "agent_loop_stream_execution",
      );

      yield this.createErrorEvent(
        agentLoopId,
        standardizedError.message,
        entity.state.currentIteration,
        "agent_loop_stream_execution",
      );
      await this.emitToRegistry(
        this.createErrorEvent(
          agentLoopId,
          standardizedError.message,
          entity.state.currentIteration,
          "agent_loop_stream_execution",
        ),
        entity,
      );
    }
  }

  /**
   * Create main loop iterator for streaming execution
   */
  private async *createMainLoopIterator(
    entity: AgentLoopEntity,
    conversationManager: ConversationSession,
    toolSchemas: ToolSchema[] | undefined,
    profileId: string,
    maxIterations: number,
  ): AsyncGenerator<AgentLoopStreamEvent> {
    const agentLoopId = entity.id;

    while (entity.state.currentIteration < maxIterations) {
      logger.debug("Starting new stream iteration", {
        agentLoopId,
        iteration: entity.state.currentIteration + 1,
        maxIterations,
      });

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

    logger.info("Agent Loop stream reached maximum iterations", {
      agentLoopId,
      maxIterations,
      toolCallCount: entity.state.toolCallCount,
    });

    entity.state.complete();
    yield this.createAgentEndEvent(
      agentLoopId,
      conversationManager.getMessages(),
      entity.state.currentIteration,
      entity.state.toolCallCount,
      true,
    );
  }

  /**
   * Execute a single iteration (sync mode)
   */
  private async executeIteration(
    entity: AgentLoopEntity,
    conversationManager: ConversationSession,
    toolSchemas: ToolSchema[] | undefined,
    profileId: string,
    abortSignal?: AbortSignal,
  ): Promise<{
    success: boolean;
    shouldContinue: boolean;
    content?: string;
    interruption?: string;
  }> {
    const agentLoopId = entity.id;

    await executeAgentHook(entity, "BEFORE_ITERATION", this.emitAgentEvent);
    entity.state.startIteration();
    
    // Emit ITERATION_START event
    const iterationStartEvent = this.createIterationStartEvent(agentLoopId, entity.state.currentIteration);
    await this.emitToRegistry(iterationStartEvent, entity);
    
    await executeAgentHook(entity, "BEFORE_LLM_CALL", this.emitAgentEvent);

    logger.debug("Calling LLM", {
      agentLoopId,
      iteration: entity.state.currentIteration,
      messageCount: conversationManager.getMessageCount(),
    });

    // LLM Executor now throws errors directly (including AbortError)
    // The outer executeWithInterruptionHandling wrapper will catch and handle interruptions
    const llmResult = await this.llmExecutor.executeLLMCall(
      conversationManager.getMessages(),
      { prompt: "", profileId, parameters: {}, tools: toolSchemas, stream: false },
      { abortSignal, executionId: entity.id, nodeId: entity.nodeId },
    );

    logger.debug("LLM call completed", {
      agentLoopId,
      iteration: entity.state.currentIteration,
      hasToolCalls: !!llmResult.toolCalls?.length,
    });

    // LLM Executor now returns result directly (no success/interruption wrapper)
    const response = llmResult;

    await executeAgentHook(entity, "AFTER_LLM_CALL", this.emitAgentEvent, undefined, {
      content: response.content,
      toolCalls: response.toolCalls,
    });

    const toolCalls = response.toolCalls?.map(tc => ({
      id: tc.id,
      type: "function" as const,
      function: { name: tc.name, arguments: tc.arguments },
    }));

    conversationManager.addAssistantMessage(response.content, toolCalls);
    entity.addMessage({ role: "assistant", content: response.content, toolCalls });

    if (!response.toolCalls || response.toolCalls.length === 0) {
      logger.debug("No tool calls required, completing execution", {
        agentLoopId,
        iteration: entity.state.currentIteration,
        contentLength: response.content.length,
      });

      entity.state.endIteration(response.content);
      await executeAgentHook(entity, "AFTER_ITERATION", this.emitAgentEvent);
      entity.state.complete();

      return { success: true, shouldContinue: false, content: response.content };
    }

    // Execute tool calls using ToolExecutionCoordinator
    await this.toolExecutionCoordinator.executeToolCalls(
      entity,
      conversationManager,
      response.toolCalls.map(tc => ({
        id: tc.id,
        name: tc.name,
        arguments: tc.arguments,
      })),
    );
    entity.state.endIteration(response.content);
    await executeAgentHook(entity, "AFTER_ITERATION", this.emitAgentEvent);

    return { success: true, shouldContinue: true, content: response.content };
  }

  /**
   * Execute a single iteration (stream mode)
   */
  private async *executeIterationStream(
    entity: AgentLoopEntity,
    conversationManager: ConversationSession,
    toolSchemas: ToolSchema[] | undefined,
    profileId: string,
  ): AsyncGenerator<AgentLoopStreamEvent, boolean> {
    const agentLoopId = entity.id;

    await executeAgentHook(entity, "BEFORE_ITERATION", this.emitAgentEvent);
    entity.state.startIteration();
    
    // Emit ITERATION_START event
    yield this.createIterationStartEvent(agentLoopId, entity.state.currentIteration);
    await this.emitToRegistry(
      this.createIterationStartEvent(agentLoopId, entity.state.currentIteration),
      entity,
    );
    
    await executeAgentHook(entity, "BEFORE_LLM_CALL", this.emitAgentEvent);

    logger.debug("Calling LLM for stream", {
      agentLoopId,
      iteration: entity.state.currentIteration,
      messageCount: conversationManager.getMessageCount(),
    });

    const llmWrapperResult = await this.llmExecutor["llmWrapper"].generateStream({
      profileId,
      messages: conversationManager.getMessages(),
      tools: toolSchemas,
      stream: true,
      signal: entity.getAbortSignal(),
    });

    if (llmWrapperResult.isErr()) {
      const error = llmWrapperResult.error;
      return yield* this.handleStreamLLMError(entity, agentLoopId, error);
    }

    const messageStream = llmWrapperResult.value;
    const streamResult = yield* this.processMessageStream(entity, agentLoopId, messageStream);

    if (!streamResult.success) {
      return false;
    }

    const finalResult = streamResult.finalResult!;

    await executeAgentHook(entity, "AFTER_LLM_CALL", this.emitAgentEvent, undefined, {
      content: finalResult.content,
      toolCalls: finalResult.toolCalls,
    });

    const assistantMessage: LLMMessage = {
      role: "assistant",
      content: finalResult.content,
      toolCalls: finalResult.toolCalls?.map(
        (tc: { id: string; function: { name: string; arguments: string } }) => ({
          id: tc.id,
          type: "function" as const,
          function: { name: tc.function.name, arguments: tc.function.arguments },
        }),
      ),
    };
    conversationManager.addAssistantMessage(
      finalResult.content,
      finalResult.toolCalls?.map(
        (tc: { id: string; function: { name: string; arguments: string } }) => ({
          id: tc.id,
          type: "function" as const,
          function: { name: tc.function.name, arguments: tc.function.arguments },
        }),
      ),
    );
    entity.addMessage(assistantMessage);

    if (!finalResult.toolCalls || finalResult.toolCalls.length === 0) {
      logger.debug("No tool calls required in stream, completing execution", {
        agentLoopId,
        iteration: entity.state.currentIteration,
        contentLength: finalResult.content.length,
      });

      entity.state.endIteration(finalResult.content);
      await executeAgentHook(entity, "AFTER_ITERATION", this.emitAgentEvent);
      entity.state.complete();

      yield this.createAgentEndEvent(
        agentLoopId,
        conversationManager.getMessages(),
        entity.state.currentIteration,
        entity.state.toolCallCount,
        true,
      );
      await this.emitToRegistry(
        this.createAgentEndEvent(
          agentLoopId,
          conversationManager.getMessages(),
          entity.state.currentIteration,
          entity.state.toolCallCount,
          true,
        ),
        entity,
      );
      return false;
    }

    // Execute tool calls using ToolExecutionCoordinator (stream mode)
    yield* this.toolExecutionCoordinator.executeToolCallsStream(
      entity,
      conversationManager,
      finalResult.toolCalls,
    );

    yield this.createIterationCompleteEvent(agentLoopId, entity.state.currentIteration, true);
    await this.emitToRegistry(
      this.createIterationCompleteEvent(agentLoopId, entity.state.currentIteration, true),
      entity,
    );

    logger.debug("Stream iteration completed", {
      agentLoopId,
      iteration: entity.state.currentIteration,
    });

    entity.state.endIteration(finalResult.content);
    await executeAgentHook(entity, "AFTER_ITERATION", this.emitAgentEvent);

    return true;
  }

  /**
   * Process message stream with real-time event forwarding
   */
  private async *processMessageStream(
    entity: AgentLoopEntity,
    agentLoopId: string,
    messageStream: MessageStream,
  ): AsyncGenerator<AgentLoopStreamEvent, { success: boolean; finalResult?: LLMResult }> {
    const eventQueue: MessageStreamEvent[] = [];
    let streamDone = false;
    let streamError: Error | null = null;

    try {
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

      messageStream
        .done()
        .then(() => {
          streamDone = true;
        })
        .catch(error => {
          streamError = error;
          streamDone = true;
        });

      // Wait for stream completion - interruption is handled by outer iterateWithInterruptionHandling
      while (!streamDone || eventQueue.length > 0) {
        while (eventQueue.length > 0) {
          const event = eventQueue.shift()!;
          yield event;
        }

        if (!streamDone) {
          await new Promise(resolve => setTimeout(resolve, 0));
        }
      }

      if (streamError) {
        return yield* this.handleStreamError(entity, agentLoopId, streamError);
      }

      const finalResult = await messageStream.getFinalResult();
      return { success: true, finalResult };
    } finally {
      // Cleanup message stream listeners to prevent memory leaks
      messageStream.cleanup();
    }
  }

  /**
   * Handle stream LLM error
   *
   * Note: Interruption handling is done by outer iterateWithInterruptionHandling.
   * This method only handles actual errors (non-abort errors).
   */
  private async *handleStreamLLMError(
    entity: AgentLoopEntity,
    agentLoopId: string,
    error: Error,
  ): AsyncGenerator<AgentLoopStreamEvent, boolean> {
    // Let outer iterateWithInterruptionHandling handle abort/interruption
    // Only process actual errors here
    const standardizedError = await handleAgentError(
      entity,
      error,
      "llm_stream_call",
      undefined,
      this.eventManager,
    );
    
    yield this.createErrorEvent(
      agentLoopId,
      standardizedError.message,
      entity.state.currentIteration,
      "llm_stream_call",
    );
    await this.emitToRegistry(
      this.createErrorEvent(
        agentLoopId,
        standardizedError.message,
        entity.state.currentIteration,
        "llm_stream_call",
      ),
      entity,
    );
    return false;
  }

  /**
   * Handle stream error
   *
   * Note: Interruption handling is done by outer iterateWithInterruptionHandling.
   * This method only handles actual errors (non-abort errors).
   */
  private async *handleStreamError(
    entity: AgentLoopEntity,
    agentLoopId: string,
    error: Error,
  ): AsyncGenerator<AgentLoopStreamEvent, { success: boolean }> {
    // Let outer iterateWithInterruptionHandling handle abort/interruption
    // Only process actual errors here
    const standardizedError = await handleAgentError(
      entity,
      error,
      "message_stream_done",
      undefined,
      this.eventManager,
    );
    
    yield this.createErrorEvent(
      agentLoopId,
      standardizedError.message,
      entity.state.currentIteration,
      "message_stream_done",
    );
    await this.emitToRegistry(
      this.createErrorEvent(
        agentLoopId,
        standardizedError.message,
        entity.state.currentIteration,
        "message_stream_done",
      ),
      entity,
    );
    return { success: false };
  }

  // ============ Event Factory Methods ============

  private createAgentStartEvent(
    agentLoopId: string,
    maxIterations: number,
    initialMessageCount: number,
  ): AgentStreamEvent {
    return {
      type: "agent_start",
      timestamp: Date.now(),
      agentLoopId,
      maxIterations,
      initialMessageCount,
    };
  }

  private createAgentEndEvent(
    agentLoopId: string,
    messages: LLMMessage[],
    iterations: number,
    toolCallCount: number,
    success: boolean,
  ): AgentStreamEvent {
    return {
      type: "agent_end",
      timestamp: Date.now(),
      agentLoopId,
      messages,
      iterations,
      toolCallCount,
      success,
    };
  }

  private createIterationCompleteEvent(
    agentLoopId: string,
    iteration: number,
    shouldContinue: boolean,
  ): AgentStreamEvent {
    return {
      type: "iteration_complete",
      timestamp: Date.now(),
      agentLoopId,
      iteration,
      shouldContinue,
    };
  }

  private createIterationStartEvent(agentLoopId: string, iteration: number): AgentStreamEvent {
    return {
      type: "iteration_start",
      timestamp: Date.now(),
      agentLoopId,
      iteration,
    };
  }

  private createErrorEvent(
    agentLoopId: string,
    error: string,
    iteration: number,
    context: string,
  ): AgentStreamEvent {
    return {
      type: "agent_error",
      timestamp: Date.now(),
      agentLoopId,
      error,
      iteration,
      context,
    };
  }

  // ============ Event Registry Helpers ============

  private async emitToRegistry(
    event: AgentLoopStreamEvent,
    entity: AgentLoopEntity,
  ): Promise<void> {
    if (!this.eventManager) return;

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

  private convertToRegistryEvent(
    event: AgentLoopStreamEvent,
    entity: AgentLoopEntity,
  ): RegistryEvent | null {
    const baseData = {
      id: crypto.randomUUID(),
      timestamp: Date.now(),
      workflowId: entity.id,
      executionId: entity.id,
      agentLoopId: entity.id,
      nodeId: entity.nodeId,
      metadata: {},
    };

    switch (event.type) {
      case "agent_start":
        return buildAgentStartedEvent({
          ...baseData,
          maxIterations: event.maxIterations,
          initialMessageCount: event.initialMessageCount,
        });
      case "agent_end":
        return buildAgentCompletedEvent({
          ...baseData,
          iterations: event.iterations,
          toolCallCount: event.toolCallCount,
          success: event.success,
          error: event.success ? undefined : event.error,
        });
      case "iteration_complete":
        return buildAgentIterationCompletedEvent({
          ...baseData,
          iteration: event.iteration,
          toolCallCount: 0,
          shouldContinue: event.shouldContinue,
        });
      case "tool_execution_start":
        return buildAgentToolExecutionStartedEvent({
          ...baseData,
          toolCallId: event.toolCallId,
          toolName: event.toolName,
          iteration: event.iteration,
        });
      case "tool_execution_end":
        return buildAgentToolExecutionCompletedEvent({
          ...baseData,
          toolCallId: event.toolCallId,
          toolName: event.toolName,
          success: event.result.success,
          duration: event.duration,
          error: event.result.success ? undefined : event.result.error,
          iteration: entity.state.currentIteration,
        });
      case "agent_error":
        return null;
      default:
        return null;
    }
  }
}
