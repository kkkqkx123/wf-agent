/**
 * ThreadStateTransitor - Thread State Transitor
 *
 * Responsibilities:
 * - Thread state transitions (atomic operations)
 * - Validation of state transitions
 * - Triggering of lifecycle events
 * - Execution of lifecycle hooks (such as clearing message storage)
 * - High-level process orchestration (execute, pause, resume, stop)
 * - Cascade operations for child threads
 *
 * Design Principles:
 * - Atomic operations: Each method represents a complete state transition unit
 * - Process orchestration: Manages complex multi-step operations
 * - Delegation pattern: Coordinates multiple components
 * - ThreadEntity encapsulation: Never directly access Thread data, always use ThreadEntity methods
 */

import { StateManagementError } from "@wf-agent/types";
import type { ThreadStatus, ThreadResult } from "@wf-agent/types";
import type { EventRegistry } from "../../../core/registry/event-registry.js";
import type { ThreadRegistry } from "../../stores/thread-registry.js";
import type { TaskRegistry } from "../../stores/task/task-registry.js";
import type { ThreadEntity } from "../../entities/thread-entity.js";
import { GraphConversationSession } from "../../message/graph-conversation-session.js";
import { validateTransition } from "../utils/thread-state-validator.js";
import {
  buildThreadStartedEvent,
  buildThreadStateChangedEvent,
  buildThreadPausedEvent,
  buildThreadResumedEvent,
  buildThreadCompletedEvent,
  buildThreadFailedEvent,
  buildThreadCancelledEvent,
} from "../utils/event/index.js";
import { emit } from "../../../core/utils/event/event-emitter.js";
import { now, getErrorOrNew } from "@wf-agent/common-utils";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import { getContainer } from "../../../core/di/index.js";
import * as Identifiers from "../../../core/di/service-identifiers.js";
import type { AgentLoopRegistry } from "../../../agent/loop/agent-loop-registry.js";

const logger = createContextualLogger({ component: "ThreadStateTransitor" });

/**
 * ThreadStateTransitor - Thread State Transitor
 *
 * Provides atomic state transition operations and high-level process orchestration
 */
export class ThreadStateTransitor {
  constructor(
    private eventManager: EventRegistry,
    private graphConversationSession: GraphConversationSession,
    private threadRegistry: ThreadRegistry,
    private taskRegistry: TaskRegistry,
  ) {}

  /**
   * Start a Thread
   *
   * @param threadEntity Thread entity instance
   * @throws ValidationError The status transition is invalid
   */
  async startThread(threadEntity: ThreadEntity): Promise<void> {
    const previousStatus = threadEntity.getStatus();
    logger.info("Starting thread", { threadId: threadEntity.id, previousStatus });

    validateTransition(threadEntity.id, previousStatus, "RUNNING" as ThreadStatus);

    threadEntity.setStatus("RUNNING" as ThreadStatus);

    const startedEvent = buildThreadStartedEvent(threadEntity);
    await emit(this.eventManager, startedEvent);

    const stateChangedEvent = buildThreadStateChangedEvent(threadEntity, previousStatus, "RUNNING");
    await emit(this.eventManager, stateChangedEvent);

    logger.info("Thread started", { threadId: threadEntity.id, status: "RUNNING" });
  }

  /**
   * Pause Thread
   *
   * @param threadEntity Thread entity instance
   * @throws ValidationError The status transition is invalid
   */
  async pauseThread(threadEntity: ThreadEntity): Promise<void> {
    const currentStatus = threadEntity.getStatus();
    if (currentStatus === "PAUSED") {
      logger.debug("Thread already paused, skipping", { threadId: threadEntity.id });
      return;
    }

    logger.info("Pausing thread", { threadId: threadEntity.id, previousStatus: currentStatus });

    validateTransition(threadEntity.id, currentStatus, "PAUSED" as ThreadStatus);

    threadEntity.setStatus("PAUSED" as ThreadStatus);

    const pausedEvent = buildThreadPausedEvent(threadEntity);
    await emit(this.eventManager, pausedEvent);

    const stateChangedEvent = buildThreadStateChangedEvent(threadEntity, currentStatus, "PAUSED");
    await emit(this.eventManager, stateChangedEvent);

    logger.info("Thread paused", { threadId: threadEntity.id });
  }

