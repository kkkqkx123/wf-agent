/**
 * TriggeredWorkflowExecutionManager - Triggered Workflow Execution Manager (Simplified)
 *
 * Manages the execution of triggered (subworkflow) executions with resource pooling.
 * Simplified design: combines queue management and execution coordination in one place.
 *
 * Design Principles:
 * - Single responsibility: manage triggered workflow executions
 * - Simplified pending queue (FIFO, no complex state tracking)
 * - Resource limiting via WorkflowExecutionPool
 * - Direct integration with TaskRegistry (single source of truth for state)
 * - Replaces: TriggeredSubworkflowHandler (functionally absorbed)
 * - Removes: TaskQueue complexity (integrated here)
 */

import type { WorkflowExecutionEntity } from "../../entities/index.js";
import type { TaskSubmissionResult, ExecutedSubworkflowResult } from "../types/triggered-subworkflow.types.js";
import { TaskRegistry, type TaskManager } from "../../../shared/registry/task-registry.js";
import type { WorkflowExecutionPool } from "../workflow-execution-pool.js";
import type { EventRegistry } from "../../../shared/registry/event-registry.js";
import { now } from "@wf-agent/common-utils";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import {
  buildTriggeredSubgraphCompletedEvent,
  buildTriggeredSubgraphFailedEvent,
} from "../../../shared/events/builders/index.js";
import { emit } from "../../../shared/events/emit-event.js";

const logger = createContextualLogger({ component: "TriggeredWorkflowExecutionManager" });

/**
 * Triggered Execution Configuration
 */
export interface TriggeredExecutionConfig {
  /** Execution ID of the triggered execution */
  executionId: string;

  /** Parent execution entity */
  parentEntity: WorkflowExecutionEntity;

  /** Type of parent (currently WORKFLOW, future: AGENT_LOOP) */
  parentType: "WORKFLOW" | "AGENT_LOOP";

  /** Timeout in milliseconds */
  timeout?: number;

  /** Whether to wait for completion (sync vs async) */
  waitForCompletion?: boolean;

  /** Optional callback for async completion */
  onComplete?: (result: ExecutedSubworkflowResult) => Promise<void>;
  onError?: (error: Error) => Promise<void>;
}

/**
 * Simple pending task info
 */
interface PendingTaskInfo {
  taskId: string;
  entity: WorkflowExecutionEntity;
  isSync: boolean;
  resolve?: (value: ExecutedSubworkflowResult) => void;
  reject?: (reason: Error) => void;
}

/**
 * TriggeredWorkflowExecutionManager - Simplified implementation
 *
 * Combines the functionality of old TaskQueue and TriggeredWorkflowExecutionManager
 * into a single, simpler manager.
 */
export class TriggeredWorkflowExecutionManager implements TaskManager {
  private taskRegistry: TaskRegistry;
  private workflowExecutionPool: WorkflowExecutionPool;
  private eventManager: EventRegistry;

  // Simplified pending queue: just store task IDs in FIFO order
  private pendingTaskIds: string[] = [];
  private taskInfoMap: Map<string, PendingTaskInfo> = new Map();

  // Track which tasks are currently executing
  private executingTaskIds: Set<string> = new Set();

  constructor(
    taskRegistry: TaskRegistry,
    workflowExecutionPool: WorkflowExecutionPool,
    eventManager: EventRegistry,
  ) {
    this.taskRegistry = taskRegistry;
    this.workflowExecutionPool = workflowExecutionPool;
    this.eventManager = eventManager;
  }

  /**
   * Submit a triggered execution (workflow)
   * Routes to sync or async based on configuration
   *
   * @param config Triggered execution configuration
   * @param executionEntity The workflow execution entity
   * @returns Execution result (sync) or task submission result (async)
   */
  async submitTriggeredExecution(
    config: TriggeredExecutionConfig,
    executionEntity: WorkflowExecutionEntity,
  ): Promise<ExecutedSubworkflowResult | TaskSubmissionResult> {
    const executionId = config.executionId;
    const waitForCompletion = config.waitForCompletion !== false;
    const timeout = config.timeout || this.workflowExecutionPool.getConfig().defaultTimeout;

    // Register task in registry
    const taskId = this.taskRegistry.register(executionEntity, "workflowExecution", this, timeout);

    logger.debug("Triggered workflow execution registered", {
      executionId,
      taskId,
      waitForCompletion,
    });

    if (waitForCompletion) {
      // Sync execution: wait for completion
      return new Promise((resolve, reject) => {
        this.pendingTaskIds.push(taskId);
        this.taskInfoMap.set(taskId, {
          taskId,
          entity: executionEntity,
          isSync: true,
          resolve,
          reject,
        });

        // Trigger processing (non-blocking)
        this.processPendingTasks();
      });
    } else {
      // Async execution: submit and return immediately
      this.pendingTaskIds.push(taskId);
      this.taskInfoMap.set(taskId, {
        taskId,
        entity: executionEntity,
        isSync: false,
      });

      // Trigger processing (non-blocking)
      this.processPendingTasks();

      return {
        taskId,
        status: "QUEUED",
        message: "Triggered execution submitted",
        submitTime: now(),
      };
    }
  }

  /**
   * Process pending tasks sequentially, respecting pool availability
   * Non-blocking: can be called multiple times safely
   */
  private processingQueue = false;

