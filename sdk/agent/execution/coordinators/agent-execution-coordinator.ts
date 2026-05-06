/**
 * AgentExecutionCoordinator - Agent Execution Coordinator
 *
 * Coordinates the execution flow of an agent loop and orchestrates the execution of each component.
 * Consolidates loop logic from AgentLoopExecutor and AgentStreamExecutor.
 *
 * Design Principles:
 * - Coordinator pattern: orchestrates execution flow
 * - Stateless design, all state managed through AgentLoopEntity
 * - Reuses AgentIterationExecutor for single iteration execution
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
  ToolApprovalOptions,
  ToolApprovalHandler,
  LLMToolCall,
} from "@wf-agent/types";
import { AgentStreamEventType } from "@wf-agent/types";
import type { AgentLoopEntity } from "../../entities/agent-loop-entity.js";
import type { ConversationSession } from "../../../core/messaging/conversation-session.js";
import type { LLMExecutor } from "../../../core/executors/llm-executor.js";
import type { ToolCallExecutor } from "../../../core/executors/tool-call-executor.js";
import type { EventRegistry } from "../../../core/registry/event-registry.js";
import type { ToolRegistry } from "../../../core/registry/tool-registry.js";
import { isAbortError, checkInterruption } from "@wf-agent/common-utils";
import { executeAgentHook } from "../handlers/hook-handlers/index.js";
import {
  handleAgentError,
  handleAgentInterruption as handleAgentInterruptionHandler,
} from "../handlers/agent-error-handler.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import {
  buildAgentStartedEvent,
  buildAgentCompletedEvent,
  buildAgentIterationCompletedEvent,
  buildAgentToolExecutionStartedEvent,
  buildAgentToolExecutionCompletedEvent,
} from "../../../core/utils/event/builders/agent-events.js";
import { ToolApprovalCoordinator } from "../../../core/coordinators/tool-approval-coordinator.js";

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
  /** Tool Call Executor */
  toolCallExecutor: ToolCallExecutor;
  /** Event emitter for agent events */
  emitAgentEvent: (event: AgentHookTriggeredEvent) => Promise<void>;
  /** Event Registry (optional) */
  eventManager?: EventRegistry;
  /** Tool Registry (for getting tool definitions) */
  toolService?: ToolRegistry;
  /** Tool Approval Handler (optional) */
  toolApprovalHandler?: ToolApprovalHandler;
}

/**
 * AgentExecutionCoordinator
 *
 * Coordinates the execution flow of agent loop:
 * - Manages iteration loop control
 * - Handles interruption signals (pause/stop/abort)
 * - Delegates to AgentIterationExecutor for single iteration
 * - Supports streaming with real-time event forwarding
 */
export class AgentExecutionCoordinator {
  private readonly llmExecutor: LLMExecutor;
  private readonly toolCallExecutor: ToolCallExecutor;
  private readonly emitAgentEvent: (event: AgentHookTriggeredEvent) => Promise<void>;
  private readonly eventManager?: EventRegistry;
  private readonly approvalCoordinator: ToolApprovalCoordinator;
  private readonly toolService?: ToolRegistry;
  private readonly toolApprovalHandler?: ToolApprovalHandler;