  /**
   * Recover Thread
   *
   * @param threadEntity Thread entity instance
   * @throws ValidationError The state transition is invalid
   */
  async resumeThread(threadEntity: ThreadEntity): Promise<void> {
    const currentStatus = threadEntity.getStatus();
    if (currentStatus === "RUNNING") {
      logger.debug("Thread already running, skipping resume", { threadId: threadEntity.id });
      return;
    }

    logger.info("Resuming thread", { threadId: threadEntity.id, previousStatus: currentStatus });

    validateTransition(threadEntity.id, currentStatus, "RUNNING" as ThreadStatus);

    threadEntity.setStatus("RUNNING" as ThreadStatus);

    const resumedEvent = buildThreadResumedEvent(threadEntity);
    await emit(this.eventManager, resumedEvent);

    const stateChangedEvent = buildThreadStateChangedEvent(threadEntity, currentStatus, "RUNNING");
    await emit(this.eventManager, stateChangedEvent);

    logger.info("Thread resumed", { threadId: threadEntity.id });
  }

  /**
   * Complete Thread
   *
   * @param threadEntity Thread entity instance
   * @param result Execution result
   * @throws ValidationError The status transition is invalid
   */
  async completeThread(threadEntity: ThreadEntity, result: ThreadResult): Promise<void> {
    const previousStatus = threadEntity.getStatus();
    logger.info("Completing thread", { threadId: threadEntity.id, previousStatus });

    if (previousStatus === "COMPLETED") {
      logger.debug("Thread already completed, emitting event only", { threadId: threadEntity.id });
      if (!threadEntity.getEndTime()) {
        threadEntity.state.complete();
      }
      this.graphConversationSession.cleanup();
      const completedEvent = buildThreadCompletedEvent(threadEntity, result);
      await emit(this.eventManager, completedEvent);
      return;
    }

    validateTransition(threadEntity.id, previousStatus, "COMPLETED" as ThreadStatus);

    threadEntity.setStatus("COMPLETED" as ThreadStatus);
    threadEntity.state.complete();
    this.graphConversationSession.cleanup();

    const completedEvent = buildThreadCompletedEvent(threadEntity, result);
    await emit(this.eventManager, completedEvent);

    const stateChangedEvent = buildThreadStateChangedEvent(
      threadEntity,
      previousStatus,
      "COMPLETED",
    );
    await emit(this.eventManager, stateChangedEvent);

    const endTime = threadEntity.getEndTime();
    const startTime = threadEntity.getStartTime();
    logger.info("Thread completed", {
      threadId: threadEntity.id,
      executionTime: endTime && startTime ? endTime - startTime : 0,
    });
  }

  /**
   * Failed Thread
   *
   * @param threadEntity Thread entity instance
   * @param error Error message
   * @throws ValidationError The status transition is invalid
   */
  async failThread(threadEntity: ThreadEntity, error: Error): Promise<void> {
    const previousStatus = threadEntity.getStatus();
    logger.info("Failing thread", {
      threadId: threadEntity.id,
      previousStatus,
      errorMessage: error.message,
    });

    validateTransition(threadEntity.id, previousStatus, "FAILED" as ThreadStatus);

    threadEntity.setStatus("FAILED" as ThreadStatus);
    threadEntity.state.fail(error);
    threadEntity.getErrors().push(error.message);
    this.graphConversationSession.cleanup();

    const failedEvent = buildThreadFailedEvent({ threadId: threadEntity.id, error });
    await emit(this.eventManager, failedEvent);

    const stateChangedEvent = buildThreadStateChangedEvent(threadEntity, previousStatus, "FAILED");
    await emit(this.eventManager, stateChangedEvent);

    const endTime = threadEntity.getEndTime();
    const startTime = threadEntity.getStartTime();
    logger.info("Thread failed", {
      threadId: threadEntity.id,
      executionTime: endTime && startTime ? endTime - startTime : 0,
    });
  }

