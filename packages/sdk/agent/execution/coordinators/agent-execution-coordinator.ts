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
import { RetryBudget } from "../../../shared/coordinators/retry-budget.js";
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
 * - Time budget checking with configurable modes (delay-only or total-time)
 *
 * Problem #5 Fix: Supports timeBudgetMode for consistent time tracking
 *
 * @param agentLoopId Agent loop ID for logging
 * @param entity Agent loop entity
 * @param executor Async function that performs the iteration
 * @param retryPolicy Optional retry policy configuration
 * @param timeoutMs Optional timeout in milliseconds
 * @param retryBudget Optional retry budget for tracking retry delays/execution time
 * @returns Execution result
 */
async function executeIterationWithRetryAndTimeout<T>(
  agentLoopId: string,
  entity: AgentLoopEntity,
  executor: () => Promise<T>,
  retryPolicy?: RetryPolicy,
  timeoutMs?: number,
  retryBudget?: RetryBudget,
): Promise<T> {
  const maxAttempts = retryPolicy?.enabled ? (retryPolicy.maxRetries ?? 0) + 1 : 1;
  let lastError: Error | undefined;

  for (let attemptCount = 0; attemptCount < maxAttempts; attemptCount++) {
    const attemptStartTime = Date.now();

    try {
      // Check retry budget before attempting execution
      if (retryBudget && retryBudget.isExhausted()) {
        logger.warn("Retry budget exhausted before attempt", {
          agentLoopId,
          attemptCount,
          budgetStats: retryBudget.getState(),
        });
        throw lastError || new Error("Retry budget exhausted for retry operations");
      }

      // Apply timeout if configured
      if (timeoutMs) {
        return await executeWithTimeout(executor, timeoutMs, "Agent loop execution");
      }

      const result = await executor();

      // Record execution time in budget if in total-time mode
      if (retryBudget) {
        const executionTime = Date.now() - attemptStartTime;
        retryBudget.recordExecutionTime(executionTime);
      }

      return result;
    } catch (error) {
      lastError = error as Error;
      const isLastAttempt = attemptCount === maxAttempts - 1;

      // [P6 Fix] Detect timeout errors and increment timeout counter
      if (timeoutMs && lastError.message.includes("timeout")) {
        entity.state.incrementTimeoutCount();
        logger.debug("Iteration timeout detected and counted", {
          agentLoopId,
          timeoutCount: entity.state.timeoutCount,
          timeoutMs,
        });
      }

      // Record execution time in budget
      if (retryBudget) {
        const executionTime = Date.now() - attemptStartTime;
        retryBudget.recordExecutionTime(executionTime);
      }

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

      // Check retry budget and consume in single operation
      if (retryBudget) {
        const budgetCheck = retryBudget.consumeRetry(delayMs);
        if (!budgetCheck.allowed) {
          logger.warn("Retry budget would be exceeded by retry delay, stopping retry", {
            agentLoopId,
            attemptCount,
            delayMs,
            reason: budgetCheck.reason,
            budgetStats: retryBudget.getState(),
          });
          throw lastError;
        }
      }

      // Wait before retry with exponential backoff
      logger.info("Retrying iteration after delay", {
        agentLoopId,
        attemptCount,
        delayMs,
        nextAttempt: attemptCount + 2,
        maxAttempts,
        budgetStats: retryBudget?.getState(),
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
 * Execute agent loop with main-level retry and failure handling
 *
 * Implements unified FailurePolicy framework (Problem #6):
 * - 'fail': Return error immediately on failure
 * - 'retry': Retry entire agent execution with configurable parameters
 * - 'continue': Use fallbackOutput on failure
 *
 * This wraps the core loop execution with failure policy enforcement.
 * Supports time budget tracking for consistent resource management.
 */
async function executeAgentLoopWithFailurePolicy(
  agentLoopId: string,
  executor: () => Promise<AgentLoopResult>,
  onFailure: string = "fail",
  maxMainLoopRetries: number = 0,
  mainLoopRetryDelay: number = 1000,
  mainLoopExponentialBackoff: boolean = true,
  retryBudget?: RetryBudget,
): Promise<AgentLoopResult> {
  const maxAttempts = onFailure === "retry" ? (maxMainLoopRetries + 1) : 1;
  let lastError: any = undefined;
  let mainLoopRetryCount = 0;
  const mainLoopRetryDelays: number[] = [];

  for (let attemptCount = 0; attemptCount < maxAttempts; attemptCount++) {
    try {
      const result = await executor();

      // If successful, enrich with retry statistics
      if (result.success) {
        return enrichResultWithMainLoopStats(result, mainLoopRetryCount, mainLoopRetryDelays);
      }

      // If failed, store error for potential retry
      lastError = result;
      const isLastAttempt = attemptCount === maxAttempts - 1;

      if (isLastAttempt) {
        // Last attempt and still failed
        break;
      }

      // For 'retry' strategy, continue to next attempt
      if (onFailure === "retry") {
        const delayMs = mainLoopExponentialBackoff
          ? Math.min(mainLoopRetryDelay * Math.pow(2, attemptCount), 60000)
          : mainLoopRetryDelay;

        mainLoopRetryDelays.push(delayMs);
        mainLoopRetryCount++;

        logger.info("Agent loop execution failed, retrying main loop", {
          agentLoopId,
          attemptCount,
          delayMs,
          nextAttempt: attemptCount + 2,
          maxAttempts,
        });

        // Check retry budget and consume in single operation
        if (retryBudget) {
          const budgetCheck = retryBudget.consumeRetry(delayMs);
          if (!budgetCheck.allowed) {
            logger.warn("Retry budget would be exceeded by main loop retry delay", {
              agentLoopId,
              attemptCount,
              delayMs,
              reason: budgetCheck.reason,
              budgetStats: retryBudget.getState(),
            });
            // Budget exhausted, don't retry
            break;
          }
        }

        await delay(delayMs);
        continue;
      }

      // For other strategies, don't retry
      break;
    } catch (error) {
      lastError = error;
      const isLastAttempt = attemptCount === maxAttempts - 1;

      logger.debug("Agent loop execution threw error", {
        agentLoopId,
        attemptCount,
        isLastAttempt,
        error: (error as Error).message,
      });

      if (isLastAttempt || onFailure !== "retry") {
        throw error;
      }

      // Retry on error if configured
      const delayMs = mainLoopExponentialBackoff
        ? Math.min(mainLoopRetryDelay * Math.pow(2, attemptCount), 60000)
        : mainLoopRetryDelay;

      mainLoopRetryDelays.push(delayMs);
      mainLoopRetryCount++;

      logger.info("Agent loop execution errored, retrying main loop", {
        agentLoopId,
        attemptCount,
        delayMs,
        nextAttempt: attemptCount + 2,
      });

      // Check retry budget and consume in single operation
      if (retryBudget) {
        const budgetCheck = retryBudget.consumeRetry(delayMs);
        if (!budgetCheck.allowed) {
          logger.warn("Retry budget would be exceeded by main loop retry delay", {
            agentLoopId,
            attemptCount,
            delayMs,
            reason: budgetCheck.reason,
          });
          throw error;
        }
      }

      await delay(delayMs);
    }
  }

  // Return error enriched with statistics
  return enrichResultWithMainLoopStats(lastError, mainLoopRetryCount, mainLoopRetryDelays);
}

/**
 * Enrich result with main loop retry statistics
 */
function enrichResultWithMainLoopStats(
  result: any,
  mainLoopRetryCount: number,
  mainLoopRetryDelays: number[],
): AgentLoopResult {
  const mainLoopRetryDelayTime = mainLoopRetryDelays.reduce((a, b) => a + b, 0);
  const iterationLevelRetryCount = result.totalRetryCount ?? 0;
  const iterationLevelRetryDelayTime = result.totalRetryDelayTime ?? 0;

  return {
    ...result,
    mainLoopRetryCount,
    mainLoopRetryDelayTime,
    iterationLevelRetryCount,
    iterationLevelRetryDelayTime,
    totalRetryCount: iterationLevelRetryCount + mainLoopRetryCount,
    totalRetryDelayTime: iterationLevelRetryDelayTime + mainLoopRetryDelayTime,
  };
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
    const config = entity.config;
    const onFailure = config.onFailure ?? "fail";
    const maxMainLoopRetries = config.maxMainLoopRetries ?? (onFailure === "retry" ? 1 : 0);
    const mainLoopRetryDelay = config.mainLoopRetryDelay ?? 1000;
    const mainLoopExponentialBackoff = config.mainLoopExponentialBackoff ?? true;

    // Create separate retry budgets for iteration-level and main-loop retries
    // [Issue 2 Fix] Separate budgets prevent shared budget exhaustion and double-counting
    const timeBudgetMode = config.retryPolicy?.timeBudgetMode ?? "delay-only";
    const retryBudgetMs = 300000;

    const iterationRetryBudget = new RetryBudget({
      timeBudgetMs: retryBudgetMs,
      timeBudgetMode: timeBudgetMode,
      name: `iteration-${agentLoopId}`,
    });

    const mainLoopRetryBudget = new RetryBudget({
      timeBudgetMs: retryBudgetMs,
      timeBudgetMode: timeBudgetMode,
      name: `main-loop-${agentLoopId}`,
    });

    return executeAgentLoopWithFailurePolicy(
      agentLoopId,
      async () => {
        const startTime = Date.now();
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
                iterationRetryBudget,  // [Issue 2 Fix] Use iteration-level budget
              );

              if (iterationResult.interruption) {
                return {
                  success: false,
                  iterations: entity.state.currentIteration,
                  toolCallCount: entity.state.toolCallCount,
                  error: `Execution ${iterationResult.interruption}`,
                  innerErrorRecords: (typeof entity.state.getErrorRecords === 'function'
                    ? entity.state.getErrorRecords().map(r => ({
                        id: r.id,
                        timestamp: r.timestamp,
                        message: r.message,
                        errorType: r.errorType,
                        severity: r.severity,
                        iteration: r.iteration,
                        context: r.context,
                      }))
                    : undefined),
                };
              }

              if (!iterationResult.shouldContinue) {
                logger.info("Agent Loop execution completed successfully", {
                  agentLoopId,
                  iterations: entity.state.currentIteration,
                  toolCallCount: entity.state.toolCallCount,
                });

                this.recordExecutionComplete(profileId, startTime, true, entity);

                const iterationBudgetStats = iterationRetryBudget.getState();
                return {
                  success: true,
                  content: iterationResult.content,
                  iterations: entity.state.currentIteration,
                  toolCallCount: entity.state.toolCallCount,
                  completionData: iterationResult.completionData,
                  totalRetryCount: iterationBudgetStats.retriesConsumed,
                  totalRetryDelayTime: iterationBudgetStats.totalDelayConsumedMs,
                  timeoutCount: entity.state.timeoutCount,
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
            this.recordExecutionComplete(profileId, startTime, true, entity);

            const iterationBudgetStats = iterationRetryBudget.getState();
            return {
              success: true,
              iterations: entity.state.currentIteration,
              toolCallCount: entity.state.toolCallCount,
              content: "Reached maximum iterations without final answer.",
              totalRetryCount: iterationBudgetStats.retriesConsumed,
              totalRetryDelayTime: iterationBudgetStats.totalDelayConsumedMs,
              timeoutCount: entity.state.timeoutCount,
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

            this.recordExecutionComplete(profileId, startTime, false, entity);

            const iterationBudgetStats = iterationRetryBudget.getState();
            return {
              success: false,
              iterations: entity.state.currentIteration,
              toolCallCount: entity.state.toolCallCount,
              error: `Execution ${interruption.type}`,
              totalRetryCount: iterationBudgetStats.retriesConsumed,
              totalRetryDelayTime: iterationBudgetStats.totalDelayConsumedMs,
              timeoutCount: entity.state.timeoutCount,
              innerErrorRecords: (typeof entity.state.getErrorRecords === 'function'
                ? entity.state.getErrorRecords().map(r => ({
                    id: r.id,
                    timestamp: r.timestamp,
                    message: r.message,
                    errorType: r.errorType,
                    severity: r.severity,
                    iteration: r.iteration,
                    context: r.context,
                  }))
                : undefined),
            };
          }

          return result.result;
        } catch (error) {
          this.recordExecutionComplete(profileId, startTime, false, entity);

          const iterationBudgetStats = iterationRetryBudget.getState();

          // [Issue 3 Fix] Handle "continue" strategy without calling state.fail()
          // This prevents the state inconsistency where state says FAILED but result says success: true
          if (onFailure === "continue" && config.fallbackOutput) {
            logger.info("Agent loop failed, using fallback output (onFailure=continue)", {
              agentLoopId,
              iterations: entity.state.currentIteration,
              fallbackContent: config.fallbackOutput.content?.substring(0, 50),
            });

            // Record the error as a non-fatal warning instead of failing the state
            if (typeof entity.state.addErrorRecord === 'function') {
              entity.state.addErrorRecord({
                id: `error:${Date.now()}:${Math.random().toString(36).slice(2, 9)}`,
                timestamp: Date.now(),
                message: (error as Error).message,
                errorType: "execution_error",
                severity: "warning",
                iteration: entity.state.currentIteration,
                context: { operation: "agent_loop_execution" },
                isRecoverable: true,
              });
            }

            return {
              success: true,
              iterations: entity.state.currentIteration,
              toolCallCount: entity.state.toolCallCount,
              content: config.fallbackOutput.content,
              completionData: config.fallbackOutput.data ? { data: config.fallbackOutput.data } : undefined,
              totalRetryCount: iterationBudgetStats.retriesConsumed,
              totalRetryDelayTime: iterationBudgetStats.totalDelayConsumedMs,
              timeoutCount: entity.state.timeoutCount,
              fallbackCount: 1,
            };
          }

          // For "fail" and "retry" strategies, standardize the error via handleAgentError
          const standardizedError = await handleAgentError(
            entity,
            error as Error,
            "agent_loop_execution",
          );

          return {
            success: false,
            iterations: entity.state.currentIteration,
            toolCallCount: entity.state.toolCallCount,
            error: standardizedError,
            totalRetryCount: iterationBudgetStats.retriesConsumed,
            totalRetryDelayTime: iterationBudgetStats.totalDelayConsumedMs,
            timeoutCount: entity.state.timeoutCount,
            innerErrorRecords: (typeof entity.state.getErrorRecords === 'function'
              ? entity.state.getErrorRecords().map(r => ({
                  id: r.id,
                  timestamp: r.timestamp,
                  message: r.message,
                  errorType: r.errorType,
                  severity: r.severity,
                  iteration: r.iteration,
                  context: r.context,
                }))
              : undefined),
          };
        }
      },
      onFailure,
      maxMainLoopRetries,
      mainLoopRetryDelay,
      mainLoopExponentialBackoff,
      mainLoopRetryBudget,  // [Issue 2 Fix] Use main-loop budget
    );
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

      // Check if fallback is configured for continuation on failure
      const onFailure = entity.config.onFailure ?? "fail";
      if (onFailure === "continue" && entity.config.fallbackOutput) {
        logger.info("Agent loop stream failed, using fallback output", {
          agentLoopId,
          iteration: entity.state.currentIteration,
          fallbackContent: entity.config.fallbackOutput.content?.substring(0, 50),
        });

        // Emit end event with fallback output (marked as success since recovered)
        const endEvent = this.createAgentEndEvent(
          agentLoopId,
          conversationManager.getMessages(),
          entity.state.currentIteration,
          entity.state.toolCallCount,
          true,  // success=true because fallback recovered
        );
        yield endEvent;
        await this.iterationCoordinator.emitToRegistry(endEvent, entity);
        return;
      }

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

    // Create retry budget for retry operations in streaming mode
    const timeBudgetMode = config.retryPolicy?.timeBudgetMode ?? "delay-only";
    const retryBudgetMs = 300000;  // 5 minutes time budget, unlimited retry count
    const retryBudget = new RetryBudget({
      timeBudgetMs: retryBudgetMs,
      timeBudgetMode: timeBudgetMode,
      name: `agent-loop-stream-${agentLoopId}`,
    });

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
        retryBudget,
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
   * Problem #5: Supports timeBudgetMode for consistent time tracking
   */
  private async *executeIterationStreamWithRetryAndTimeout(
    agentLoopId: string,
    entity: AgentLoopEntity,
    conversationManager: ConversationSession,
    toolSchemas: ToolSchema[] | undefined,
    profileId: string,
    retryPolicy?: RetryPolicy,
    timeoutMs?: number,
    retryBudget?: RetryBudget,
  ): AsyncGenerator<AgentLoopStreamEvent, boolean> {
    const maxAttempts = retryPolicy?.enabled ? (retryPolicy.maxRetries ?? 0) + 1 : 1;
    let lastError: Error | undefined;

    for (let attemptCount = 0; attemptCount < maxAttempts; attemptCount++) {
      const attemptStartTime = Date.now();

      try {
        // Check retry budget before attempting execution
        if (retryBudget && retryBudget.isExhausted()) {
          logger.warn("Retry budget exhausted before stream attempt", {
            agentLoopId,
            attemptCount,
            budgetStats: retryBudget.getState(),
          });
          throw lastError || new Error("Retry budget exhausted for retry operations");
        }

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

        // Record execution time in budget if in total-time mode
        if (retryBudget) {
          const executionTime = Date.now() - attemptStartTime;
          retryBudget.recordExecutionTime(executionTime);
        }

        // Iteration completed successfully
        return result;
      } catch (error) {
        lastError = error as Error;
        const isLastAttempt = attemptCount === maxAttempts - 1;

        // Record execution time in budget
        if (retryBudget) {
          const executionTime = Date.now() - attemptStartTime;
          retryBudget.recordExecutionTime(executionTime);
        }

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

        // Check retry budget and consume in single operation
        if (retryBudget) {
          const budgetCheck = retryBudget.consumeRetry(delayMs);
          if (!budgetCheck.allowed) {
            logger.warn("Retry budget would be exceeded by stream retry delay, stopping retry", {
              agentLoopId,
              attemptCount,
              delayMs,
              reason: budgetCheck.reason,
              budgetStats: retryBudget.getState(),
            });
            throw lastError;
          }
        }

        logger.info("Retrying stream iteration after delay", {
          agentLoopId,
          attemptCount,
          delayMs,
          nextAttempt: attemptCount + 2,
          maxAttempts,
          budgetStats: retryBudget?.getState(),
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

  private recordExecutionComplete(
    profileId: string,
    startTime: number,
    success: boolean,
    entity?: AgentLoopEntity,
  ): void {
    if (!this.metricsRegistry) return;
    const duration = Date.now() - startTime;
    const agentCollector = this.metricsRegistry.getAgentCollector();
    if (agentCollector) {
      agentCollector.recordExecutionComplete(profileId, {
        // [Problem #9 Fix] Pass actual iteration/tool call counts from entity state
        iterations: entity?.state.currentIteration ?? 0,
        toolCallCount: entity?.state.toolCallCount ?? 0,
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
