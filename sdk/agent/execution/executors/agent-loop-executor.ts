/**
 * Agent Loop Executor
 *
 * Main coordinator for Agent loop execution.
 * Delegates to AgentIterationExecutor and AgentStreamExecutor for actual execution.
 *
 * Design Principles:
 * - Coordinator pattern: delegates to specialized executors
 * - Stateless design, all state managed through AgentLoopEntity
 * - Supports pause/resume functionality
 * - Supports interruption control (AbortController)
 * - Integrates with Hook mechanism
 * - Uses unified ConversationSession for message management
 *
 * Architecture:
 * - AgentLoopExecutor: Main coordinator (this file)
 * - AgentIterationExecutor: Single iteration execution
 * - AgentStreamExecutor: Streaming execution
 * - ConversationSession: Unified message history management
 * - tool-schema-helper: Tool schema utilities (shared with Graph)
 */

import type { AgentLoopResult, AgentStreamEvent, AgentCustomEvent, MessageStreamEvent } from "@wf-agent/types";
import { AgentLoopEntity } from "../../entities/agent-loop-entity.js";
import { ToolRegistry } from "../../../core/registry/tool-registry.js";

import { LLMExecutor } from "../../../core/executors/llm-executor.js";
import { ToolCallExecutor } from "../../../core/executors/tool-call-executor.js";
import type { EventRegistry } from "../../../core/registry/event-registry.js";
import { safeEmit } from "../../../core/utils/event/event-emitter.js";
import { handleAgentError } from "../handlers/agent-error-handler.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import { AgentIterationExecutor } from "./agent-iteration-executor.js";
import { AgentStreamExecutor } from "./agent-stream-executor.js";
import { prepareToolSchemas } from "../../../core/utils/tools/tool-schema-helper.js";

const logger = createContextualLogger({ component: "AgentLoopExecutor" });

/**
 * Agent Loop Stream Event
 *
 * Union type containing:
 * - LLM layer events (MessageStreamEvent): text delta, tool argument parsing, etc.
 * - Agent layer events (AgentStreamEvent): tool calls, iteration complete, etc.
 */
export type AgentLoopStreamEvent = MessageStreamEvent | AgentStreamEvent;

/**
 * Agent Loop Executor Class
 *
 * Provides methods to execute Agent loops (sync and stream)
 * Accepts AgentLoopEntity parameter, supports state tracking and interruption control
 *
 * Refactoring Notes:
 * - Delegates to AgentIterationExecutor for single iteration execution
 * - Delegates to AgentStreamExecutor for streaming execution
 * - Uses ConversationSession from AgentLoopEntity for message management
 */
export class AgentLoopExecutor {
  private llmExecutor: LLMExecutor;
  private toolCallExecutor: ToolCallExecutor;
  private toolService: ToolRegistry;
  private eventManager?: EventRegistry;
  private emitEvent?: (event: AgentCustomEvent) => Promise<void>;

  // Sub-executors
  private iterationExecutor: AgentIterationExecutor;
  private streamExecutor: AgentStreamExecutor;

  constructor(
    llmExecutor: LLMExecutor,
    toolService: ToolRegistry,
    eventManager?: EventRegistry,
    emitEvent?: (event: AgentCustomEvent) => Promise<void>,
  ) {
    this.llmExecutor = llmExecutor;
    this.toolService = toolService;
    this.eventManager = eventManager;
    this.emitEvent = emitEvent;
    this.toolCallExecutor = new ToolCallExecutor(toolService);

    // Initialize sub-executors
    this.iterationExecutor = new AgentIterationExecutor(
      llmExecutor,
      this.toolCallExecutor,
      this.emitAgentEvent.bind(this),
    );
    this.streamExecutor = new AgentStreamExecutor(
      llmExecutor,
      this.toolCallExecutor,
      toolService,
      this.emitAgentEvent.bind(this),
      eventManager,
    );
  }

  /**
   * Set event emitter function
   * @param emitEvent Event emitter function
   */
  setEventEmitter(emitEvent: (event: AgentCustomEvent) => Promise<void>): void {
    this.emitEvent = emitEvent;
    // Update sub-executors
    this.iterationExecutor = new AgentIterationExecutor(
      this.llmExecutor,
      this.toolCallExecutor,
      this.emitAgentEvent.bind(this),
    );
    this.streamExecutor = new AgentStreamExecutor(
      this.llmExecutor,
      this.toolCallExecutor,
      this.toolService,
      this.emitAgentEvent.bind(this),
      this.eventManager,
    );
  }

