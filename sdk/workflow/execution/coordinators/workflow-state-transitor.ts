/**
 * WorkflowStateTransitor - Workflow State Transitor
 *
 * Responsibilities:
 * - Workflow execution state transitions (atomic operations)
 * - Validation of state transitions
 * - Triggering of lifecycle events
 * - Execution of lifecycle hooks (such as clearing message storage)
 * - High-level process orchestration (execute, pause, resume, stop)
 * - Cascade operations for child workflow executions
 *
 * Design Principles:
 * - Atomic operations: Each method represents a complete state transition unit
 * - Process orchestration: Manages complex multi-step operations
 * - Delegation pattern: Coordinates multiple components
 * - WorkflowExecutionEntity encapsulation: Never directly access workflow execution data, always use WorkflowExecutionEntity methods
 */

import { StateManagementError } from "@wf-agent/types";
import type { WorkflowExecutionStatus, WorkflowExecutionResult } from "@wf-agent/types";
import type { EventRegistry } from "../../../core/registry/event-registry.js";
import type { WorkflowExecutionRegistry } from "../../stores/workflow-execution-registry.js";
import type { WorkflowExecutionEntity } from "../../entities/workflow-execution-entity.js";
import { WorkflowConversationSession } from "../../message/workflow-conversation-session.js";
import { validateTransition } from "../utils/workflow-state-validator.js";
import {
  buildWorkflowExecutionStartedEvent,
  buildWorkflowExecutionStateChangedEvent,
  buildWorkflowExecutionPausedEvent,
  buildWorkflowExecutionResumedEvent,
  buildWorkflowExecutionCompletedEvent,
  buildWorkflowExecutionFailedEvent,
  buildWorkflowExecutionCancelledEvent,
} from "../utils/event/index.js";
import { emit } from "../../../core/utils/event/event-emitter.js";
import { now, getErrorOrNew } from "@wf-agent/common-utils";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import { getContainer } from "../../../core/di/index.js";
import * as Identifiers from "../../../core/di/service-identifiers.js";
import type { AgentLoopRegistry } from "../../../agent/loop/agent-loop-registry.js";

const logger = createContextualLogger({ component: "WorkflowStateTransitor" });

/**
 * WorkflowStateTransitor - Workflow State Transitor
 *
 * Provides atomic state transition operations and high-level process orchestration
 */
export class WorkflowStateTransitor {
  constructor(
    private eventManager: EventRegistry,
    private workflowConversationSession: WorkflowConversationSession,
    private workflowExecutionRegistry: WorkflowExecutionRegistry,
  ) {}

  /**
   * Start a Workflow Execution
   *
   * @param workflowExecutionEntity Workflow execution entity instance
   * @throws ValidationError The status transition is invalid
   */
  async startWorkflowExecution(workflowExecutionEntity: WorkflowExecutionEntity): Promise<void> {
    const previousStatus = workflowExecutionEntity.getStatus();
    logger.info("Starting workflow execution", { workflowExecutionId: workflowExecutionEntity.id, previousStatus });

    validateTransition(workflowExecutionEntity.id, previousStatus, "RUNNING" as WorkflowExecutionStatus);

    workflowExecutionEntity.setStatus("RUNNING" as WorkflowExecutionStatus);

    const startedEvent = buildWorkflowExecutionStartedEvent(workflowExecutionEntity);
    await emit(this.eventManager, startedEvent);

    const stateChangedEvent = buildWorkflowExecutionStateChangedEvent(workflowExecutionEntity, previousStatus, "RUNNING");
    await emit(this.eventManager, stateChangedEvent);

    logger.info("Workflow execution started", { workflowExecutionId: workflowExecutionEntity.id, status: "RUNNING" });
  }

  /**
   * Pause Workflow Execution
   *
   * @param workflowExecutionEntity Workflow execution entity instance
   * @throws ValidationError The status transition is invalid
   */
  async pauseWorkflowExecution(workflowExecutionEntity: WorkflowExecutionEntity): Promise<void> {
    const currentStatus = workflowExecutionEntity.getStatus();
    if (currentStatus === "PAUSED") {
      logger.debug("Workflow execution already paused, skipping", { workflowExecutionId: workflowExecutionEntity.id });
      return;
    }

    logger.info("Pausing workflow execution", { workflowExecutionId: workflowExecutionEntity.id, previousStatus: currentStatus });

    validateTransition(workflowExecutionEntity.id, currentStatus, "PAUSED" as WorkflowExecutionStatus);

    workflowExecutionEntity.setStatus("PAUSED" as WorkflowExecutionStatus);

    const pausedEvent = buildWorkflowExecutionPausedEvent(workflowExecutionEntity);
    await emit(this.eventManager, pausedEvent);

    const stateChangedEvent = buildWorkflowExecutionStateChangedEvent(workflowExecutionEntity, currentStatus, "PAUSED");
    await emit(this.eventManager, stateChangedEvent);

    logger.info("Workflow execution paused", { workflowExecutionId: workflowExecutionEntity.id });
  }

