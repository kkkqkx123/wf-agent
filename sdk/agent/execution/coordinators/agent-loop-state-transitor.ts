/**
 * AgentLoopStateTransitor - Agent Loop State Transitor
 *
 * Responsibilities:
 * - Atomic state transitions for agent loop execution
 * - Validation of state transitions
 * - Triggering of lifecycle events
 * - Execution of lifecycle hooks (such as clearing conversation session)
 *
 * Design Principles:
 * - Atomic operations: Each method represents a complete state transition unit
 * - Process orchestration: Manages complex multi-step operations
 * - Delegation pattern: Coordinates multiple components
 * - AgentLoopEntity encapsulation: Never directly access entity data, always use entity methods
 *
 * Inspired by WorkflowStateTransitor design pattern.
 */

import type { EventRegistry } from "../../../core/registry/event-registry.js";
import type { AgentLoopResult } from "@wf-agent/types";
import type { AgentLoopEntity } from "../../entities/agent-loop-entity.js";
import { AgentLoopStatus } from "@wf-agent/types";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import { emit } from "../../../core/utils/event/emit-event.js";
import {
  buildAgentStartedEvent,
  buildAgentCompletedEvent,
  buildAgentPausedEvent,
  buildAgentCancelledEvent,
  buildAgentResumedEvent,
  buildAgentFailedEvent,
} from "../../../core/utils/event/builders/agent-events.js";

const logger = createContextualLogger({ component: "AgentLoopStateTransitor" });

/**
 * AgentLoopStateTransitor - Agent Loop State Transitor
 *
 * Provides atomic state transition operations and high-level process orchestration
 */
export class AgentLoopStateTransitor {
  constructor(private eventManager: EventRegistry) {}

  /**
   * Start Agent Loop Execution
   *
   * @param entity Agent loop entity instance
   * @param messageCount Initial message count (optional, defaults to 0)
   */
  async startAgentLoop(entity: AgentLoopEntity, messageCount = 0): Promise<void> {
    const previousStatus = entity.getStatus();
    logger.info("Starting agent loop execution", {
      agentLoopId: entity.id,
      previousStatus,
    });

    // Validate transition
    if (previousStatus !== AgentLoopStatus.CREATED) {
      logger.warn("Invalid state transition", {
        agentLoopId: entity.id,
        from: previousStatus,
        to: AgentLoopStatus.RUNNING,
      });
    }

    // Update state
    entity.state.start();

    // Emit AGENT_STARTED event
    const startedEvent = buildAgentStartedEvent({
      agentLoopId: entity.id,
      maxIterations: entity.config.maxIterations ?? -1,
      initialMessageCount: messageCount,
      executionId: entity.id,
    });
    await emit(this.eventManager, startedEvent);

    logger.info("Agent loop execution started", {
      agentLoopId: entity.id,
      status: AgentLoopStatus.RUNNING,
    });
  }

  /**
   * Pause Agent Loop Execution
   *
   * @param entity Agent loop entity instance
   */
  async pauseAgentLoop(entity: AgentLoopEntity): Promise<void> {
    const currentStatus = entity.getStatus();
    if (currentStatus === AgentLoopStatus.PAUSED) {
      logger.debug("Agent loop already paused, skipping", { agentLoopId: entity.id });
      return;
    }

    logger.info("Pausing agent loop execution", {
      agentLoopId: entity.id,
      previousStatus: currentStatus,
    });

    // Update state
    entity.state.pause();

    // Emit AGENT_PAUSED event
    const pausedEvent = buildAgentPausedEvent({
      agentLoopId: entity.id,
      iteration: entity.state.currentIteration,
      toolCallCount: entity.state.toolCallCount,
      isStreaming: entity.state.isStreaming,
      pendingToolCalls: entity.state.pendingToolCalls.size,
      streamMessagePreserved: !!entity.state.streamMessage,
      executionId: entity.id,
    });
    await emit(this.eventManager, pausedEvent);

    logger.info("Agent loop execution paused", { agentLoopId: entity.id });
  }

