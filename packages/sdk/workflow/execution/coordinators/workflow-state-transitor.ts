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
import type { EventRegistry } from "../../../shared/registry/event-registry.js";
import type { WorkflowExecutionRegistry } from "../../registry/workflow-execution-registry.js";
import type { WorkflowExecutionEntity } from "../../entities/workflow-execution-entity.js";
import type { ConversationSession } from "../../../shared/messaging/conversation-session.js";
import { validateTransition } from "../utils/workflow-state-validator.js";
import {
  buildWorkflowExecutionStartedEvent,
  buildWorkflowExecutionStateChangedEvent,
  buildWorkflowExecutionPausedEvent,
  buildWorkflowExecutionResumedEvent,
  buildWorkflowExecutionCompletedEvent,
  buildWorkflowExecutionFailedEvent,
  buildWorkflowExecutionCancelledEvent,
} from "../../../shared/events/builders/index.js";
import { emit } from "../../../shared/events/emit-event.js";
import { getErrorOrNew } from "@wf-agent/common-utils";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import type { GlobalContext } from "../../../shared/global-context.js";
import * as Identifiers from "../../../di/service-identifiers.js";
import type { ExecutionHierarchyRegistry } from "../../../shared/registry/execution-hierarchy-registry.js";
import { waitForMultipleWorkflowExecutionsCompleted } from "../utils/index.js";
import { executeWithSharedTimeout, isTimeoutError } from "../../../shared/utils/timeout/index.js";
import { mergeTimeoutWithDefaults } from "../../../api/shared/config/index.js";

const logger = createContextualLogger({ component: "WorkflowStateTransitor" });

// Default timeout values for state transitor operations
const DEFAULT_TIMEOUT_CONFIG = mergeTimeoutWithDefaults({});

/**
 * WorkflowStateTransitor - Workflow State Transitor
 *
 * Provides atomic state transition operations and high-level process orchestration
 */
export class WorkflowStateTransitor {
  constructor(
    private eventManager: EventRegistry,
    private workflowConversationSession: ConversationSession,
    private workflowExecutionRegistry: WorkflowExecutionRegistry,
    private readonly globalContext: GlobalContext,
  ) {}

  /**
   * Start a Workflow Execution
   *
   * @param workflowExecutionEntity Workflow execution entity instance
   * @throws ValidationError The status transition is invalid
   */
  async startWorkflowExecution(workflowExecutionEntity: WorkflowExecutionEntity): Promise<void> {
    const previousStatus = workflowExecutionEntity.getStatus();
    logger.info("Starting workflow execution", {
      workflowExecutionId: workflowExecutionEntity.id,
      previousStatus,
    });

    validateTransition(
      workflowExecutionEntity.id,
      previousStatus,
      "RUNNING" as WorkflowExecutionStatus,
    );

    workflowExecutionEntity.state.start();

    const startedEvent = buildWorkflowExecutionStartedEvent(workflowExecutionEntity);
    await emit(this.eventManager, startedEvent);

    const stateChangedEvent = buildWorkflowExecutionStateChangedEvent(
      workflowExecutionEntity,
      previousStatus,
      "RUNNING",
    );
    await emit(this.eventManager, stateChangedEvent);

    logger.info("Workflow execution started", {
      workflowExecutionId: workflowExecutionEntity.id,
      status: "RUNNING",
    });
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
      logger.debug("Workflow execution already paused, skipping", {
        workflowExecutionId: workflowExecutionEntity.id,
      });
      return;
    }

    logger.info("Pausing workflow execution", {
      workflowExecutionId: workflowExecutionEntity.id,
      previousStatus: currentStatus,
    });

    validateTransition(
      workflowExecutionEntity.id,
      currentStatus,
      "PAUSED" as WorkflowExecutionStatus,
    );

    workflowExecutionEntity.state.pause();

    const pausedEvent = buildWorkflowExecutionPausedEvent(workflowExecutionEntity);
    await emit(this.eventManager, pausedEvent);