  /**
   * Cancel Thread
   *
   * @param threadEntity Thread entity instance
   * @param reason Reason for cancellation
   * @throws ValidationError Illegal state transition
   */
  async cancelThread(threadEntity: ThreadEntity, reason?: string): Promise<void> {
    const currentStatus = threadEntity.getStatus();
    if (currentStatus === "CANCELLED") {
      logger.debug("Thread already cancelled, skipping", { threadId: threadEntity.id });
      return;
    }

    logger.info("Cancelling thread", {
      threadId: threadEntity.id,
      previousStatus: currentStatus,
      reason,
    });

    validateTransition(threadEntity.id, currentStatus, "CANCELLED" as ThreadStatus);

    threadEntity.setStatus("CANCELLED" as ThreadStatus);
    threadEntity.state.cancel();
    this.graphConversationSession.cleanup();

    const cancelledEvent = buildThreadCancelledEvent(threadEntity, reason);
    await emit(this.eventManager, cancelledEvent);

    const stateChangedEvent = buildThreadStateChangedEvent(
      threadEntity,
      currentStatus,
      "CANCELLED",
    );
    await emit(this.eventManager, stateChangedEvent);

    logger.info("Thread cancelled", { threadId: threadEntity.id, reason });
  }

  /**
   * Cascade cancellation of all child threads
   *
   * @param parentThreadId Parent thread ID
   * @returns Number of child threads that were canceled
   */
  async cascadeCancel(parentThreadId: string): Promise<number> {
    const parentContext = this.threadRegistry.get(parentThreadId);
    if (!parentContext) {
      return 0;
    }

    const childThreadIds = parentContext.getChildThreadIds();
    if (childThreadIds.length === 0) {
      return 0;
    }

    let cancelledCount = 0;

    for (const childThreadId of childThreadIds) {
      try {
        const success = await this.cancelChildThread(childThreadId);
        if (success) {
          cancelledCount++;
        }
      } catch (error) {
        throw new StateManagementError(
          `Failed to cancel child thread ${childThreadId}`,
          "thread",
          "delete",
          childThreadId,
          undefined,
          undefined,
          { childThreadId, originalError: getErrorOrNew(error) },
        );
      }
    }

    return cancelledCount;
  }

  /**
   * Cancel a single child thread
   *
   * @param childThreadId The ID of the child thread
   * @returns Whether the cancellation was successful
   * @private
   */
  private async cancelChildThread(childThreadId: string): Promise<boolean> {
    const childContext = this.threadRegistry.get(childThreadId);
    if (!childContext) {
      return false;
    }

    const childStatus = childContext.getStatus();

    if (childStatus === "RUNNING" || childStatus === "PAUSED") {
      await this.cancelThread(childContext, "parent_cancelled");

      const taskInfo = this.taskRegistry.getByThreadId(childThreadId);
      if (taskInfo) {
        await this.taskRegistry.cancelTask(taskInfo.id);
      }

      return true;
    }

    return false;
  }

  /**
   * Cleanup child AgentLoops associated with a thread
   * @param threadId Thread ID
   */
  async cleanupChildAgentLoops(threadId: string): Promise<void> {
    try {
      const container = getContainer();
      const agentLoopRegistry = container.get(Identifiers.AgentLoopRegistry) as AgentLoopRegistry;

      if (agentLoopRegistry) {
        const cleanedCount = agentLoopRegistry.cleanupByParentThreadId(threadId);
        if (cleanedCount > 0) {
          logger.info("Cleaned up child AgentLoops", {
            threadId,
            cleanedCount,
          });
        }
      }
    } catch (error) {
      logger.warn("Failed to cleanup child AgentLoops", {
        threadId,
        error: error instanceof Error ? error.message : String(error),
      });
    }
  }