  /**
   * Resume Workflow Execution
   *
   * @param workflowExecutionEntity Workflow execution entity instance
   * @throws ValidationError The state transition is invalid
   */
  async resumeWorkflowExecution(workflowExecutionEntity: WorkflowExecutionEntity): Promise<void> {
    const currentStatus = workflowExecutionEntity.getStatus();
    if (currentStatus === "RUNNING") {
      logger.debug("Workflow execution already running, skipping resume", { workflowExecutionId: workflowExecutionEntity.id });
      return;
    }

    logger.info("Resuming workflow execution", { workflowExecutionId: workflowExecutionEntity.id, previousStatus: currentStatus });

    validateTransition(workflowExecutionEntity.id, currentStatus, "RUNNING" as WorkflowExecutionStatus);

    workflowExecutionEntity.setStatus("RUNNING" as WorkflowExecutionStatus);

    const resumedEvent = buildWorkflowExecutionResumedEvent(workflowExecutionEntity);
    await emit(this.eventManager, resumedEvent);

    const stateChangedEvent = buildWorkflowExecutionStateChangedEvent(workflowExecutionEntity, currentStatus, "RUNNING");
    await emit(this.eventManager, stateChangedEvent);

    logger.info("Workflow execution resumed", { workflowExecutionId: workflowExecutionEntity.id });
  }

  /**
   * Complete Workflow Execution
   *
   * @param workflowExecutionEntity Workflow execution entity instance
   * @param result Execution result
   * @throws ValidationError The status transition is invalid
   */
  async completeWorkflowExecution(workflowExecutionEntity: WorkflowExecutionEntity, result: WorkflowExecutionResult): Promise<void> {
    const previousStatus = workflowExecutionEntity.getStatus();
    logger.info("Completing workflow execution", { workflowExecutionId: workflowExecutionEntity.id, previousStatus });

    if (previousStatus === "COMPLETED") {
      logger.debug("Workflow execution already completed, emitting event only", { workflowExecutionId: workflowExecutionEntity.id });
      if (!workflowExecutionEntity.getEndTime()) {
        workflowExecutionEntity.state.complete();
      }
      this.workflowConversationSession.cleanup();
      const completedEvent = buildWorkflowExecutionCompletedEvent(workflowExecutionEntity, result);
      await emit(this.eventManager, completedEvent);
      return;
    }

    validateTransition(workflowExecutionEntity.id, previousStatus, "COMPLETED" as WorkflowExecutionStatus);

    workflowExecutionEntity.setStatus("COMPLETED" as WorkflowExecutionStatus);
    workflowExecutionEntity.state.complete();
    this.workflowConversationSession.cleanup();

    const completedEvent = buildWorkflowExecutionCompletedEvent(workflowExecutionEntity, result);
    await emit(this.eventManager, completedEvent);

    const stateChangedEvent = buildWorkflowExecutionStateChangedEvent(
      workflowExecutionEntity,
      previousStatus,
      "COMPLETED",
    );
    await emit(this.eventManager, stateChangedEvent);

    const endTime = workflowExecutionEntity.getEndTime();
    const startTime = workflowExecutionEntity.getStartTime();
    logger.info("Workflow execution completed", {
      workflowExecutionId: workflowExecutionEntity.id,
      executionTime: endTime && startTime ? endTime - startTime : 0,
    });
  }

