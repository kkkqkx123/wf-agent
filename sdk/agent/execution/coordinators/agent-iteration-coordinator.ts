/**
 * AgentIterationCoordinator - Agent Iteration Coordinator
 *
 * Coordinates the execution of a single agent loop iteration.
 * Handles LLM calls, response processing, tool execution delegation, event emission,
 * and hook lifecycle within one iteration.
 *
 * Design Principles:
 * - Single responsibility: only handles one iteration execution
 * - Delegates to specialized coordinators (CoreLLMExecutionCoordinator, ToolExecutionCoordinator)
 * - Stateless design, all state managed through AgentLoopEntity
 * - Supports both sync and streaming execution modes
 *
 * This is the agent-side counterpart to workflow's LLMExecutionCoordinator,
 * handling the single iteration flow: hooks -> LLM call -> response -> tool execution.
 */

import type {
  LLMResult,
  LLMUsage,
  AgentHookTriggeredEvent,
  AgentStreamEvent,
  MessageStreamEvent,
  ToolSchema,
  LLMMessage,
  Event as RegistryEvent,
} from "@wf-agent/types";
import type { AgentLoopEntity } from "../../entities/agent-loop-entity.js";
import type { ConversationSession } from "../../../core/messaging/conversation-session.js";
import type { EventRegistry } from "../../../core/registry/event-registry.js";
import type { MessageStream } from "../../../core/llm/message-stream.js";
import type { LLMExecutionCoordinator as CoreLLMExecutionCoordinator } from "../../../core/coordinators/llm-execution-coordinator.js";
import {
  checkAgentInterruption,
  getAgentInterruptionDescription,
} from "../utils/agent-interruption-utils.js";
import { executeAgentHook } from "../handlers/hook-handlers/index.js";
import { handleAgentError } from "../handlers/agent-error-handler.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import type { AgentStateCoordinator } from "../../state-managers/agent-state-coordinator.js";
import {
  buildAgentStartedEvent,
  buildAgentCompletedEvent,
  buildAgentIterationCompletedEvent,
  buildAgentToolExecutionStartedEvent,
  buildAgentToolExecutionCompletedEvent,
  buildMessageAddedEvent,
} from "../../../core/utils/event/builders/index.js";
import { ToolExecutionCoordinator } from "./tool-execution-coordinator.js";

/**
 * Agent Loop Stream Event (union of message-level and agent-level stream events)
 */
export type AgentLoopStreamEvent = MessageStreamEvent | AgentStreamEvent;

const logger = createContextualLogger({ component: "AgentIterationCoordinator" });

/**
 * AgentIterationCoordinator Dependencies
 */
export interface AgentIterationCoordinatorDependencies {
  /** Core LLM Execution Coordinator (unified LLM execution with transformContext support) */
  coreCoordinator: CoreLLMExecutionCoordinator;
  /** Tool Execution Coordinator */
  toolExecutionCoordinator: ToolExecutionCoordinator;
  /** Event emitter for agent hooks */
  emitAgentEvent: (event: AgentHookTriggeredEvent) => Promise<void>;
  /** Event Registry (optional) */
  eventManager?: EventRegistry;
}

/**
 * AgentIterationCoordinator
 *
 * Coordinates a single iteration of the agent loop:
 * - Manages hook lifecycle (BEFORE_ITERATION, BEFORE_LLM_CALL, AFTER_LLM_CALL, AFTER_ITERATION)
 * - Executes LLM calls via CoreLLMExecutionCoordinator
 * - Processes LLM responses (final answer vs tool calls)
 * - Delegates tool execution to ToolExecutionCoordinator
 * - Handles interruption checks within iteration
 * - Emits iteration-scoped events
 */
export class AgentIterationCoordinator {
  private readonly coreCoordinator: CoreLLMExecutionCoordinator;
  private readonly toolExecutionCoordinator: ToolExecutionCoordinator;
  private readonly emitAgentEvent: (event: AgentHookTriggeredEvent) => Promise<void>;
  private readonly eventManager?: EventRegistry;
  private readonly stateCoordinator: AgentStateCoordinator;

  constructor(deps: AgentIterationCoordinatorDependencies & { stateCoordinator: AgentStateCoordinator }) {
    this.coreCoordinator = deps.coreCoordinator;
    this.toolExecutionCoordinator = deps.toolExecutionCoordinator;
    this.emitAgentEvent = deps.emitAgentEvent;
    this.eventManager = deps.eventManager;
    this.stateCoordinator = deps.stateCoordinator;
  }

