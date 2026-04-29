/**
 * ExecutionQueue - Generic Execution Queue
 *
 * Responsibilities:
 * - Manages the queue of execution instances waiting to be executed
 * - Coordinates the assignment of tasks to the execution pool
 * - Supports both synchronous and asynchronous task submission
 * - Handles task completion and failure
 *
 * Design Principles:
 * - Generic design supporting any execution instance type
 * - Supports both synchronous and asynchronous execution modes
 * - Automatically handles coordination between the queue and the execution pool
 */

import { TaskRegistry } from "../../workflow/stores/task/task-registry.js";
import { ExecutionPool, type Executor } from "./execution-pool.js";
import type { EventRegistry } from "../registry/event-registry.js";
import {
  type ExecutionInstance,
  type ExecutionInstanceType,
  isAgentInstance,
  isThreadInstance,
  type QueueStats,
} from "../types/index.js";
import type { WorkflowExecutionResult } from "@wf-agent/types";
import { now, diffTimestamp, getErrorOrNew } from "@wf-agent/common-utils";
import { SDKError } from "@wf-agent/types";
import { logError, emitErrorEvent } from "../utils/error-utils.js";
import { emit } from "../../workflow/execution/utils/index.js";
import {
  buildTriggeredSubgraphCompletedEvent,
  buildTriggeredSubgraphFailedEvent,
} from "../../workflow/execution/utils/event/index.js";

/**
 * Queue task interface
 */
export interface QueueTask {
  /** Task ID */
  taskId: string;
  /** Execution instance */
  instance: ExecutionInstance;
  /** Instance type */
  instanceType: ExecutionInstanceType;
  /** Resolve callback for sync execution */
  resolve: (result: ExecutionResult) => void;
  /** Reject callback for sync execution */
  reject: (error: Error) => void;
  /** Submit time */
  submitTime: number;
  /** Timeout (optional) */
  timeout?: number;
}

/**
 * Execution result interface
 */
export interface ExecutionResult {
  /** Execution instance */
  instance: ExecutionInstance;
  /** Thread result (for thread execution) */
  threadResult?: WorkflowExecutionResult;
  /** Agent result (for agent execution) */
  agentResult?: unknown;
  /** Execution time in ms */
  executionTime: number;
}

/**
 * Task submission result
 */
export interface TaskSubmissionResult {
  /** Task ID */
  taskId: string;
  /** Status */
  status: "QUEUED" | "REJECTED";
  /** Message */
  message: string;
  /** Submit time */
  submitTime: number;
}

/**
 * ExecutionQueue - Generic Execution Queue
 *
 * @template T - The execution instance type
 */
export class ExecutionQueue<T extends ExecutionInstance> {
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
   * Execution Pool Service
   */
  private poolService: ExecutionPool<T>;

  /**
   * Event Manager
   */
  private eventManager: EventRegistry;

  /**
   * Execute function - how to execute the instance
   */
  private executeFn: (executor: Executor<T>, instance: T) => Promise<unknown>;

  /**
   * Is the queue being processed?
   */
  private isProcessing: boolean = false;

  /**
   * Constructor
   * @param taskRegistry Task Registry
   * @param poolService Execution Pool Service
   * @param eventManager Event Manager
   * @param executeFn Function to execute the instance
   */
  constructor(
    taskRegistry: TaskRegistry,
    poolService: ExecutionPool<T>,
    eventManager: EventRegistry,
    executeFn: (executor: Executor<T>, instance: T) => Promise<unknown>,
  ) {
    this.taskRegistry = taskRegistry;
    this.poolService = poolService;
    this.eventManager = eventManager;
    this.executeFn = executeFn;
  }