  /**
   * Resume Agent Loop Execution
   *
   * @param entity Agent loop entity instance
   */
  async resumeAgentLoop(entity: AgentLoopEntity): Promise<void> {
    const currentStatus = entity.getStatus();
    if (currentStatus === AgentLoopStatus.RUNNING) {
      logger.debug("Agent loop already running, skipping resume", { agentLoopId: entity.id });
      return;
    }

    logger.info("Resuming agent loop execution", {
      agentLoopId: entity.id,
      previousStatus: currentStatus,
    });

    // Update state
    entity.state.resume();

    // Emit AGENT_RESUMED event
    const resumedEvent = buildAgentResumedEvent({
      agentLoopId: entity.id,
      iteration: entity.state.currentIteration,
      toolCallCount: entity.state.toolCallCount,
      executionId: entity.id,
    });
    await emit(this.eventManager, resumedEvent);

    logger.info("Agent loop execution resumed", { agentLoopId: entity.id });
  }

  /**
   * Complete Agent Loop Execution
   *
   * @param entity Agent loop entity instance
   * @param result Execution result
   */
  async completeAgentLoop(entity: AgentLoopEntity, result: AgentLoopResult): Promise<void> {
    const previousStatus = entity.getStatus();
    logger.info("Completing agent loop execution", {
      agentLoopId: entity.id,
      previousStatus,
      iterations: result.iterations,
      toolCallCount: result.toolCallCount,
    });

    // Update state
    entity.state.complete();

    // Emit AGENT_COMPLETED event (best-effort, don't throw on failure)
    try {
      const completedEvent = buildAgentCompletedEvent({
        agentLoopId: entity.id,
        iterations: result.iterations,
        toolCallCount: result.toolCallCount,
        success: true,
        executionId: entity.id,
      });
      await emit(this.eventManager, completedEvent);
    } catch (emitError) {
      logger.warn("Failed to emit AGENT_COMPLETED event", {
        agentLoopId: entity.id,
        error: emitError instanceof Error ? emitError.message : String(emitError),
      });
    }

    logger.info("Agent loop execution completed successfully", {
      agentLoopId: entity.id,
      iterations: result.iterations,
      toolCallCount: result.toolCallCount,
    });
  }

  /**
   * Fail Agent Loop Execution
   *
   * @param entity Agent loop entity instance
   * @param error Error that caused failure
   */
  async failAgentLoop(entity: AgentLoopEntity, error: Error | unknown): Promise<void> {
    const previousStatus = entity.getStatus();
    const errorMessage = error instanceof Error ? error.message : String(error);

    logger.info("Failing agent loop execution", {
      agentLoopId: entity.id,
      previousStatus,
      errorMessage,
    });

    // Update state
    entity.state.fail(error);

    // Emit AGENT_FAILED event (best-effort, don't throw on failure)
    try {
      const failedEvent = buildAgentFailedEvent({
        agentLoopId: entity.id,
        iteration: entity.state.currentIteration,
        toolCallCount: entity.state.toolCallCount,
        error:
          error instanceof Error ? { message: error.message, name: error.name } : String(error),
        executionId: entity.id,
      });
      await emit(this.eventManager, failedEvent);
    } catch (emitError) {
      logger.warn("Failed to emit AGENT_FAILED event", {
        agentLoopId: entity.id,
        error: emitError instanceof Error ? emitError.message : String(emitError),
      });
    }

    logger.info("Agent loop execution failed", {
      agentLoopId: entity.id,
      errorMessage,
    });
  }

  /**
   * Cancel Agent Loop Execution
   *
   * @param entity Agent loop entity instance
   * @param reason Reason for cancellation
   */
  async cancelAgentLoop(entity: AgentLoopEntity, reason?: string): Promise<void> {
    const currentStatus = entity.getStatus();
    if (currentStatus === AgentLoopStatus.CANCELLED) {
      logger.debug("Agent loop already cancelled, skipping", { agentLoopId: entity.id });
      return;
    }

    logger.info("Cancelling agent loop execution", {
      agentLoopId: entity.id,
      previousStatus: currentStatus,
      reason,
    });

    // Update state
    entity.state.cancel();

    // Emit AGENT_CANCELLED event
    const cancelledEvent = buildAgentCancelledEvent({
      agentLoopId: entity.id,
      iteration: entity.state.currentIteration,
      toolCallCount: entity.state.toolCallCount,
      isStreaming: entity.state.isStreaming,
      pendingToolCalls: entity.state.pendingToolCalls.size,
      reason,
      executionId: entity.id,
    });
    await emit(this.eventManager, cancelledEvent);

    logger.info("Agent loop execution cancelled", {
      agentLoopId: entity.id,
      reason,
    });
  }
}
