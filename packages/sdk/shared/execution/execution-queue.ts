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

import { TaskRegistry } from "../registry/task-registry.js";
import { ExecutionPool, type Executor } from "./execution-pool.js";
import type { EventRegistry } from "../registry/event-registry.js";
import {
  type ExecutionInstance,
  type ExecutionInstanceType,
  isAgentInstance,
  isWorkflowExecutionInstance,
  type QueueStats,
} from "../types/index.js";
import type { WorkflowExecutionResult } from "@wf-agent/types";
import { now, diffTimestamp, getErrorOrNew } from "@wf-agent/common-utils";
import { SDKError } from "@wf-agent/types";
import { logError, emitErrorEvent } from "../utils/error-utils.js";
import { emit } from "../events/emit-event.js";
import {
  buildTriggeredSubgraphCompletedEvent,
  buildTriggeredSubgraphFailedEvent,
} from "../events/builders/index.js";

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
  /** AbortSignal for cancellation */
  abortSignal: AbortSignal;
}

/**
 * Execution result interface
 */
export interface ExecutionResult {
  /** Execution instance */
  instance: ExecutionInstance;
  /** Workflow execution result (for workflow execution) */
  executionResult?: WorkflowExecutionResult;
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
   * AbortController mapping for active tasks - supports cancellation
   */
  private abortControllers: Map<string, AbortController> = new Map();

  /**
   * Timeout guard mapping - for timeout detection
   */
  private timeoutGuards: Map<string, NodeJS.Timeout> = new Map();

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
  private executeFn: (executor: Executor<T>, instance: T, signal?: AbortSignal) => Promise<unknown>;

  /**
   * Is the queue being processed?
   */
  private isProcessing: boolean = false;

  /**
   * Constructor
   * @param taskRegistry Task Registry
   * @param poolService Execution Pool Service
   * @param eventManager Event Manager
   * @param executeFn Function to execute the instance (receives AbortSignal for cancellation)
   */
  constructor(
    taskRegistry: TaskRegistry,
    poolService: ExecutionPool<T>,
    eventManager: EventRegistry,
    executeFn: (executor: Executor<T>, instance: T, signal?: AbortSignal) => Promise<unknown>,
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
      const abortController = new AbortController();
      const submitTime = now();
      const deadlineTime = timeout ? submitTime + timeout : undefined;

      const queueTask: QueueTask = {
        taskId,
        instance,
        instanceType,
        resolve,
        reject,
        submitTime,
        timeout,
        abortSignal: abortController.signal,
      };

      // Store deadline for timeout recovery
      (queueTask as any).deadlineTime = deadlineTime;

      this.pendingQueue.push(queueTask);
      this.abortControllers.set(taskId, abortController);

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
    const abortController = new AbortController();
    const submitTime = now();
    const deadlineTime = timeout ? submitTime + timeout : undefined;

    const queueTask: QueueTask = {
      taskId,
      instance,
      instanceType,
      resolve: () => {}, // Asynchronous tasks do not require resolve
      reject: () => {}, // Asynchronous tasks do not require reject
      submitTime,
      timeout,
      abortSignal: abortController.signal,
    };

    // Store deadline for timeout recovery
    (queueTask as any).deadlineTime = deadlineTime;

    this.pendingQueue.push(queueTask);
    this.abortControllers.set(taskId, abortController);

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
        this.taskRegistry.updateStatus(queueTask.taskId, "RUNNING");
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
   * Execute the task with timeout guard and cancellation support.
   * @param executor: The executor
   * @param queueTask: The queue task
   */
  private async executeTask(executor: Executor<T>, queueTask: QueueTask): Promise<void> {
    const startTime = now();
    let timeoutGuard: NodeJS.Timeout | undefined;

    try {
      // Set up timeout guard if timeout is specified
      if (queueTask.timeout && queueTask.timeout > 0) {
        timeoutGuard = setTimeout(() => {
          this.taskRegistry.updateStatus(queueTask.taskId, "TIMEOUT");
          const abortCtrl = this.abortControllers.get(queueTask.taskId);
          if (abortCtrl) {
            abortCtrl.abort();
          }
        }, queueTask.timeout);
        this.timeoutGuards.set(queueTask.taskId, timeoutGuard);
      }

      // Execute the instance with AbortSignal for cancellation
      const result = await this.executeFn(executor, queueTask.instance as T, queueTask.abortSignal);

      const executionTime = diffTimestamp(startTime, now());

      // Task completion.
      await this.handleTaskCompleted(queueTask, result, executionTime);
    } catch (error) {
      const executionTime = diffTimestamp(startTime, now());

      // Handle abort (cancellation) explicitly
      if (error instanceof Error && error.name === "AbortError") {
        // Task was cancelled
        this.taskRegistry.updateStatus(queueTask.taskId, "CANCELLED");
        this.runningTasks.delete(queueTask.taskId);
      } else {
        // Task processing failed.
        await this.handleTaskFailed(queueTask, error as Error, executionTime);
      }
    } finally {
      // Clean up timeout guard
      if (timeoutGuard) {
        clearTimeout(timeoutGuard);
        this.timeoutGuards.delete(queueTask.taskId);
      }

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
    this.taskRegistry.updateStatus(queueTask.taskId, "COMPLETED", { result: result as WorkflowExecutionResult });

    // Remove from the running tasks.
    this.runningTasks.delete(queueTask.taskId);

    // Trigger the completion event (for workflow execution instances)
    if (isWorkflowExecutionInstance(queueTask.instance)) {
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
        executionResult: isWorkflowExecutionInstance(queueTask.instance)
          ? (result as WorkflowExecutionResult)
          : undefined,
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
    this.taskRegistry.updateStatus(queueTask.taskId, "FAILED", { error });

    // Remove from the running tasks.
    this.runningTasks.delete(queueTask.taskId);

    // Trigger a failure event (for workflow execution instances)
    if (isWorkflowExecutionInstance(queueTask.instance)) {
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
   * Cancel Task - supports both queued and running tasks
   * For running tasks, sends an abort signal to allow graceful cancellation
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

      this.taskRegistry.updateStatus(taskId, "CANCELLED");
      this.abortControllers.delete(taskId);

      // Trigger the cancellation event (for workflow execution instances)
      if (isWorkflowExecutionInstance(queueTask.instance)) {
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

    // Check if the task is running - abort it
    if (this.runningTasks.has(taskId)) {
      const abortCtrl = this.abortControllers.get(taskId);
      if (abortCtrl) {
        abortCtrl.abort();  // Send abort signal
        return true;
      }
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
      this.taskRegistry.updateStatus(queueTask.taskId, "CANCELLED");
      this.abortControllers.delete(queueTask.taskId);
    }

    // Clean up timeout guards
    for (const guard of this.timeoutGuards.values()) {
      clearTimeout(guard);
    }
    this.timeoutGuards.clear();

    this.pendingQueue = [];
  }
}
