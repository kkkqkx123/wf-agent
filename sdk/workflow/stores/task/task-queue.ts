/**
 * TaskQueue - Task Queue Manager
 *
 * Responsibilities:
 * - Manages the queue of ThreadContexts waiting to be executed
 * - Coordinates the assignment of tasks to the thread pool
 * - Supports both synchronous and asynchronous task submission
 * - Handles task completion and failure
 *
 * Design Principles:
 * - Stateful multi-instance architecture, held by TriggeredSubworkflowHandler
 * - Supports both synchronous and asynchronous execution modes
 * - Automatically handles coordination between the queue and the thread pool
 */

import { WorkflowExecutor } from "../../execution/executors/workflow-executor.js";
import { TaskRegistry } from "./task-registry.js";
import { WorkflowExecutionPool } from "../../execution/workflow-execution-pool.js";
import type { EventRegistry } from "../../../core/registry/event-registry.js";
import type { WorkflowExecutionEntity } from "../../entities/index.js";
import type { WorkflowExecutionResult } from "@wf-agent/types";
import {
  type QueueTask,
  type ExecutedSubgraphResult,
  type TaskSubmissionResult,
} from "../../execution/types/triggered-subworkflow.types.js";
import type { QueueStats } from "../../../core/types/index.js";
import { now, diffTimestamp, getErrorOrNew } from "@wf-agent/common-utils";
import { emit } from "../../../core/utils/event/event-emitter.js";
import {
  buildTriggeredSubgraphCompletedEvent,
  buildTriggeredSubgraphFailedEvent,
} from "../../execution/utils/event/index.js";
import { SDKError } from "@wf-agent/types";
import { logError, emitErrorEvent } from "../../../core/utils/error-utils.js";

/**
 * TaskQueue - Task Queue Manager
 */
export class TaskQueue {
  /**
   * Waiting Execution Queue
   */
  private pendingQueue: QueueTask[] = [];

  /**
   * Running task mapping
   */
  private runningTasks: Map<string, QueueTask> = new Map();

  /**
   * Task registration registry
   */
  private taskRegistry: TaskRegistry;

  /**
   * Workflow Execution Pool Service
   */
  private threadPoolService: WorkflowExecutionPool;

  /**
   * Event Manager
   */
  private eventManager: EventRegistry;

  /**
   * Is the queue being processed?
   */
  private isProcessing: boolean = false;

  /**
   * Constructor
   * @param taskRegistry Task Registry
   * @param threadPoolService Workflow Execution Pool Service
   * @param eventManager Event Manager
   */
  constructor(
    taskRegistry: TaskRegistry,
    threadPoolService: WorkflowExecutionPool,
    eventManager: EventRegistry,
  ) {
    this.taskRegistry = taskRegistry;
    this.threadPoolService = threadPoolService;
    this.eventManager = eventManager;
  }

  /**
   * Submit a synchronization task
   * @param taskId Task ID (registered by the manager)
   * @param threadEntity Thread entity
   * @param timeout Timeout period (in milliseconds)
   * @returns Execution result
   */
  async submitSync(
    taskId: string,
    threadEntity: WorkflowExecutionEntity,
    timeout?: number,
  ): Promise<ExecutedSubgraphResult> {
    return new Promise((resolve, reject) => {
      const queueTask: QueueTask = {
        taskId,
        threadEntity,
        workflowExecutionEntity: threadEntity,
        resolve: resolve as (
          value: ExecutedSubgraphResult | PromiseLike<ExecutedSubgraphResult>,
        ) => void,
        reject,
        submitTime: now(),
        timeout,
      };

      this.pendingQueue.push(queueTask);

      // Trigger queue processing
      this.processQueue();
    });
  }

  /**
   * Submit an asynchronous task
   * @param taskId Task ID (already registered by the manager)
   * @param threadEntity Thread entity
   * @param timeout Timeout period (in milliseconds)
   * @returns Task submission result
   */
  submitAsync(taskId: string, threadEntity: WorkflowExecutionEntity, timeout?: number): TaskSubmissionResult {
    const queueTask: QueueTask = {
      taskId,
      threadEntity,
      workflowExecutionEntity: threadEntity,
      resolve: () => {}, // Asynchronous tasks do not require resolve
      reject: () => {}, // Asynchronous tasks do not require reject
      submitTime: now(),
      timeout,
    };

    this.pendingQueue.push(queueTask);

    // Trigger queue processing
    this.processQueue();

    return {
      taskId,
      status: "QUEUED",
      message: "Task submitted successfully",
      submitTime: now(),
    };
  }

  /**
   * Processing queue
   */
  private async processQueue(): Promise<void> {
    if (this.isProcessing || this.pendingQueue.length === 0) {
      return;
    }

    this.isProcessing = true;

    try {
      while (this.pendingQueue.length > 0) {
        // Remove the first task from the queue.
        const queueTask = this.pendingQueue.shift()!;

        // Assign an executor
        const executor = await this.threadPoolService.allocateExecutor();

        // Update task status
        this.taskRegistry.updateStatusToRunning(queueTask.taskId);
        this.runningTasks.set(queueTask.taskId, queueTask);

        // Execute the task.
        this.executeTask(executor, queueTask);
      }
    } catch (error) {
      const errorObj = getErrorOrNew(error);
      const sdkError = new SDKError("Error processing queue", "error", {}, errorObj);
      logError(sdkError);
      emitErrorEvent(this.eventManager, {
        threadId: "",
        workflowId: "",
        error: sdkError,
      });
    } finally {
      this.isProcessing = false;
    }
  }