  /**
   * Submit a synchronization task
   * @param taskId Task ID (registered by the manager)
   * @param instance Execution instance
   * @param instanceType Instance type
   * @param timeout Timeout period (in milliseconds)
   * @returns Execution result
   */
  async submitSync(
    taskId: string,
    instance: T,
    instanceType: ExecutionInstanceType,
    timeout?: number,
  ): Promise<ExecutionResult> {
    return new Promise((resolve, reject) => {
      const queueTask: QueueTask = {
        taskId,
        instance,
        instanceType,
        resolve,
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
   * @param instance Execution instance
   * @param instanceType Instance type
   * @param timeout Timeout period (in milliseconds)
   * @returns Task submission result
   */
  submitAsync(
    taskId: string,
    instance: T,
    instanceType: ExecutionInstanceType,
    timeout?: number,
  ): TaskSubmissionResult {
    const queueTask: QueueTask = {
      taskId,
      instance,
      instanceType,
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
        const executor = await this.poolService.allocateExecutor();

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
        executionId: "",
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
  private async executeTask(executor: Executor<T>, queueTask: QueueTask): Promise<void> {
    const startTime = now();

    try {
      // Execute the instance
      const result = await this.executeFn(executor, queueTask.instance as T);

      const executionTime = diffTimestamp(startTime, now());

      // Task completion.
      await this.handleTaskCompleted(queueTask, result, executionTime);
    } catch (error) {
      const executionTime = diffTimestamp(startTime, now());

      // Task processing failed.
      await this.handleTaskFailed(queueTask, error as Error, executionTime);
    } finally {
      // Release the executor
      await this.poolService.releaseExecutor(executor);

      // Continue processing the queue.
      this.processQueue();
    }
  }

  /**
   * Task processing completed.
   * @param queueTask: Queue task
   * @param result: Execution result
   * @param executionTime: Execution time
   */
  private async handleTaskCompleted(
    queueTask: QueueTask,
    result: unknown,
    executionTime: number,
  ): Promise<void> {
    // Update task registry
    this.taskRegistry.updateStatusToCompleted(queueTask.taskId, result as WorkflowExecutionResult);

    // Remove from the running tasks.
    this.runningTasks.delete(queueTask.taskId);

    // Trigger the completion event (for thread instances)
    if (isThreadInstance(queueTask.instance)) {
      const completedEvent = buildTriggeredSubgraphCompletedEvent({
        executionId: queueTask.instance.id,
        workflowId: queueTask.instance.getWorkflowId(),
        subgraphId: queueTask.instance.getTriggeredSubworkflowId() || "",
        triggerId: "",
        output: queueTask.instance.getOutput(),
        executionTime,
      });
      await emit(this.eventManager, completedEvent);
    }

    // If it is a synchronous task, call resolve.
    if (queueTask.resolve) {
      const execResult: ExecutionResult = {
        instance: queueTask.instance,
        threadResult: isThreadInstance(queueTask.instance) ? (result as WorkflowExecutionResult) : undefined,
        agentResult: isAgentInstance(queueTask.instance) ? result : undefined,
        executionTime,
      };
      queueTask.resolve(execResult);
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

    // Trigger a failure event (for thread instances)
    if (isThreadInstance(queueTask.instance)) {
      const failedEvent = buildTriggeredSubgraphFailedEvent({
        executionId: queueTask.instance.id,
        workflowId: queueTask.instance.getWorkflowId(),
        subgraphId: queueTask.instance.getTriggeredSubworkflowId() || "",
        triggerId: "",
        error: getErrorOrNew(error),
      });
      await emit(this.eventManager, failedEvent);
    }

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

      // Trigger the cancellation event (for thread instances)
      if (isThreadInstance(queueTask.instance)) {
        const cancelledEvent = buildTriggeredSubgraphFailedEvent({
          executionId: queueTask.instance.id,
          workflowId: queueTask.instance.getWorkflowId(),
          subgraphId: queueTask.instance.getTriggeredSubworkflowId() || "",
          triggerId: "",
          error: new Error("Task cancelled"),
        });
        emit(this.eventManager, cancelledEvent);
      }

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
