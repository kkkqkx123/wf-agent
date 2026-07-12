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
 * - Supports retry with exponential backoff (Task #7)
 * - Supports per-execution timeout (Task #9)
 * - Integrates with global retry budget (Task #8)
 */

import type { AgentLoopResult, AgentStreamEvent, ToolSchema, LLMMessage } from "@wf-agent/types";
import type { RetryPolicy } from "@wf-agent/types";
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

// ============================================================================
// Retry and Timeout Helper Functions (Task #7, #9)
// ============================================================================

/**
 * Execute iteration with retry and timeout support
 *
 * Wraps iteration execution with:
 * - Exponential backoff retry (if retryPolicy configured)
 * - Timeout protection (if executionTimeout configured)
 * - Global retry budget checking and consumption
 *
 * @param agentLoopId Agent loop ID for logging
 * @param entity Agent loop entity
 * @param executor Async function that performs the iteration
 * @param retryPolicy Optional retry policy configuration
 * @param timeoutMs Optional timeout in milliseconds
 * @returns Execution result
 */
async function executeIterationWithRetryAndTimeout<T>(
  agentLoopId: string,
  entity: AgentLoopEntity,
  executor: () => Promise<T>,
  retryPolicy?: RetryPolicy,
  timeoutMs?: number,
): Promise<T> {
  const maxAttempts = retryPolicy?.enabled ? (retryPolicy.maxRetries ?? 0) + 1 : 1;
  let lastError: Error | undefined;

  for (let attemptCount = 0; attemptCount < maxAttempts; attemptCount++) {
    try {
      // Apply timeout if configured
      if (timeoutMs) {
        return await executeWithTimeout(executor, timeoutMs, "Agent loop execution");
      }

      return await executor();
    } catch (error) {
      lastError = error as Error;
      const isLastAttempt = attemptCount === maxAttempts - 1;

      // Log failure
      logger.debug("Iteration failed", {
        agentLoopId,
        attemptCount,
        isLastAttempt,
        error: lastError.message,
      });

      // If this is the last attempt or retry is disabled, throw
      if (isLastAttempt || !retryPolicy?.enabled) {
        throw lastError;
      }

      // Check if we should retry this error
      if (!retryPolicy.shouldRetry(lastError, attemptCount)) {
        logger.debug("Error not retryable, stopping retry", {
          agentLoopId,
          attemptCount,
          error: lastError.message,
        });
        throw lastError;
      }

      // Calculate delay for next retry
      const delayMs = retryPolicy.getNextDelay(attemptCount);

      // Check global retry budget before consuming
      const retryBudget = (entity as any).getRetryBudget?.();
      if (retryBudget && !retryBudget.canRetry(delayMs)) {
        logger.warn("Global retry budget exhausted, stopping retry", {
          agentLoopId,
          attemptCount,
        });
        throw lastError;
      }

      // Consume from budget if available
      if (retryBudget) {
        retryBudget.consumeRetry(delayMs);
      }

      // Wait before retry with exponential backoff
      logger.info("Retrying iteration after delay", {
        agentLoopId,
        attemptCount,
        delayMs,
        nextAttempt: attemptCount + 2,
        maxAttempts,
      });

      await delay(delayMs);
    }
  }

  // Should never reach here, but just in case
  throw lastError || new Error("Iteration failed for unknown reason");
}

/**
 * Execute a promise with timeout
 *
 * @param executor Async function to execute
 * @param timeoutMs Timeout in milliseconds
 * @param context Context for error message
 * @returns Execution result
 */
async function executeWithTimeout<T>(
  executor: () => Promise<T>,
  timeoutMs: number,
  context: string,
): Promise<T> {
  const timeoutPromise = new Promise<never>((_, reject) => {
    setTimeout(() => {
      reject(new Error(`${context} exceeded ${timeoutMs}ms timeout`));
    }, timeoutMs);
  });

  return Promise.race([executor(), timeoutPromise]);
}

/**
 * Sleep helper function
 *
 * @param ms Milliseconds to sleep
 */
function delay(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}

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
    const config = entity.config;

    this.recordExecutionStart(profileId, config.agentConfigId || "unknown", agentLoopId);

    try {
      const result = await executeWithInterruptionHandling(async signal => {
        while (entity.state.currentIteration < maxIterations) {
          logger.debug("Starting new iteration", {
            agentLoopId,
            iteration: entity.state.currentIteration + 1,
            maxIterations,
          });

          const iterationResult = await executeIterationWithRetryAndTimeout(
            agentLoopId,
            entity,
            () =>
              this.iterationCoordinator.executeIteration(
                entity,
                conversationManager,
                toolSchemas,
                profileId,
                signal,
              ),
            config.retryPolicy,
            config.executionTimeout,
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
    const config = entity.config;

    while (entity.state.currentIteration < maxIterations) {
      logger.debug("Starting new stream iteration", {
        agentLoopId,
        iteration: entity.state.currentIteration + 1,
        maxIterations,
      });

      // For streaming, we need to wrap the generator with retry and timeout support
      const shouldContinue = yield* this.executeIterationStreamWithRetryAndTimeout(
        agentLoopId,
        entity,
        conversationManager,
        toolSchemas,
        profileId,
        config.retryPolicy,
        config.executionTimeout,
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

  /**
   * Execute iteration stream with retry and timeout support
   *
   * For streaming execution, we need to handle retries at the generator level.
   * Each retry will re-execute the iteration stream generator.
   *
   * Semantics:
   * - Returns true: iteration completed normally (agent decided to continue or stop)
   * - Throws error: iteration failed after all retries exhausted
   *
   * Note: Timeout errors are NOT retried (wastes budget on already-failed operations)
   */
  private async *executeIterationStreamWithRetryAndTimeout(
    agentLoopId: string,
    entity: AgentLoopEntity,
    conversationManager: ConversationSession,
    toolSchemas: ToolSchema[] | undefined,
    profileId: string,
    retryPolicy?: RetryPolicy,
    timeoutMs?: number,
  ): AsyncGenerator<AgentLoopStreamEvent, boolean> {
    const maxAttempts = retryPolicy?.enabled ? (retryPolicy.maxRetries ?? 0) + 1 : 1;
    let lastError: Error | undefined;

    for (let attemptCount = 0; attemptCount < maxAttempts; attemptCount++) {
      try {
        // For streaming, we need to handle timeout differently
        // We'll track elapsed time and emit events accordingly
        let result: boolean;

        if (timeoutMs) {
          result = yield* this.executeIterationStreamWithTimeout(
            entity,
            conversationManager,
            toolSchemas,
            profileId,
            timeoutMs,
          );
        } else {
          result = yield* this.iterationCoordinator.executeIterationStream(
            entity,
            conversationManager,
            toolSchemas,
            profileId,
          );
        }

        // Iteration completed successfully
        return result;
      } catch (error) {
        lastError = error as Error;
        const isLastAttempt = attemptCount === maxAttempts - 1;

        logger.debug("Stream iteration failed", {
          agentLoopId,
          attemptCount,
          isLastAttempt,
          error: lastError.message,
        });

        if (isLastAttempt || !retryPolicy?.enabled) {
          throw lastError;
        }

        // Timeout errors should NOT be retried - already waited maximum time
        if (lastError.message?.includes("timeout")) {
          logger.debug("Timeout error not retryable, stopping retry", {
            agentLoopId,
            attemptCount,
          });
          throw lastError;
        }

        if (!retryPolicy.shouldRetry(lastError, attemptCount)) {
          logger.debug("Error not retryable in stream, stopping retry", {
            agentLoopId,
            attemptCount,
          });
          throw lastError;
        }

        const delayMs = retryPolicy.getNextDelay(attemptCount);

        const retryBudget = (entity as any).getRetryBudget?.();
        if (retryBudget && !retryBudget.canRetry(delayMs)) {
          logger.warn("Global retry budget exhausted in stream, stopping retry", {
            agentLoopId,
            attemptCount,
          });
          throw lastError;
        }

        if (retryBudget) {
          retryBudget.consumeRetry(delayMs);
        }

        logger.info("Retrying stream iteration after delay", {
          agentLoopId,
          attemptCount,
          delayMs,
          nextAttempt: attemptCount + 2,
          maxAttempts,
        });

        await delay(delayMs);
      }
    }

    throw lastError || new Error("Stream iteration failed for unknown reason");
  }

  /**
   * Execute iteration stream with timeout protection
   * Returns true if should continue, false if should stop
   */
  private async *executeIterationStreamWithTimeout(
    entity: AgentLoopEntity,
    conversationManager: ConversationSession,
    toolSchemas: ToolSchema[] | undefined,
    profileId: string,
    timeoutMs: number,
  ): AsyncGenerator<AgentLoopStreamEvent, boolean> {
    const startTime = Date.now();
    const iterationGenerator = this.iterationCoordinator.executeIterationStream(
      entity,
      conversationManager,
      toolSchemas,
      profileId,
    );

    for await (const event of iterationGenerator) {
      // Check timeout before yielding each event
      const elapsed = Date.now() - startTime;
      if (elapsed > timeoutMs) {
        throw new Error(`Stream iteration exceeded ${timeoutMs}ms timeout after ${elapsed}ms`);
      }

      yield event;
    }

    return true;
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