  /**
   * Fail Workflow Execution
   *
   * @param workflowExecutionEntity Workflow execution entity instance
   * @param error Error message
   * @throws ValidationError The status transition is invalid
   */
  async failWorkflowExecution(workflowExecutionEntity: WorkflowExecutionEntity, error: Error): Promise<void> {
    const previousStatus = workflowExecutionEntity.getStatus();
    logger.info("Failing workflow execution", {
      workflowExecutionId: workflowExecutionEntity.id,
      previousStatus,
      errorMessage: error.message,
    });

    validateTransition(workflowExecutionEntity.id, previousStatus, "FAILED" as WorkflowExecutionStatus);

    workflowExecutionEntity.setStatus("FAILED" as WorkflowExecutionStatus);
    workflowExecutionEntity.state.fail(error);
    workflowExecutionEntity.getErrors().push(error.message);
    this.workflowConversationSession.cleanup();

    const failedEvent = buildWorkflowExecutionFailedEvent({ executionId: workflowExecutionEntity.id, error });
    await emit(this.eventManager, failedEvent);

    const stateChangedEvent = buildWorkflowExecutionStateChangedEvent(workflowExecutionEntity, previousStatus, "FAILED");
    await emit(this.eventManager, stateChangedEvent);

    const endTime = workflowExecutionEntity.getEndTime();
    const startTime = workflowExecutionEntity.getStartTime();
    logger.info("Workflow execution failed", {
      workflowExecutionId: workflowExecutionEntity.id,
      executionTime: endTime && startTime ? endTime - startTime : 0,
    });
  }

  /**
   * Cancel Workflow Execution
   *
   * @param workflowExecutionEntity Workflow execution entity instance
   * @param reason Reason for cancellation
   * @throws ValidationError Illegal state transition
   */
  async cancelWorkflowExecution(workflowExecutionEntity: WorkflowExecutionEntity, reason?: string): Promise<void> {
    const currentStatus = workflowExecutionEntity.getStatus();
    if (currentStatus === "CANCELLED") {
      logger.debug("Workflow execution already cancelled, skipping", { workflowExecutionId: workflowExecutionEntity.id });
      return;
    }

    logger.info("Cancelling workflow execution", {
      workflowExecutionId: workflowExecutionEntity.id,
      previousStatus: currentStatus,
      reason,
    });

    validateTransition(workflowExecutionEntity.id, currentStatus, "CANCELLED" as WorkflowExecutionStatus);

    workflowExecutionEntity.setStatus("CANCELLED" as WorkflowExecutionStatus);
    workflowExecutionEntity.state.cancel();
    this.workflowConversationSession.cleanup();

    const cancelledEvent = buildWorkflowExecutionCancelledEvent(workflowExecutionEntity, reason);
    await emit(this.eventManager, cancelledEvent);

    const stateChangedEvent = buildWorkflowExecutionStateChangedEvent(
      workflowExecutionEntity,
      currentStatus,
      "CANCELLED",
    );
    await emit(this.eventManager, stateChangedEvent);

    logger.info("Workflow execution cancelled", { workflowExecutionId: workflowExecutionEntity.id, reason });
  }

  /**
   * Cascade cancellation of all child workflow executions
   *
   * @param parentExecutionId Parent workflow execution ID
   * @returns Number of child workflow executions that were canceled
   */
  async cascadeCancel(parentExecutionId: string): Promise<number> {
    const parentContext = this.workflowExecutionRegistry.get(parentExecutionId);
    if (!parentContext) {
      return 0;
    }

    const childExecutionIds = parentContext.getChildExecutionIds();
    if (childExecutionIds.length === 0) {
      return 0;
    }

    let cancelledCount = 0;

    for (const childExecutionId of childExecutionIds) {
      try {
        const success = await this.cancelChildWorkflowExecution(childExecutionId);
        if (success) {
          cancelledCount++;
        }
      } catch (error) {
        throw new StateManagementError(
          `Failed to cancel child workflow execution ${childExecutionId}`,
          "workflowExecution",
          "delete",
          childExecutionId,
          undefined,
          undefined,
          { childExecutionId, originalError: getErrorOrNew(error) },
        );
      }
    }

    return cancelledCount;
  }

  /**
   * Cancel a single child workflow execution
   *
   * @param childExecutionId The ID of the child workflow execution
   * @returns Whether the cancellation was successful
   * @private
   */
  private async cancelChildWorkflowExecution(childExecutionId: string): Promise<boolean> {
    const childContext = this.workflowExecutionRegistry.get(childExecutionId);
    if (!childContext) {
      return false;
    }

    const childStatus = childContext.getStatus();

    if (childStatus === "RUNNING" || childStatus === "PAUSED") {
      await this.cancelWorkflowExecution(childContext, "parent_cancelled");
      return true;
    }

    return false;
  }

  /**
   * Cleanup child AgentLoops associated with a workflow execution
   * @param executionId Workflow Execution ID
   */
  async cleanupChildAgentLoops(executionId: string): Promise<void> {
    try {
      const container = getContainer();
      const agentLoopRegistry = container.get(Identifiers.AgentLoopRegistry) as AgentLoopRegistry;

      if (agentLoopRegistry) {
        const cleanedCount = agentLoopRegistry.cleanupByParentWorkflowExecutionId(executionId);
        if (cleanedCount > 0) {
          logger.info("Cleaned up child AgentLoops", {
            executionId,
            cleanedCount,
          });
        }
      }
    } catch (error) {
      logger.warn("Failed to cleanup child AgentLoops", {
        executionId,
        error: error instanceof Error ? error.message : String(error),
      });
    }
  }