  /**
   * Execute a single iteration (sync mode)
   */
  async executeIteration(
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

    await executeAgentHook(entity, "BEFORE_ITERATION", this.emitAgentEvent, this.stateCoordinator);
    entity.state.startIteration();

    const iterationStartEvent = this.createIterationStartEvent(
      agentLoopId,
      entity.state.currentIteration,
    );
    await this.emitToRegistry(iterationStartEvent, entity);

    await executeAgentHook(entity, "BEFORE_LLM_CALL", this.emitAgentEvent, this.stateCoordinator);

    const preCheck = checkAgentInterruption(abortSignal, entity.state.currentIteration);
    if (preCheck.type === "paused" || preCheck.type === "stopped") {
      logger.info("Agent iteration interrupted before LLM call", {
        agentLoopId,
        iteration: entity.state.currentIteration,
        interruptionType: preCheck.type,
      });
      return {
        success: false,
        shouldContinue: false,
        interruption: getAgentInterruptionDescription(preCheck),
      };
    }

    logger.debug("Calling LLM", {
      agentLoopId,
      iteration: entity.state.currentIteration,
      messageCount: conversationManager.getMessageCount(),
    });

    const llmResult = await this.coreCoordinator.executeLLMCallWithMessages(
      conversationManager.getMessages(),
      { profileId, parameters: {}, tools: toolSchemas },
      { abortSignal, executionId: entity.id, nodeId: entity.nodeId },
      entity.config.transformContext,
    );

    logger.debug("LLM call completed", {
      agentLoopId,
      iteration: entity.state.currentIteration,
      hasToolCalls: !!llmResult.toolCalls?.length,
    });

    const postCheck = checkAgentInterruption(abortSignal, entity.state.currentIteration);
    if (postCheck.type === "paused" || postCheck.type === "stopped") {
      logger.info("Agent iteration interrupted after LLM call", {
        agentLoopId,
        iteration: entity.state.currentIteration,
        interruptionType: postCheck.type,
      });
      return {
        success: false,
        shouldContinue: false,
        interruption: getAgentInterruptionDescription(postCheck),
      };
    }

    const response = llmResult;

    await executeAgentHook(entity, "AFTER_LLM_CALL", this.emitAgentEvent, this.stateCoordinator, undefined, {
      content: response.content,
      toolCalls: response.toolCalls,
    });

    // Track token usage from LLM response (capability gap closure)
    if (response.usage) {
      conversationManager.updateTokenUsage(response.usage as LLMUsage);
      conversationManager.finalizeCurrentRequest();
    }

    const toolCalls = response.toolCalls?.map(tc => ({
      id: tc.id,
      type: "function" as const,
      function: { name: tc.name, arguments: tc.arguments },
    }));

    conversationManager.addAssistantMessage(response.content, toolCalls);

    // Emit message-added event for observability (unified with workflow behavior)
    await this.emitMessageEvent(entity, response.content, "assistant");

    if (!response.toolCalls || response.toolCalls.length === 0) {
      logger.debug("No tool calls required, completing execution", {
        agentLoopId,
        iteration: entity.state.currentIteration,
        contentLength: response.content.length,
      });

      entity.state.endIteration(response.content);
      await executeAgentHook(entity, "AFTER_ITERATION", this.emitAgentEvent, this.stateCoordinator);
      entity.state.complete();

      return { success: true, shouldContinue: false, content: response.content };
    }

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
    await executeAgentHook(entity, "AFTER_ITERATION", this.emitAgentEvent, this.stateCoordinator);

    return { success: true, shouldContinue: true, content: response.content };
  }