    const stateChangedEvent = buildWorkflowExecutionStateChangedEvent(
      workflowExecutionEntity,
      currentStatus,
      "PAUSED",
    );
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
      logger.debug("Workflow execution already running, skipping resume", {
        workflowExecutionId: workflowExecutionEntity.id,
      });
      return;
    }

    logger.info("Resuming workflow execution", {
      workflowExecutionId: workflowExecutionEntity.id,
      previousStatus: currentStatus,
    });

    validateTransition(
      workflowExecutionEntity.id,
      currentStatus,
      "RUNNING" as WorkflowExecutionStatus,
    );

    workflowExecutionEntity.state.resume();

    const resumedEvent = buildWorkflowExecutionResumedEvent(workflowExecutionEntity);
    await emit(this.eventManager, resumedEvent);

    const stateChangedEvent = buildWorkflowExecutionStateChangedEvent(
      workflowExecutionEntity,
      currentStatus,
      "RUNNING",
    );
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
  async completeWorkflowExecution(
    workflowExecutionEntity: WorkflowExecutionEntity,
    result: WorkflowExecutionResult,
  ): Promise<void> {
    const previousStatus = workflowExecutionEntity.getStatus();
    logger.info("Completing workflow execution", {
      workflowExecutionId: workflowExecutionEntity.id,
      previousStatus,
    });

    if (previousStatus === "COMPLETED") {
      logger.debug("Workflow execution already completed, emitting event only", {
        workflowExecutionId: workflowExecutionEntity.id,
      });
      if (!workflowExecutionEntity.getEndTime()) {
        workflowExecutionEntity.state.complete();
      }
      this.workflowConversationSession.cleanup();
      const completedEvent = buildWorkflowExecutionCompletedEvent(workflowExecutionEntity, result);
      await emit(this.eventManager, completedEvent);
      return;
    }

    validateTransition(
      workflowExecutionEntity.id,
      previousStatus,
      "COMPLETED" as WorkflowExecutionStatus,
    );

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
  async failWorkflowExecution(
    workflowExecutionEntity: WorkflowExecutionEntity,
    error: Error,
  ): Promise<void> {
    const previousStatus = workflowExecutionEntity.getStatus();
    logger.info("Failing workflow execution", {
      workflowExecutionId: workflowExecutionEntity.id,
      previousStatus,
      errorMessage: error.message,
    });

    validateTransition(
      workflowExecutionEntity.id,
      previousStatus,
      "FAILED" as WorkflowExecutionStatus,
    );

    workflowExecutionEntity.state.fail(error);
    workflowExecutionEntity.getErrors().push(error.message);
    this.workflowConversationSession.cleanup();

    const failedEvent = buildWorkflowExecutionFailedEvent({
      executionId: workflowExecutionEntity.id,
      error,
    });
    await emit(this.eventManager, failedEvent);

    const stateChangedEvent = buildWorkflowExecutionStateChangedEvent(
      workflowExecutionEntity,
      previousStatus,
      "FAILED",
    );
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
  async cancelWorkflowExecution(
    workflowExecutionEntity: WorkflowExecutionEntity,
    reason?: string,
  ): Promise<void> {
    const currentStatus = workflowExecutionEntity.getStatus();
    if (currentStatus === "CANCELLED") {
      logger.debug("Workflow execution already cancelled, skipping", {
        workflowExecutionId: workflowExecutionEntity.id,
      });
      return;
    }

    logger.info("Cancelling workflow execution", {
      workflowExecutionId: workflowExecutionEntity.id,
      previousStatus: currentStatus,
      reason,
    });

    validateTransition(
      workflowExecutionEntity.id,
      currentStatus,
      "CANCELLED" as WorkflowExecutionStatus,
    );

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

    logger.info("Workflow execution cancelled", {
      workflowExecutionId: workflowExecutionEntity.id,
      reason,
    });
  }

  /**
   * Cascade cancellation of all child workflow executions
   *
   * @param parentExecutionId Parent workflow execution ID
   * @param options Optional configuration for timeout and strategy
   * @returns Number of child workflow executions that were canceled
   */
  async cascadeCancel(
    parentExecutionId: string,
    options?: {
      timeout?: number; // Overall timeout in milliseconds (default: 30000)
      strategy?: "sequential" | "parallel"; // Cancellation strategy (default: 'parallel')
    },
  ): Promise<number> {
    const parentContext = this.workflowExecutionRegistry.get(parentExecutionId);
    if (!parentContext) {
      return 0;
    }

    const childExecutionIds = parentContext.getChildExecutionIds();
    if (childExecutionIds.length === 0) {
      return 0;
    }

    const timeout = options?.timeout ?? DEFAULT_TIMEOUT_CONFIG.cascadeCancel;
    const strategy = options?.strategy ?? "parallel";

    logger.debug("Starting cascade cancel", {
      parentExecutionId,
      childCount: childExecutionIds.length,
      timeout,
      strategy,
    });

    try {
      if (strategy === "parallel") {
        // Parallel cancellation with shared timeout
        const cancelOperations: Record<string, () => Promise<boolean>> = {};
        for (const childExecutionId of childExecutionIds) {
          cancelOperations[childExecutionId] = () =>
            this.cancelChildWorkflowExecution(childExecutionId);
        }

        const results = await executeWithSharedTimeout(cancelOperations, timeout, {
          message: `Cascade cancel timed out for parent: ${parentExecutionId}`,
        });

        const cancelledCount = Array.from(results.values()).filter(Boolean).length;
        logger.info("Parallel cascade cancel completed", {
          parentExecutionId,
          totalChildren: childExecutionIds.length,
          cancelledCount,
        });
        return cancelledCount;
      } else {
        // Sequential cancellation with per-operation timeout
        let cancelledCount = 0;
        const perOperationTimeout = timeout / childExecutionIds.length;

        for (const childExecutionId of childExecutionIds) {
          try {
            const success = await executeWithSharedTimeout(
              { cancel: () => this.cancelChildWorkflowExecution(childExecutionId) },
              perOperationTimeout,
              { message: `Cancel child ${childExecutionId} timed out` },
            );

            if (success.get("cancel")) {
              cancelledCount++;
            }
          } catch (error) {
            if (isTimeoutError(error)) {
              logger.warn("Individual child cancel timed out", {
                childExecutionId,
                perOperationTimeout,
              });
              // Continue with next child instead of throwing
            } else {
              throw error;
            }
          }
        }

        logger.info("Sequential cascade cancel completed", {
          parentExecutionId,
          totalChildren: childExecutionIds.length,
          cancelledCount,
        });
        return cancelledCount;
      }
    } catch (error) {
      if (isTimeoutError(error)) {
        logger.warn("Cascade cancel timed out", {
          parentExecutionId,
          timeout,
          error: error instanceof Error ? error.message : String(error),
        });
        // Return partial success count instead of throwing
        // Note: In parallel mode, we may have already cancelled some children
        return 0; // Conservative estimate
      }

      // Re-throw non-timeout errors
      throw new StateManagementError(
        `Failed to cascade cancel child workflow executions`,
        "workflowExecution",
        "delete",
        parentExecutionId,
        undefined,
        undefined,
        { parentExecutionId, originalError: getErrorOrNew(error) },
      );
    }
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
      // Use unified hierarchy registry for cleanup
      const hierarchyRegistry = this.globalContext.container.get(
        Identifiers.ExecutionHierarchyRegistry,
      ) as ExecutionHierarchyRegistry;

      if (hierarchyRegistry) {
        // Use unified cleanup that handles mixed hierarchies (Workflow → Agent, Agent → Agent, etc.)
        const cleanedCount = hierarchyRegistry.cleanupHierarchy(executionId);
        if (cleanedCount > 0) {
          logger.info("Cleaned up execution hierarchy using unified registry", {
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
    timeout: number = DEFAULT_TIMEOUT_CONFIG.childExecutionWait,
  ): Promise<boolean> {
    const parentContext = this.workflowExecutionRegistry.get(parentExecutionId);
    if (!parentContext) {
      return false;
    }

    const childExecutionIds = parentContext.getChildExecutionIds();
    if (childExecutionIds.length === 0) {
      return true;
    }

    // Get event manager from global context
    const eventManager = this.globalContext.container.get(
      Identifiers.EventRegistry,
    ) as EventRegistry;
    if (!eventManager) {
      logger.error("EventRegistry not available for waiting", { parentExecutionId });
      return false;
    }

    try {
      // Use event-driven waiting with shared timeout
      await executeWithSharedTimeout(
        {
          wait: () =>
            waitForMultipleWorkflowExecutionsCompleted(
              eventManager,
              childExecutionIds,
              undefined, // No individual timeout, use shared timeout
              { timeoutMode: "shared" }, // Use shared timeout mode
            ),
        },
        timeout,
        { message: `Timeout waiting for all child executions of ${parentExecutionId}` },
      );
      return true;
    } catch (error) {
      if (isTimeoutError(error)) {
        logger.warn("Timeout waiting for child executions", {
          parentExecutionId,
          childExecutionIds,
          timeout,
        });
        return false;
      }
      logger.error("Error waiting for child executions", {
        parentExecutionId,
        error: error instanceof Error ? error.message : String(error),
      });
      return false;
    }
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

    // Use new unified hierarchy API
    return context.getHierarchyDepth() + 1; // Convert from 0-based to 1-based depth
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