  private async processPendingTasks(): Promise<void> {
    // Simple guard: only one processor at a time
    if (this.processingQueue) {
      return;
    }
    this.processingQueue = true;

    try {
      while (this.pendingTaskIds.length > 0) {
        const taskId = this.pendingTaskIds[0]!;
        const taskInfo = this.taskInfoMap.get(taskId);

        if (!taskInfo) {
          // Task was cancelled
          this.pendingTaskIds.shift();
          continue;
        }

        // Acquire executor from pool
        const executor = await this.workflowExecutionPool.allocateExecutor();

        // Double-check task still exists (might have been cancelled during await)
        if (!this.taskInfoMap.has(taskId)) {
          await this.workflowExecutionPool.releaseExecutor(executor);
          this.pendingTaskIds.shift();
          continue;
        }

        // Remove from pending queue
        this.pendingTaskIds.shift();

        // Mark as executing
        this.executingTaskIds.add(taskId);

        // Update status to RUNNING
        this.taskRegistry.updateStatus(taskId, "RUNNING");

        // Execute asynchronously (don't await here, let it run in background)
        this.executeTask(taskId, taskInfo, executor);
      }
    } finally {
      this.processingQueue = false;
    }
  }

  /**
   * Execute a single task
   * This runs in the background for async tasks, or resolves the promise for sync tasks
   */
  private async executeTask(
    taskId: string,
    taskInfo: PendingTaskInfo,
    executor: any,
  ): Promise<void> {
    const startTime = now();

    try {
      // Execute workflow
      const result = await executor.executeWorkflow(taskInfo.entity);

      const executionTime = now() - startTime;

      // Update status
      this.taskRegistry.updateStatus(taskId, "COMPLETED", { result });

      // Emit completion event
      await this.emitCompletionEvent(taskInfo.entity, executionTime);

      // For sync tasks, resolve the promise
      if (taskInfo.isSync && taskInfo.resolve) {
        const resolveResult: ExecutedSubworkflowResult = {
          subworkflowEntity: taskInfo.entity,
          executionResult: result,
          executionTime,
        };
        taskInfo.resolve(resolveResult);
      }

      logger.debug("Triggered workflow execution completed", {
        taskId,
        executionTime,
        executionId: taskInfo.entity.id,
      });
    } catch (error) {
      const executionTime = now() - startTime;
      const errorObj = error as Error;

      // Update status
      this.taskRegistry.updateStatus(taskId, "FAILED", { error: errorObj });

      // Emit failure event
      await this.emitFailureEvent(taskInfo.entity, errorObj);

      // For sync tasks, reject the promise
      if (taskInfo.isSync && taskInfo.reject) {
        taskInfo.reject(errorObj);
      }

      logger.error("Triggered workflow execution failed", {
        taskId,
        executionTime,
        executionId: taskInfo.entity.id,
        error: errorObj.message,
      });
    } finally {
      // Release executor back to pool
      await this.workflowExecutionPool.releaseExecutor(executor);

      // Remove from executing set
      this.executingTaskIds.delete(taskId);

      // Remove from task info map
      this.taskInfoMap.delete(taskId);

      // Continue processing next tasks
      this.processPendingTasks();
    }
  }

  /**
   * Emit completion event
   */
  private async emitCompletionEvent(
    entity: WorkflowExecutionEntity,
    executionTime: number,
  ): Promise<void> {
    try {
      const completedEvent = buildTriggeredSubgraphCompletedEvent({
        executionId: entity.id,
        workflowId: entity.getWorkflowId(),
        subgraphId: entity.getTriggeredSubworkflowId() || "",
        triggerId: "",
        output: entity.getOutput(),
        executionTime,
      });
      await emit(this.eventManager, completedEvent);
    } catch (error) {
      logger.warn("Failed to emit completion event", { error: (error as Error).message });
    }
  }

  /**
   * Emit failure event
   */
  private async emitFailureEvent(entity: WorkflowExecutionEntity, error: Error): Promise<void> {
    try {
      const failedEvent = buildTriggeredSubgraphFailedEvent({
        executionId: entity.id,
        workflowId: entity.getWorkflowId(),
        subgraphId: entity.getTriggeredSubworkflowId() || "",
        triggerId: "",
        error,
      });
      await emit(this.eventManager, failedEvent);
    } catch (emitError) {
      logger.warn("Failed to emit failure event", { error: (emitError as Error).message });
    }
  }

  /**
   * Cancel a task (implements TaskManager interface)
   * @param taskId Task ID
   * @returns Whether cancellation was successful
   */
  async cancelTask(taskId: string): Promise<boolean> {
    // Can only cancel pending tasks (not executing)
    const pendingIndex = this.pendingTaskIds.indexOf(taskId);
    if (pendingIndex > -1) {
      this.pendingTaskIds.splice(pendingIndex, 1);
      this.taskInfoMap.delete(taskId);
      this.taskRegistry.updateStatus(taskId, "CANCELLED");

      logger.debug("Pending task cancelled", { taskId });
      return true;
    }

    // Running tasks cannot be cancelled
    if (this.executingTaskIds.has(taskId)) {
      logger.warn("Cannot cancel executing task", { taskId });
      return false;
    }

    return false;
  }

  /**
   * Get task status (implements TaskManager interface)
   * @param taskId Task ID
   * @returns Task information or null
   */
  getTaskStatus(taskId: string) {
    return this.taskRegistry.get(taskId);
  }

  /**
   * Get queue statistics
   */
  getQueueStats() {
    const stats = this.taskRegistry.getStats();
    return {
      pendingCount: this.pendingTaskIds.length,
      runningCount: this.executingTaskIds.size,
      completedCount: stats.completed,
      failedCount: stats.failed,
      cancelledCount: stats.cancelled,
    };
  }

  /**
   * Get pool statistics
   */
  getPoolStats() {
    return this.workflowExecutionPool.getStats();
  }

  /**
   * Get task registry statistics
   */
  getTaskStats() {
    return this.taskRegistry.getStats();
  }
}