  /**
   * Get the status of all child threads
   *
   * @param parentThreadId Parent thread ID
   * @returns Map of child thread statuses
   */
  getChildThreadsStatus(parentThreadId: string): Map<string, string> {
    const parentContext = this.threadRegistry.get(parentThreadId);
    if (!parentContext) {
      return new Map();
    }

    const childThreadIds = parentContext.getChildThreadIds();
    const statusMap = new Map<string, string>();

    for (const childThreadId of childThreadIds) {
      const childContext = this.threadRegistry.get(childThreadId);
      if (childContext) {
        statusMap.set(childThreadId, childContext.getStatus());
      }
    }

    return statusMap;
  }

  /**
   * Check if there are any active child threads
   *
   * @param parentThreadId Parent thread ID
   * @returns Whether there are any active child threads
   */
  hasActiveChildThreads(parentThreadId: string): boolean {
    const statusMap = this.getChildThreadsStatus(parentThreadId);

    for (const status of statusMap.values()) {
      if (status === "RUNNING" || status === "PAUSED") {
        return true;
      }
    }

    return false;
  }

  /**
   * Wait for all child threads to complete (event-driven)
   *
   * @param parentThreadId Parent thread ID
   * @param timeout Timeout in milliseconds
   * @returns Whether all child threads completed successfully
   */
  async waitForChildThreadsCompletion(
    parentThreadId: string,
    timeout: number = 30000,
  ): Promise<boolean> {
    const parentContext = this.threadRegistry.get(parentThreadId);
    if (!parentContext) {
      return false;
    }

    const childThreadIds = parentContext.getChildThreadIds();
    if (childThreadIds.length === 0) {
      return true;
    }

    const completionPromises = childThreadIds.map((childThreadId: string) => {
      return this.waitForChildThreadCompletion(childThreadId, timeout);
    });

    try {
      await Promise.all(completionPromises);
      return true;
    } catch {
      return false;
    }
  }

  /**
   * Wait for a single child thread to complete
   *
   * @param childThreadId The ID of the child thread
   * @param timeout Timeout in milliseconds
   * @returns Promise that resolves when the thread completes
   * @private
   */
  private async waitForChildThreadCompletion(
    childThreadId: string,
    timeout: number,
  ): Promise<void> {
    return new Promise((resolve, reject) => {
      const checkInterval = setInterval(() => {
        const childContext = this.threadRegistry.get(childThreadId);
        if (!childContext) {
          clearInterval(checkInterval);
          resolve();
          return;
        }

        const status = childContext.getStatus();
        if (status === "COMPLETED" || status === "FAILED" || status === "CANCELLED") {
          clearInterval(checkInterval);
          resolve();
        }
      }, 100);

      setTimeout(() => {
        clearInterval(checkInterval);
        reject(new Error(`Timeout waiting for child thread ${childThreadId}`));
      }, timeout);
    });
  }

  /**
   * Check if a status is terminal
   *
   * @param status Thread status
   * @returns Whether the status is terminal
   * @private
   */
  private isTerminalStatus(status: string): boolean {
    return status === "COMPLETED" || status === "FAILED" || status === "CANCELLED";
  }

  /**
   * Get the depth of the thread tree
   *
   * @param threadId Thread ID
   * @returns Depth of the thread tree
   */
  getThreadTreeDepth(threadId: string): number {
    const context = this.threadRegistry.get(threadId);
    if (!context) {
      return 0;
    }

    const parentThreadId = context.getParentThreadId();
    if (!parentThreadId) {
      return 1;
    }

    return 1 + this.getThreadTreeDepth(parentThreadId);
  }

  /**
   * Get all descendant thread IDs
   *
   * @param threadId Thread ID
   * @returns Array of all descendant thread IDs
   */
  getAllDescendantThreadIds(threadId: string): string[] {
    const context = this.threadRegistry.get(threadId);
    if (!context) {
      return [];
    }

    const childThreadIds = context.getChildThreadIds();
    const allDescendants: string[] = [...childThreadIds];

    for (const childThreadId of childThreadIds) {
      const childDescendants = this.getAllDescendantThreadIds(childThreadId);
      allDescendants.push(...childDescendants);
    }

    return allDescendants;
  }
}