  constructor(deps: AgentExecutionCoordinatorDependencies) {
    this.llmExecutor = deps.llmExecutor;
    this.toolCallExecutor = deps.toolCallExecutor;
    this.emitAgentEvent = deps.emitAgentEvent;
    this.eventManager = deps.eventManager;
    this.toolService = deps.toolService;
    this.toolApprovalHandler = deps.toolApprovalHandler;
    
    // Initialize approval coordinator
    this.approvalCoordinator = new ToolApprovalCoordinator(deps.eventManager);
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

    try {
      while (entity.state.currentIteration < maxIterations) {
        logger.debug("Starting new iteration", {
          agentLoopId,
          iteration: entity.state.currentIteration + 1,
          maxIterations,
        });

        const interruptionResult = this.checkInterruption(entity);
        if (interruptionResult) {
          return interruptionResult;
        }

        const result = await this.executeIteration(
          entity,
          conversationManager,
          toolSchemas,
          profileId,
        );

        if (result.interruption) {
          return {
            success: false,
            iterations: entity.state.currentIteration,
            toolCallCount: entity.state.toolCallCount,
            error: `Execution ${result.interruption}`,
          };
        }

        if (!result.shouldContinue) {
          logger.info("Agent Loop execution completed successfully", {
            agentLoopId,
            iterations: entity.state.currentIteration,
            toolCallCount: entity.state.toolCallCount,
          });
          return {
            success: true,
            content: result.content,
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
      return {
        success: true,
        iterations: entity.state.currentIteration,
        toolCallCount: entity.state.toolCallCount,
        content: "Reached maximum iterations without final answer.",
      };
    } catch (error) {
      const standardizedError = await handleAgentError(
        entity,
        error as Error,
        "agent_loop_execution",
        undefined,
        this.eventManager,
      );

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
      while (entity.state.currentIteration < maxIterations) {
        logger.debug("Starting new stream iteration", {
          agentLoopId,
          iteration: entity.state.currentIteration + 1,
          maxIterations,
        });

        const interruptionResult = this.checkInterruptionStream(entity, agentLoopId);
        if (interruptionResult) {
          yield* interruptionResult;
          return;
        }

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
   * Check interruption signals (sync mode)
   */
  private checkInterruption(entity: AgentLoopEntity): AgentLoopResult | null {
    if (entity.isAborted() || entity.shouldStop()) {
      logger.info("Agent Loop execution cancelled", {
        agentLoopId: entity.id,
        iteration: entity.state.currentIteration,
      });
      entity.state.cancel();
      return {
        success: false,
        iterations: entity.state.currentIteration,
        toolCallCount: entity.state.toolCallCount,
        error: "Execution cancelled",
      };
    }

    if (entity.shouldPause()) {
      logger.info("Agent Loop execution paused", {
        agentLoopId: entity.id,
        iteration: entity.state.currentIteration,
      });
      entity.state.pause();
      return {
        success: false,
        iterations: entity.state.currentIteration,
        toolCallCount: entity.state.toolCallCount,
        error: "Execution paused",
      };
    }

    return null;
  }

  /**
   * Check interruption signals (stream mode)
   */
  private checkInterruptionStream(
    entity: AgentLoopEntity,
    agentLoopId: string,
  ): AsyncGenerator<AgentLoopStreamEvent> | null {
    if (entity.isAborted() || entity.shouldStop()) {
      logger.info("Agent Loop stream execution cancelled", {
        agentLoopId,
        iteration: entity.state.currentIteration,
      });
      entity.state.cancel();
      return this.createErrorGenerator(
        entity,
        agentLoopId,
        "Execution cancelled",
        "execution_cancelled",
      );
    }

    if (entity.shouldPause()) {
      logger.info("Agent Loop stream execution paused", {
        agentLoopId,
        iteration: entity.state.currentIteration,
      });
      entity.state.pause();
      return this.createErrorGenerator(entity, agentLoopId, "Execution paused", "execution_paused");
    }

    return null;
  }

  /**
   * Execute a single iteration (sync mode)
   */
  private async executeIteration(
    entity: AgentLoopEntity,
    conversationManager: ConversationSession,
    toolSchemas: ToolSchema[] | undefined,
    profileId: string,
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

    // Check interruption before LLM call
    const preLLMInterruption = checkInterruption(entity.getAbortSignal());
    if (preLLMInterruption.type === "paused" || preLLMInterruption.type === "stopped") {
      logger.info("Interrupted before LLM call", {
        agentLoopId,
        iteration: entity.state.currentIteration,
        interruptionType: preLLMInterruption.type,
      });
      return this.handleLLMFailure(entity, preLLMInterruption.type);
    }

    logger.debug("Calling LLM", {
      agentLoopId,
      iteration: entity.state.currentIteration,
      messageCount: conversationManager.getMessageCount(),
    });

    const llmResult = await this.llmExecutor.executeLLMCall(
      conversationManager.getMessages(),
      { prompt: "", profileId, parameters: {}, tools: toolSchemas, stream: false },
      { abortSignal: entity.getAbortSignal(), executionId: entity.id, nodeId: entity.nodeId },
    );

    // Check interruption after LLM call
    const postLLMInterruption = checkInterruption(entity.getAbortSignal());
    if (postLLMInterruption.type === "paused" || postLLMInterruption.type === "stopped") {
      logger.info("Interrupted after LLM call", {
        agentLoopId,
        iteration: entity.state.currentIteration,
        interruptionType: postLLMInterruption.type,
      });
      return this.handleLLMFailure(entity, postLLMInterruption.type);
    }

    logger.debug("LLM call completed", {
      agentLoopId,
      iteration: entity.state.currentIteration,
      success: llmResult.success,
      hasToolCalls: llmResult.success ? !!llmResult.result?.toolCalls?.length : false,
    });

    if (!llmResult.success) {
      return this.handleLLMFailure(entity, llmResult.interruption.type);
    }

    const response = llmResult.result;

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

    await this.executeToolCalls(entity, conversationManager, response.toolCalls);
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

    yield* this.executeToolCallsStream(entity, conversationManager, finalResult.toolCalls);

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
    messageStream: { on: Function; done: () => Promise<void>; getFinalResult: () => Promise<any> },
  ): AsyncGenerator<AgentLoopStreamEvent, { success: boolean; finalResult?: any }> {
    const eventQueue: MessageStreamEvent[] = [];
    let streamDone = false;
    let streamError: Error | null = null;

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
      if (entity.isAborted() || entity.shouldStop()) {
        const result = checkInterruption(entity.getAbortSignal());
        entity.state.cancel();
        yield this.createErrorEvent(
          agentLoopId,
          result.type === "paused" ? "Execution paused" : "Execution cancelled",
          entity.state.currentIteration,
          "stream_interruption",
        );
        await this.emitToRegistry(
          this.createErrorEvent(
            agentLoopId,
            result.type === "paused" ? "Execution paused" : "Execution cancelled",
            entity.state.currentIteration,
            "stream_interruption",
          ),
          entity,
        );
        return { success: false };
      }

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
  }

  /**
   * Handle LLM failure (sync mode)
   */
  private handleLLMFailure(
    entity: AgentLoopEntity,
    interruptionType: string,
  ): { success: boolean; shouldContinue: boolean; interruption?: string } {
    const agentLoopId = entity.id;
    const iteration = entity.state.currentIteration;

    if (interruptionType === "paused") {
      logger.info("LLM call paused", { agentLoopId, iteration });
      entity.state.pause();
      return { success: false, shouldContinue: false, interruption: "paused" };
    }

    if (interruptionType === "stopped" || interruptionType === "aborted") {
      logger.info("LLM call stopped", { agentLoopId, iteration });
      entity.state.cancel();
      return { success: false, shouldContinue: false, interruption: interruptionType };
    }

    throw new Error("LLM execution failed with unknown error");
  }

  /**
   * Handle stream LLM error
   */
  private async *handleStreamLLMError(
    entity: AgentLoopEntity,
    agentLoopId: string,
    error: Error,
  ): AsyncGenerator<AgentLoopStreamEvent, boolean> {
    if (isAbortError(error)) {
      const isInterruption = await handleAgentInterruptionHandler(
        entity,
        error,
        "llm_stream_call",
        this.eventManager,
      );
      if (isInterruption) {
        yield this.createErrorEvent(
          agentLoopId,
          (entity.state.error as Error | undefined)?.message || "Execution interrupted",
          entity.state.currentIteration,
          "llm_stream_call",
        );
        await this.emitToRegistry(
          this.createErrorEvent(
            agentLoopId,
            (entity.state.error as Error | undefined)?.message || "Execution interrupted",
            entity.state.currentIteration,
            "llm_stream_call",
          ),
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
    if (isAbortError(error)) {
      const isInterruption = await handleAgentInterruptionHandler(
        entity,
        error,
        "message_stream_done",
        this.eventManager,
      );
      if (isInterruption) {
        yield this.createErrorEvent(
          agentLoopId,
          (entity.state.error as Error | undefined)?.message || "Execution interrupted",
          entity.state.currentIteration,
          "llm_stream_call",
        );
        await this.emitToRegistry(
          this.createErrorEvent(
            agentLoopId,
            (entity.state.error as Error | undefined)?.message || "Execution interrupted",
            entity.state.currentIteration,
            "llm_stream_call",
          ),
          entity,
        );
        return { success: false };
      }
    }

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
    return { success: false };
  }

  /**
   * Execute tool calls (sync mode)
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

    // Check interruption before tool execution
    const preToolInterruption = checkInterruption(entity.getAbortSignal());
    if (preToolInterruption.type === "paused" || preToolInterruption.type === "stopped") {
      logger.info("Interrupted before tool execution", {
        agentLoopId,
        iteration,
        interruptionType: preToolInterruption.type,
        pendingToolCalls: toolCalls.length,
      });
      // Cancel all pending tool calls
      for (const tc of toolCalls) {
        entity.state.recordToolCallEnd(tc.id, undefined, "Cancelled due to interruption");
      }
      return;
    }

    // Convert to LLMToolCall format for batch processing
    const llmToolCalls = toolCalls.map(tc => ({
      id: tc.id,
      type: "function" as const,
      function: {
        name: tc.name,
        arguments: tc.arguments,
      },
    }));

    // Get approval config from agent configuration
    const approvalOptions = this.getApprovalOptions(entity);

    // Process batch through approval coordinator
    const batchResult = await this.approvalCoordinator.processToolBatch(
      llmToolCalls,
      approvalOptions,
      entity.id,
      entity.nodeId || "unknown",
      {
        requestApproval: async (request) => {
          return this.requestAgentApproval(request, entity);
        },
      },
      this.eventManager,
    );

    // Execute auto-approved tools
    for (let i = 0; i < batchResult.autoExecuted.length; i++) {
      const autoResult = batchResult.autoExecuted[i];
      // Auto-executed tools correspond to the first N tools in the batch
      if (i < toolCalls.length && autoResult) {
        const originalToolCall = toolCalls[i]!;
        await this.executeSingleApprovedTool(
          entity,
          conversationManager,
          originalToolCall,
          autoResult,
        );
      }
    }

    // Handle confirmation-required tool
    if (batchResult.confirmationRequired && batchResult.confirmationResult?.approved) {
      const confirmedToolCall = toolCalls.find(
        tc => tc.id === batchResult.confirmationRequired?.id,
      );
      if (confirmedToolCall) {
        // Apply edited parameters if provided
        if (batchResult.confirmationResult.editedParameters) {
          confirmedToolCall.arguments = JSON.stringify(
            batchResult.confirmationResult.editedParameters,
          );
        }

        // Add user instruction if provided
        if (batchResult.confirmationResult.userInstruction) {
          conversationManager.addMessage({
            role: "user",
            content: batchResult.confirmationResult.userInstruction,
          });
        }

        // Create placeholder result for execution
        const placeholderResult = {
          success: true,
          result: {},
          executionTime: 0,
          retryCount: 0,
        };

        await this.executeSingleApprovedTool(
          entity,
          conversationManager,
          confirmedToolCall,
          placeholderResult,
        );
      }

      // Continue with remaining queue if flag is set
      if (
        batchResult.confirmationResult.continueBatch !== false &&
        batchResult.remainingQueue.length > 0
      ) {
        const remainingToolCalls = batchResult.remainingQueue.map(tc => ({
          id: tc.id,
          name: tc.function?.name || "unknown",
          arguments: tc.function?.arguments || "{}",
        }));

        // Recursively process remaining tools
        await this.executeToolCalls(entity, conversationManager, remainingToolCalls);
      }
    }

    // Check interruption after tool execution
    const postToolInterruption = checkInterruption(entity.getAbortSignal());
    if (postToolInterruption.type === "paused" || postToolInterruption.type === "stopped") {
      logger.info("Interrupted after tool execution", {
        agentLoopId,
        iteration,
        interruptionType: postToolInterruption.type,
      });
      // Still record results but will be handled by main loop
    }

    logger.debug("Tool calls execution completed", {
      agentLoopId,
      iteration,
      autoExecutedCount: batchResult.autoExecuted.length,
      hasConfirmation: !!batchResult.confirmationRequired,
    });
  }

  /**
   * Execute a single approved tool call
   */
  private async executeSingleApprovedTool(
    entity: AgentLoopEntity,
    conversationManager: ConversationSession,
    toolCall: { id: string; name: string; arguments: string },
    executionResult: { success: boolean; result?: unknown; error?: string },
  ): Promise<void> {
    const agentLoopId = entity.id;

    const toolCallInfo = {
      id: toolCall.id,
      name: toolCall.name,
      arguments: toolCall.arguments ? JSON.parse(toolCall.arguments) : {},
    };

    await executeAgentHook(entity, "BEFORE_TOOL_CALL", this.emitAgentEvent, toolCallInfo);

    if (executionResult.success) {
      logger.debug("Tool call succeeded", {
        agentLoopId,
        toolCallId: toolCall.id,
        toolName: toolCallInfo.name,
      });

      // Execute actual tool call
      const toolResults = await this.toolCallExecutor.executeToolCalls(
        [toolCall],
        conversationManager,
        entity.id,
        entity.nodeId,
        { abortSignal: entity.getAbortSignal() },
      );

      const result = toolResults[0];
      if (result?.success) {
        entity.state.recordToolCallEnd(toolCall.id, result.result);
        await executeAgentHook(entity, "AFTER_TOOL_CALL", this.emitAgentEvent, {
          ...toolCallInfo,
          result: result.result,
        });
      } else {
        entity.state.recordToolCallEnd(
          toolCall.id,
          undefined,
          result?.error || "Tool execution failed",
        );
        await executeAgentHook(entity, "AFTER_TOOL_CALL", this.emitAgentEvent, {
          ...toolCallInfo,
          error: result?.error,
        });
      }
    } else {
      logger.warn("Tool call rejected or failed", {
        agentLoopId,
        toolCallId: toolCall.id,
        toolName: toolCallInfo.name,
        error: executionResult.error,
      });
      entity.state.recordToolCallEnd(
        toolCall.id,
        undefined,
        executionResult.error || "Rejected by user",
      );
      await executeAgentHook(entity, "AFTER_TOOL_CALL", this.emitAgentEvent, {
        ...toolCallInfo,
        error: executionResult.error,
      });
    }
  }

  /**
   * Get approval options from agent configuration
   *
   * @param entity - Agent loop entity
   * @returns ToolApprovalOptions
   */
  private getApprovalOptions(entity: AgentLoopEntity): ToolApprovalOptions {
    // Get from agent configuration or use defaults
    // For now, return safe defaults
    // In future, this could read from entity.config.approvalOptions
    return {
      autoApprovalEnabled: false,  // Safe default: require approval for all tools
    };
  }

  /**
   * Request tool approval for agent mode
   *
   * This is a placeholder implementation. In production, this should integrate
   * with the application's UI (CLI prompt, TUI interaction, web interface, etc.)
   *
   * @param request - Approval request
   * @param entity - Agent loop entity
   * @returns Approval result
   */
  private async requestAgentApproval(
    request: { toolCall: { id: string; function?: { name?: string; arguments?: string } } },
    entity: AgentLoopEntity,
  ): Promise<{ approved: boolean; toolCallId: string; editedParameters?: Record<string, unknown>; userInstruction?: string; rejectionReason?: string }> {
    // Use registered handler if available
    if (this.toolApprovalHandler) {
      try {
        // Construct full LLMToolCall object
        const llmToolCall: LLMToolCall = {
          id: request.toolCall.id,
          type: "function",
          function: {
            name: request.toolCall.function?.name || "unknown",
            arguments: request.toolCall.function?.arguments || "{}",
          },
        };

        const result = await this.toolApprovalHandler.requestApproval({
          toolCall: llmToolCall,
          batchId: undefined, // Will be set by processToolBatch context
          toolIndex: 0,
          totalTools: 1,
          pendingQueue: [],
          contextId: entity.id,
          nodeId: entity.nodeId || "unknown",
          interactionId: `approval-${Date.now()}-${request.toolCall.id}`,
        });

        return {
          approved: result.approved,
          toolCallId: result.toolCallId,
          editedParameters: result.editedParameters,
          userInstruction: result.userInstruction,
          rejectionReason: result.rejectionReason,
        };
      } catch (error) {
        logger.error("Tool approval handler failed", {
          agentLoopId: entity.id,
          toolCallId: request.toolCall.id,
          error,
        });

        return {
          approved: false,
          toolCallId: request.toolCall.id,
          rejectionReason: `Handler error: ${error instanceof Error ? error.message : String(error)}`,
        };
      }
    }

    // Fallback to current behavior when no handler configured
    const toolName = request.toolCall.function?.name || "unknown";
    
    logger.warn("No tool approval handler configured, rejecting by default", {
      agentLoopId: entity.id,
      toolCallId: request.toolCall.id,
      toolName,
    });

    return {
      approved: false,
      toolCallId: request.toolCall.id,
      rejectionReason: `No approval handler configured. Tool "${toolName}" requires manual approval but no handler is registered.`,
    };
  }

  /**
   * Execute tool calls (stream mode)
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

    const executorToolCalls = toolCalls.map(tc => ({
      id: tc.id,
      name: tc.function.name,
      arguments: tc.function.arguments,
    }));

    const toolResults = await this.toolCallExecutor.executeToolCalls(
      executorToolCalls,
      conversationManager,
      entity.id,
      entity.nodeId,
      { abortSignal: entity.getAbortSignal() },
    );

    for (const result of toolResults) {
      const toolCall = toolCalls.find(tc => tc.id === result.toolCallId);
      const args = toolCall ? JSON.parse(toolCall.function.arguments) : {};
      const toolCallInfo = {
        id: result.toolCallId,
        name: toolCall?.function.name || "",
        arguments: args,
      };

      await executeAgentHook(entity, "BEFORE_TOOL_CALL", this.emitAgentEvent, toolCallInfo);

      const startEvent: AgentStreamEvent = {
        type: AgentStreamEventType.TOOL_EXECUTION_START,
        timestamp: Date.now(),
        agentLoopId,
        toolCallId: result.toolCallId,
        toolName: toolCallInfo.name,
        args,
        iteration: entity.state.currentIteration,
      };
      yield startEvent;
      await this.emitToRegistry(startEvent, entity);

      entity.state.recordToolCallStart(result.toolCallId, toolCallInfo.name, args);

      if (result.success) {
        entity.state.recordToolCallEnd(result.toolCallId, result.result);
        await executeAgentHook(entity, "AFTER_TOOL_CALL", this.emitAgentEvent, {
          ...toolCallInfo,
          result: result.result,
        });

        const endEvent: AgentStreamEvent = {
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
        yield endEvent;
        await this.emitToRegistry(endEvent, entity);
      } else {
        entity.state.recordToolCallEnd(result.toolCallId, undefined, result.error);
        await executeAgentHook(entity, "AFTER_TOOL_CALL", this.emitAgentEvent, {
          ...toolCallInfo,
          error: result.error,
        });

        const endEvent: AgentStreamEvent = {
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
        yield endEvent;
        await this.emitToRegistry(endEvent, entity);
      }
    }
  }

  // ============ Event Factory Methods ============

  private createAgentStartEvent(
    agentLoopId: string,
    maxIterations: number,
    initialMessageCount: number,
  ): AgentStreamEvent {
    return {
      type: AgentStreamEventType.AGENT_START,
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
      type: AgentStreamEventType.AGENT_END,
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
      type: AgentStreamEventType.ITERATION_COMPLETE,
      timestamp: Date.now(),
      agentLoopId,
      iteration,
      shouldContinue,
    };
  }

  private createIterationStartEvent(agentLoopId: string, iteration: number): AgentStreamEvent {
    return {
      type: AgentStreamEventType.ITERATION_START,
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
      type: AgentStreamEventType.ERROR,
      timestamp: Date.now(),
      agentLoopId,
      error,
      iteration,
      context,
    };
  }

  private async *createErrorGenerator(
    entity: AgentLoopEntity,
    agentLoopId: string,
    error: string,
    context: string,
  ): AsyncGenerator<AgentLoopStreamEvent> {
    const event = this.createErrorEvent(agentLoopId, error, entity.state.currentIteration, context);
    yield event;
    await this.emitToRegistry(event, entity);
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
      case AgentStreamEventType.AGENT_START:
        return buildAgentStartedEvent({
          ...baseData,
          maxIterations: event.maxIterations,
          initialMessageCount: event.initialMessageCount,
        });
      case AgentStreamEventType.AGENT_END:
        return buildAgentCompletedEvent({
          ...baseData,
          iterations: event.iterations,
          toolCallCount: event.toolCallCount,
          success: event.success,
          error: event.success ? undefined : event.error,
        });
      case AgentStreamEventType.ITERATION_COMPLETE:
        return buildAgentIterationCompletedEvent({
          ...baseData,
          iteration: event.iteration,
          toolCallCount: 0,
          shouldContinue: event.shouldContinue,
        });
      case AgentStreamEventType.TOOL_EXECUTION_START:
        return buildAgentToolExecutionStartedEvent({
          ...baseData,
          toolCallId: event.toolCallId,
          toolName: event.toolName,
          iteration: event.iteration,
        });
      case AgentStreamEventType.TOOL_EXECUTION_END:
        return buildAgentToolExecutionCompletedEvent({
          ...baseData,
          toolCallId: event.toolCallId,
          toolName: event.toolName,
          success: event.result.success,
          duration: event.duration,
          error: event.result.success ? undefined : event.result.error,
          iteration: entity.state.currentIteration,
        });
      case AgentStreamEventType.ERROR:
        return null;
      default:
        return null;
    }
  }
}