  /**
   * Set event manager
   * @param eventManager Event manager
   */
  setEventManager(eventManager: EventRegistry): void {
    this.eventManager = eventManager;
  }

  /**
   * Unified event emission method
   * Prioritizes EventRegistry, otherwise uses emitEvent callback
   * @param event Agent custom event
   */
  private async emitAgentEvent(event: AgentCustomEvent): Promise<void> {
    if (this.eventManager) {
      await safeEmit(this.eventManager, event);
    } else if (this.emitEvent) {
      await this.emitEvent(event);
    }
  }

  /**
   * Execute Agent Loop (based on AgentLoopEntity)
   *
   * Uses ConversationSession from AgentLoopEntity for message management.
   * No longer creates temporary MessageHistory instances.
   *
   * @param entity Agent Loop entity
   * @returns Execution result
   */
  async execute(entity: AgentLoopEntity): Promise<AgentLoopResult> {
    const config = entity.config;
    const maxIterations = config.maxIterations ?? 10;

    // Use ConversationSession from entity (unified message management)
    const conversationManager = entity.conversationManager;

    logger.info("Agent Loop execution started", {
      agentLoopId: entity.id,
      maxIterations,
      toolsCount: config.tools?.length || 0,
      profileId: config.profileId || "DEFAULT",
      initialMessageCount: conversationManager.getMessageCount(),
    });

    // Prepare tool schemas using shared helper
    const toolSchemas = prepareToolSchemas(config.tools, this.toolService);

    if (toolSchemas) {
      logger.debug("Tool schemas prepared", {
        agentLoopId: entity.id,
        toolsCount: toolSchemas.length,
      });
    }

    try {
      while (entity.state.currentIteration < maxIterations) {
        logger.debug("Starting new iteration", {
          agentLoopId: entity.id,
          iteration: entity.state.currentIteration + 1,
          maxIterations,
        });

        // Check interruption signals
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

        // Check pause signal
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

        // Execute one iteration using AgentIterationExecutor
        // Pass ConversationSession instead of MessageHistory
        const result = await this.iterationExecutor.execute(
          entity,
          conversationManager,
          toolSchemas,
          config.profileId || "DEFAULT",
        );

        // Handle iteration result
        if (result.interruption) {
          return {
            success: false,
            iterations: entity.state.currentIteration,
            toolCallCount: entity.state.toolCallCount,
            error: `Execution ${result.interruption}`,
          };
        }

        if (!result.shouldContinue) {
          // Loop completed
          logger.info("Agent Loop execution completed successfully", {
            agentLoopId: entity.id,
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
          agentLoopId: entity.id,
          iteration: entity.state.currentIteration,
        });
      }

      // Reached max iterations
      logger.info("Agent Loop reached maximum iterations", {
        agentLoopId: entity.id,
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
   * Stream execute Agent Loop (based on AgentLoopEntity)
   *
   * Returns union type AgentLoopStreamEvent containing:
   * - LLM layer events (MessageStreamEvent): forwarded from MessageStream
   * - Agent layer events (AgentStreamEvent): tool calls, iteration complete, etc.
   *
   * Uses ConversationSession from AgentLoopEntity for message management.
   *
   * @param entity Agent Loop entity
   * @returns Stream event generator
   */
  async *executeStream(entity: AgentLoopEntity): AsyncGenerator<AgentLoopStreamEvent> {
    const config = entity.config;
    const maxIterations = config.maxIterations ?? 10;

    // Use ConversationSession from entity (unified message management)
    const conversationManager = entity.conversationManager;

    logger.info("Agent Loop stream execution started", {
      agentLoopId: entity.id,
      maxIterations,
      toolsCount: config.tools?.length || 0,
      profileId: config.profileId || "DEFAULT",
      initialMessageCount: conversationManager.getMessageCount(),
    });

    // Prepare tool schemas using shared helper
    const toolSchemas = prepareToolSchemas(config.tools, this.toolService);

    // Delegate to AgentStreamExecutor
    yield* this.streamExecutor.execute(
      entity,
      conversationManager,
      toolSchemas,
      config.profileId || "DEFAULT",
      maxIterations,
    );
  }
}