  /**
   * Get the status of all child workflow executions
   *
   * @param parentExecutionId Parent workflow execution ID
   * @returns Map of child workflow execution statuses
   */
  getChildExecutionsStatus(parentExecutionId: string): Map<string, string> {
    const parentContext = this.workflowExecutionRegistry.get(parentExecutionId);
    if (!parentContext) {
      return new Map();
    }

    const childExecutionIds = parentContext.getChildExecutionIds();
    const statusMap = new Map<string, string>();

    for (const childExecutionId of childExecutionIds) {
      const childContext = this.workflowExecutionRegistry.get(childExecutionId);
      if (childContext) {
        statusMap.set(childExecutionId, childContext.getStatus());
      }
    }

    return statusMap;
  }

  /**
   * Check if there are any active child workflow executions
   *
   * @param parentExecutionId Parent workflow execution ID
   * @returns Whether there are any active child workflow executions
   */
  hasActiveChildExecutions(parentExecutionId: string): boolean {
    const statusMap = this.getChildExecutionsStatus(parentExecutionId);

    for (const status of statusMap.values()) {
      if (status === "RUNNING" || status === "PAUSED") {
        return true;
      }
    }

    return false;
  }

  /**
   * Wait for all child workflow executions to complete (event-driven)
   *
   * @param parentExecutionId Parent workflow execution ID
   * @param timeout Timeout in milliseconds
   * @returns Whether all child workflow executions completed successfully
   */
  async waitForChildExecutionsCompletion(
    parentExecutionId: string,
    timeout: number = 30000,
  ): Promise<boolean> {
    const parentContext = this.workflowExecutionRegistry.get(parentExecutionId);
    if (!parentContext) {
      return false;
    }

    const childExecutionIds = parentContext.getChildExecutionIds();
    if (childExecutionIds.length === 0) {
      return true;
    }

    const completionPromises = childExecutionIds.map((childExecutionId: string) => {
      return this.waitForChildExecutionCompletion(childExecutionId, timeout);
    });

    try {
      await Promise.all(completionPromises);
      return true;
    } catch {
      return false;
    }
  }

  /**
   * Wait for a single child workflow execution to complete
   *
   * @param childExecutionId The ID of the child workflow execution
   * @param timeout Timeout in milliseconds
   * @returns Promise that resolves when the workflow execution completes
   * @private
   */
  private async waitForChildExecutionCompletion(
    childExecutionId: string,
    timeout: number,
  ): Promise<void> {
    return new Promise((resolve, reject) => {
      const checkInterval = setInterval(() => {
        const childContext = this.workflowExecutionRegistry.get(childExecutionId);
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
        reject(new Error(`Timeout waiting for child workflow execution ${childExecutionId}`));
      }, timeout);
    });
  }

  /**
   * Check if a status is terminal
   *
   * @param status Workflow execution status
   * @returns Whether the status is terminal
   * @private
   */
  private isTerminalStatus(status: string): boolean {
    return status === "COMPLETED" || status === "FAILED" || status === "CANCELLED";
  }

  /**
   * Get the depth of the workflow execution tree
   *
   * @param executionId Workflow execution ID
   * @returns Depth of the workflow execution tree
   */
  getWorkflowExecutionTreeDepth(executionId: string): number {
    const context = this.workflowExecutionRegistry.get(executionId);
    if (!context) {
      return 0;
    }

    const parentExecutionId = context.getParentExecutionId();
    if (!parentExecutionId) {
      return 1;
    }

    return 1 + this.getWorkflowExecutionTreeDepth(parentExecutionId);
  }

  /**
   * Get all descendant workflow execution IDs
   *
   * @param executionId Workflow execution ID
   * @returns Array of all descendant workflow execution IDs
   */
  getAllDescendantExecutionIds(executionId: string): string[] {
    const context = this.workflowExecutionRegistry.get(executionId);
    if (!context) {
      return [];
    }

    const childExecutionIds = context.getChildExecutionIds();
    const allDescendants: string[] = [...childExecutionIds];

    for (const childExecutionId of childExecutionIds) {
      const childDescendants = this.getAllDescendantExecutionIds(childExecutionId);
      allDescendants.push(...childDescendants);
    }

    return allDescendants;
  }
}