  /**
   * Execute a single iteration (stream mode)
   */
  async *executeIterationStream(
    entity: AgentLoopEntity,
    conversationManager: ConversationSession,
    toolSchemas: ToolSchema[] | undefined,
    profileId: string,
  ): AsyncGenerator<AgentLoopStreamEvent, boolean> {
    const agentLoopId = entity.id;

    await executeAgentHook(entity, "BEFORE_ITERATION", this.emitAgentEvent, this.stateCoordinator);
    entity.state.startIteration();

    yield this.createIterationStartEvent(agentLoopId, entity.state.currentIteration);
    await this.emitToRegistry(
      this.createIterationStartEvent(agentLoopId, entity.state.currentIteration),
      entity,
    );

    await executeAgentHook(entity, "BEFORE_LLM_CALL", this.emitAgentEvent, this.stateCoordinator);

    logger.debug("Calling LLM for stream", {
      agentLoopId,
      iteration: entity.state.currentIteration,
      messageCount: conversationManager.getMessageCount(),
    });

    let messageStream: MessageStream;
    try {
      messageStream = await this.coreCoordinator.executeLLMStream(
        {
          contextId: entity.id,
          prompt: "",
          config: { profileId, parameters: {} },
          tools: toolSchemas,
          abortSignal: entity.getAbortSignal(),
          transformContext: entity.config.transformContext,
        },
        conversationManager,
      );
    } catch (error) {
      return yield* this.handleStreamLLMError(entity, agentLoopId, error as Error);
    }

    const result = yield* this.processMessageStream(entity, agentLoopId, messageStream);

    if (!result.success) {
      return false;
    }

    const finalResult = result.finalResult!;

    await executeAgentHook(entity, "AFTER_LLM_CALL", this.emitAgentEvent, this.stateCoordinator, undefined, {
      content: finalResult.content,
      toolCalls: finalResult.toolCalls,
    });

    // Track token usage from stream LLM response
    if (finalResult.usage) {
      conversationManager.updateTokenUsage(finalResult.usage as LLMUsage);
      conversationManager.finalizeCurrentRequest();
    }

    const toolCalls = finalResult.toolCalls?.map(tc => ({
      id: tc.id,
      type: "function" as const,
      function: { name: tc.function.name, arguments: tc.function.arguments as string },
    }));

    conversationManager.addAssistantMessage(finalResult.content, toolCalls);
    await this.emitMessageEvent(entity, finalResult.content, "assistant");

    if (!finalResult.toolCalls || finalResult.toolCalls.length === 0) {
      logger.debug("No tool calls required, completing stream iteration", {
        agentLoopId,
        iteration: entity.state.currentIteration,
        contentLength: finalResult.content.length,
      });

      yield this.createIterationCompleteEvent(agentLoopId, entity.state.currentIteration, false);
      await this.emitToRegistry(
        this.createIterationCompleteEvent(agentLoopId, entity.state.currentIteration, false),
        entity,
      );

      entity.state.endIteration(finalResult.content);
      await executeAgentHook(entity, "AFTER_ITERATION", this.emitAgentEvent, this.stateCoordinator);

      return false;
    }

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
    await executeAgentHook(entity, "AFTER_ITERATION", this.emitAgentEvent, this.stateCoordinator);

    return true;
  }

  /**
   * Emit a stream event to the EventRegistry
   */
  async emitToRegistry(event: AgentLoopStreamEvent, entity: AgentLoopEntity): Promise<void> {
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
      messageStream.cleanup();
    }
  }

  /**
   * Handle stream LLM error
   */
  private async *handleStreamLLMError(
    entity: AgentLoopEntity,
    agentLoopId: string,
    error: Error,
  ): AsyncGenerator<AgentLoopStreamEvent, boolean> {
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
   */
  private async *handleStreamError(
    entity: AgentLoopEntity,
    agentLoopId: string,
    error: Error,
  ): AsyncGenerator<AgentLoopStreamEvent, { success: boolean }> {
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

  private createIterationStartEvent(agentLoopId: string, iteration: number): AgentStreamEvent {
    return {
      type: "iteration_start",
      timestamp: Date.now(),
      agentLoopId,
      iteration,
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

  /**
   * Emit a message-added event to the EventRegistry
   * Unifies agent event emission with workflow behavior for observability.
   */
  private async emitMessageEvent(
    entity: AgentLoopEntity,
    content: string,
    role: string,
  ): Promise<void> {
    if (!this.eventManager) return;
    try {
      const event = buildMessageAddedEvent({
        executionId: entity.id,
        role,
        content,
        nodeId: entity.nodeId,
      });
      await this.eventManager.emit(event);
    } catch (error) {
      logger.warn("Failed to emit message added event", {
        agentLoopId: entity.id,
        role,
        error,
      });
    }
  }

  // ============ Event Registry Helpers ============

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
