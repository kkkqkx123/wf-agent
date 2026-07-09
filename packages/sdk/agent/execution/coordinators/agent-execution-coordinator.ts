/**
 * AgentExecutionCoordinator - Agent Execution Coordinator
 *
 * Coordinates the execution flow of an agent loop and orchestrates the execution of each component.
 *
 * Design Principles:
 * - Coordinator pattern: orchestrates execution flow
 * - Stateless design, all state managed through AgentLoopEntity
 * - Delegates to AgentIterationCoordinator for per-iteration execution
 * - Delegates to ToolExecutionCoordinator for tool execution
 * - Supports both sync and streaming execution modes
 * - Integrates with interruption handling
 */

import type { AgentLoopResult, AgentStreamEvent, ToolSchema, LLMMessage } from "@wf-agent/types";
import type { AgentLoopEntity } from "../../entities/agent-loop-entity.js";
import type { ConversationSession } from "../../../shared/messaging/conversation-session.js";
import type { MetricsRegistry } from "../../../metrics/metrics-registry.js";
import {
  executeWithInterruptionHandling,
  iterateWithInterruptionHandling,
} from "../../../shared/utils/interruption/index.js";
import { handleAgentError } from "../handlers/agent-error-handler.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import {
  AgentIterationCoordinator,
  type AgentLoopStreamEvent,
} from "./agent-iteration-coordinator.js";

export type { AgentLoopStreamEvent };

const logger = createContextualLogger({ component: "AgentExecutionCoordinator" });

/**
 * AgentExecutionCoordinator Dependencies
 */
export interface AgentExecutionCoordinatorDependencies {
  /** Iteration Coordinator (handles per-iteration execution) */
  iterationCoordinator: AgentIterationCoordinator;
  /** Metrics Registry (optional) */
  metricsRegistry?: MetricsRegistry;
}

/**
 * AgentExecutionCoordinator
 *
 * Coordinates the execution flow of agent loop:
 * - Manages iteration loop control (sync & stream)
 * - Handles interruption signals (pause/stop/abort)
 * - Delegates iteration execution to AgentIterationCoordinator
 * - Records loop-level metrics
 */
export class AgentExecutionCoordinator {
  private readonly iterationCoordinator: AgentIterationCoordinator;
  private readonly metricsRegistry?: MetricsRegistry;

  constructor(deps: AgentExecutionCoordinatorDependencies) {
    this.iterationCoordinator = deps.iterationCoordinator;
    this.metricsRegistry = deps.metricsRegistry;
  }

  /**
   * Execute agent loop (synchronous mode)
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

    this.recordExecutionStart(profileId, entity.config.agentConfigId || "unknown", agentLoopId);

    try {
      const result = await executeWithInterruptionHandling(async signal => {
        while (entity.state.currentIteration < maxIterations) {
          logger.debug("Starting new iteration", {
            agentLoopId,
            iteration: entity.state.currentIteration + 1,
            maxIterations,
          });

          const iterationResult = await this.iterationCoordinator.executeIteration(
            entity,
            conversationManager,
            toolSchemas,
            profileId,
            signal,
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

            this.recordExecutionComplete(profileId, startTime, true);

            return {
              success: true,
              content: iterationResult.content,
              iterations: entity.state.currentIteration,
              toolCallCount: entity.state.toolCallCount,
              completionData: iterationResult.completionData,
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
        this.recordExecutionComplete(profileId, startTime, true);

        return {
          success: true,
          iterations: entity.state.currentIteration,
          toolCallCount: entity.state.toolCallCount,
          content: "Reached maximum iterations without final answer.",
        };
      }, entity.getAbortSignal());

      if (!result.success) {
        const interruption = result.interruption;
        const type = interruption.type === "paused" ? "PAUSE" : "STOP";

        if (type === "PAUSE") {
          entity.state.pause();
        } else {
          entity.state.cancel();
        }

        this.recordExecutionComplete(profileId, startTime, false);

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
      );

      this.recordExecutionComplete(profileId, startTime, false);

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
   */
  async *executeStream(
    entity: AgentLoopEntity,
    conversationManager: ConversationSession,
    toolSchemas: ToolSchema[] | undefined,
    profileId: string,
    maxIterations: number,
  ): AsyncGenerator<AgentLoopStreamEvent> {
    const agentLoopId = entity.id;

    const startEvent = this.createAgentStartEvent(
      agentLoopId,
      maxIterations,
      conversationManager.getMessageCount(),
    );
    yield startEvent;
    await this.iterationCoordinator.emitToRegistry(startEvent, entity);

    try {
      const mainLoop = this.createMainLoopIterator(
        entity,
        conversationManager,
        toolSchemas,
        profileId,
        maxIterations,
      );

      for await (const item of iterateWithInterruptionHandling(mainLoop, entity.getAbortSignal())) {
        if (item.type === "interrupted") {
          const interruption = item.interruption;
          const type = interruption.type === "paused" ? "PAUSE" : "STOP";

          if (type === "PAUSE") {
            entity.state.pause();
          } else {
            entity.state.cancel();
          }

          const errorEvent = this.createErrorEvent(
            agentLoopId,
            `Execution ${interruption.type}`,
            entity.state.currentIteration,
            "stream_interruption",
          );
          yield errorEvent;
          await this.iterationCoordinator.emitToRegistry(errorEvent, entity);
          return;
        }

        yield item.value;
      }
    } catch (error) {
      const standardizedError = await handleAgentError(
        entity,
        error as Error,
        "agent_loop_stream_execution",
      );

      const errorEvent = this.createErrorEvent(
        agentLoopId,
        standardizedError.message,
        entity.state.currentIteration,
        "agent_loop_stream_execution",
      );
      yield errorEvent;
      await this.iterationCoordinator.emitToRegistry(errorEvent, entity);
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

      const shouldContinue = yield* this.iterationCoordinator.executeIterationStream(
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
    const endEvent = this.createAgentEndEvent(
      agentLoopId,
      conversationManager.getMessages(),
      entity.state.currentIteration,
      entity.state.toolCallCount,
      true,
    );
    yield endEvent;
    await this.iterationCoordinator.emitToRegistry(endEvent, entity);
  }

  // ============ Metrics Recording ============

  private recordExecutionStart(
    profileId: string,
    agentConfigId: string,
    agentLoopId: string,
  ): void {
    if (!this.metricsRegistry) return;
    const agentCollector = this.metricsRegistry.getAgentCollector();
    if (agentCollector) {
      agentCollector.recordExecutionStart(profileId, agentConfigId, agentLoopId);
    }
  }

  private recordExecutionComplete(profileId: string, startTime: number, success: boolean): void {
    if (!this.metricsRegistry) return;
    const duration = Date.now() - startTime;
    const agentCollector = this.metricsRegistry.getAgentCollector();
    if (agentCollector) {
      agentCollector.recordExecutionComplete(profileId, {
        iterations: 0,
        toolCallCount: 0,
        duration,
        success,
      });
    }
  }

  // ============ Event Factory Methods (loop-scoped only) ============

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
}