  /**
   * Execute the task.
   * @param executor: The executor
   * @param queueTask: The queue task
   */
  private async executeTask(executor: WorkflowExecutor, queueTask: QueueTask): Promise<void> {
    const startTime = now();

    try {
      // Execute Thread
      const threadResult = await executor.executeWorkflow(queueTask.threadEntity);

      const executionTime = diffTimestamp(startTime, now());

      // Task completion.
      await this.handleTaskCompleted(queueTask, threadResult, executionTime);
    } catch (error) {
      const executionTime = diffTimestamp(startTime, now());

      // Task processing failed.
      await this.handleTaskFailed(queueTask, error as Error, executionTime);
    } finally {
      // Release the executor
      await this.threadPoolService.releaseExecutor(executor);

      // Continue processing the queue.
      this.processQueue();
    }
  }

  /**
   * Task processing completed.
   * @param queueTask: Queue task
   * @param threadResult: Execution result
   * @param executionTime: Execution time
   */
  private async handleTaskCompleted(
    queueTask: QueueTask,
    threadResult: WorkflowExecutionResult,
    executionTime: number,
  ): Promise<void> {
    // Update task registry
    this.taskRegistry.updateStatusToCompleted(queueTask.taskId, threadResult);

    // Remove from the running tasks.
    this.runningTasks.delete(queueTask.taskId);

    // Trigger the completion event
    const completedEvent = buildTriggeredSubgraphCompletedEvent({
      threadId: queueTask.workflowExecutionEntity.id,
      workflowId: queueTask.threadEntity.getWorkflowId(),
      subgraphId: queueTask.threadEntity.getTriggeredSubworkflowId() || "",
      triggerId: "",
      output: queueTask.threadEntity.getOutput(),
      executionTime,
    });
    await emit(this.eventManager, completedEvent);

    // If it is a synchronous task, call resolve.
    if (queueTask.resolve) {
      const result: ExecutedSubgraphResult = {
        subgraphEntity: queueTask.threadEntity,
        threadResult,
        executionTime,
      };
      queueTask.resolve(result);
    }
  }

  /**
   * Task processing failed.
   * @param queueTask: Queue task
   * @param error: Error message
   * @param executionTime: Execution time
   */
  private async handleTaskFailed(
    queueTask: QueueTask,
    error: Error,
    _executionTime: number,
  ): Promise<void> {
    // Update task registry
    this.taskRegistry.updateStatusToFailed(queueTask.taskId, error);

    // Remove from the running tasks.
    this.runningTasks.delete(queueTask.taskId);

    // Trigger a failure event.
    const failedEvent = buildTriggeredSubgraphFailedEvent({
      threadId: queueTask.workflowExecutionEntity.id,
      workflowId: queueTask.threadEntity.getWorkflowId(),
      subgraphId: queueTask.threadEntity.getTriggeredSubworkflowId() || "",
      triggerId: "",
      error: getErrorOrNew(error),
    });
    await emit(this.eventManager, failedEvent);

    // If it's a synchronous task, call reject.
    if (queueTask.reject) {
      queueTask.reject(error);
    }
  }

  /**
   * Cancel Task
   * @param taskId Task ID
   * @returns Whether the cancellation was successful
   */
  cancelTask(taskId: string): boolean {
    // Check if the task is in the queue of pending execution.
    const pendingIndex = this.pendingQueue.findIndex(task => task.taskId === taskId);
    if (pendingIndex > -1) {
      const queueTask = this.pendingQueue.splice(pendingIndex, 1)[0];
      if (!queueTask) {
        return false;
      }

      this.taskRegistry.updateStatusToCancelled(taskId);

      // Trigger the cancellation event (using the FAILED event type, as the CANCELLED event type does not exist)
      const cancelledEvent = buildTriggeredSubgraphFailedEvent({
        threadId: queueTask.workflowExecutionEntity.id,
        workflowId: queueTask.threadEntity.getWorkflowId(),
        subgraphId: queueTask.threadEntity.getTriggeredSubworkflowId() || "",
        triggerId: "",
        error: new Error("Task cancelled"),
      });
      emit(this.eventManager, cancelledEvent);

      return true;
    }

    // Check if the task is running.
    if (this.runningTasks.has(taskId)) {
      // The running task cannot be canceled.
      return false;
    }

    return false;
  }

  /**
   * Get queue statistics information
   * @returns Statistical information
   */
  getQueueStats(): QueueStats {
    return {
      pendingCount: this.pendingQueue.length,
      runningCount: this.runningTasks.size,
      completedCount: this.taskRegistry.getStats().completed,
      failedCount: this.taskRegistry.getStats().failed,
      cancelledCount: this.taskRegistry.getStats().cancelled,
    };
  }

  /**
   * Wait for all tasks to complete.
   */
  async drain(): Promise<void> {
    while (this.pendingQueue.length > 0 || this.runningTasks.size > 0) {
      await new Promise(resolve => setTimeout(resolve, 100));
    }
  }

  /**
   * Clear the queue
   */
  clear(): void {
    // Cancel all pending tasks.
    for (const queueTask of this.pendingQueue) {
      this.taskRegistry.updateStatusToCancelled(queueTask.taskId);
    }

    this.pendingQueue = [];
  }
}
