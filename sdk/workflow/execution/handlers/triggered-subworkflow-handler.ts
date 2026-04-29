/**
 * TriggeredSubworkflowHandler - Triggered Sub-workflow Manager (Global Singleton Service)
 *
 * Responsibilities:
 * - Manages the entire lifecycle of triggered sub-workflows
 * - Coordinates the creation and execution of sub-workflows
 * - Uses a task queue and thread pool for execution management
 * - Supports both synchronous and asynchronous execution modes
 * - Provides functionality for querying task status and canceling tasks
 *
 * Design Principles:
 * - Acts as a global singleton service, managed by a Dependency Injection (DI) container
 * - Handles resources shared across threads (thread pool, task queue)
 * - No thread isolation required; all triggered sub-workflows share the same instance
 * - Implements the TaskManager interface for use in conjunction with TaskRegistry
 */

import type { WorkflowExecutionEntity } from "../../entities/index.js";
import type { WorkflowStateCoordinator } from "../../state-managers/workflow-state-coordinator.js";
import type { ConversationSession } from "../../../core/messaging/conversation-session.js";
import { getErrorOrNew, now } from "@wf-agent/common-utils";
import { TaskRegistry, type TaskManager } from "../../stores/task/task-registry.js";
import type { WorkflowExecutionPool } from "../workflow-execution-pool.js";
import { TaskQueue } from "../../stores/task/task-queue.js";
import { CallbackState } from "../../state-managers/callback-state.js";
import type { EventRegistry } from "../../../core/registry/event-registry.js";
import {
  type TriggeredSubgraphTask,
  type ExecutedSubgraphResult,
  type TaskSubmissionResult,
} from "../types/triggered-subworkflow.types.js";
import { emit } from "../../../core/utils/event/event-emitter.js";
import {
  buildTriggeredSubgraphStartedEvent,
  buildTriggeredSubgraphCompletedEvent,
  buildTriggeredSubgraphFailedEvent,
} from "../utils/event/index.js";
import { RuntimeValidationError, SDKError } from "@wf-agent/types";
import { logError, emitErrorEvent } from "../../../core/utils/error-utils.js";

/**
 * Workflow Execution Build Result (simplified interface for TriggeredSubworkflowHandler)
 */
interface ThreadBuildResultSimple {
  executionEntity: WorkflowExecutionEntity;
  stateCoordinator: WorkflowStateCoordinator;
  conversationManager: ConversationSession;
}

/**
 * TriggeredSubworkflowHandler - Triggered Sub-workflow Manager (a global singleton service)
 */
export class TriggeredSubworkflowHandler implements TaskManager {
  /**
   * Global Task Registry
   */
  private taskRegistry: TaskRegistry;

  /**
   * Thread Pool Service
   */
  private threadPoolService: WorkflowExecutionPool;

  /**
   * Task Queue Manager
   */
  private taskQueueManager: TaskQueue;

  /**
   * Event Manager
   */
  private eventManager: EventRegistry;

  /**
   * Thread Registry
   */
  private workflowExecutionRegistry: {
    register: (entity: WorkflowExecutionEntity) => void;
    get: (id: string) => WorkflowExecutionEntity | undefined;
  };

  /**
   * Thread Builder
   */
  private executionBuilder: {
    build: (
      subgraphId: string,
      options: { input: Record<string, unknown> },
    ) => Promise<ThreadBuildResultSimple>;
  };

  /**
   * Callback State
   */
  private callbackState: CallbackState<ExecutedSubgraphResult>;

  /**
   * Active Task Mapping
   * Used to track and manage tasks that are executed asynchronously, to prevent memory leaks.
   */
  private activeTasks: Map<
    string,
    { taskId: string; executionId: string; submitTime: number; timeout: number }
  > = new Map();

  /**
   * Constructor
   * @param workflowExecutionRegistry: Thread registry
   * @param executionBuilder: Thread builder
   * @param taskQueueManager: Task queue manager
   * @param eventManager: Event manager
   * @param threadPoolService: Thread pool service
   */
  constructor(
    workflowExecutionRegistry: {
      register: (entity: WorkflowExecutionEntity) => void;
      get: (id: string) => WorkflowExecutionEntity | undefined;
    },
    executionBuilder: {
      build: (
        subgraphId: string,
        options: { input: Record<string, unknown> },
      ) => Promise<ThreadBuildResultSimple>;
    },
    taskQueueManager: TaskQueue,
    eventManager: EventRegistry,
    threadPoolService: WorkflowExecutionPool,
  ) {
    this.workflowExecutionRegistry = workflowExecutionRegistry;
    this.executionBuilder = executionBuilder;
    this.taskQueueManager = taskQueueManager;
    this.eventManager = eventManager;
    this.threadPoolService = threadPoolService;

    // Get the global task registry
    this.taskRegistry = TaskRegistry.getInstance();

    // Create a callback state
    this.callbackState = new CallbackState<ExecutedSubgraphResult>();
  }

  /**
   * Execute the trigger workflow
   * @param task The trigger workflow task
   * @returns Execution result (synchronous) or task submission result (asynchronous)
   */
  async executeTriggeredSubgraph(
    task: TriggeredSubgraphTask,
  ): Promise<ExecutedSubgraphResult | TaskSubmissionResult> {
    // Verify parameters
    if (!task.subgraphId) {
      throw new RuntimeValidationError("subgraphId is required", {
        operation: "executeTriggeredSubgraph",
        field: "subgraphId",
      });
    }

    if (!task.mainThreadEntity) {
      throw new RuntimeValidationError("mainThreadEntity is required", {
        operation: "executeTriggeredSubgraph",
        field: "mainThreadEntity",
      });
    }

    // Prepare the input data
    const input = this.prepareInputData(task);

    // Create a sub-workflow WorkflowExecutionEntity
    const subgraphEntity = await this.createSubgraphContext(task, input);

    // Register with WorkflowExecutionRegistry
    this.workflowExecutionRegistry.register(subgraphEntity);

    // Establish a parent-child thread relationship
    const parentThreadId = task.mainThreadEntity.id;
    const childThreadId = subgraphEntity.id;
    task.mainThreadEntity.registerChildThread(childThreadId);
    subgraphEntity.setParentThreadId(parentThreadId);
    subgraphEntity.setTriggeredSubworkflowId(task.subgraphId);

    // Trigger the start event
    await this.emitStartedEvent(task, subgraphEntity);

    // Select the execution method based on the configuration.
    const waitForCompletion = task.config?.waitForCompletion !== false; // The default value is `true`.
    const timeout = task.config?.timeout || this.threadPoolService.getConfig().defaultTimeout;

    if (waitForCompletion) {
      // Synchronize execution
      return await this.executeSync(subgraphEntity, timeout);
    } else {
      // Asynchronous execution
      return this.executeAsync(subgraphEntity, timeout);
    }
  }

  /**
   * Prepare input data
   * @param task: Trigger the sub-workflow task
   * @returns: Input data
   */
  private prepareInputData(task: TriggeredSubgraphTask): Record<string, unknown> {
    return {
      triggerId: task.triggerId,
      output: task.mainThreadEntity.getOutput(),
      input: task.mainThreadEntity.getInput(),
      ...task.input,
    };
  }

  /**
   * Create a sub-workflow context
   * @param task: The task that triggers the sub-workflow
   * @param input: The input data
   * @returns: The sub-workflow entity
   */
  private async createSubgraphContext(
    task: TriggeredSubgraphTask,
    input: Record<string, unknown>,
  ): Promise<WorkflowExecutionEntity> {
    const { executionEntity: subgraphEntity } = await this.executionBuilder.build(task.subgraphId, {
      input,
    });

    // Set the thread type to TRIGGERED_SUBWORKFLOW
    subgraphEntity.setThreadType("TRIGGERED_SUBWORKFLOW");

    return subgraphEntity;
  }

  /**
   * Synchronize execution
   * @param subgraphEntity Sub-workflow entity
   * @param timeout Timeout period
   * @returns Execution result
   */
  private async executeSync(
    subgraphEntity: WorkflowExecutionEntity,
    timeout: number,
  ): Promise<ExecutedSubgraphResult> {
    // First, register the task with the global TaskRegistry.
    const taskId = this.taskRegistry.register(subgraphEntity, "thread", this, timeout);

    try {
      const result = await this.taskQueueManager.submitSync(taskId, subgraphEntity, timeout);

      // Cancel the parent-child relationship
      this.unregisterParentChildRelationship(subgraphEntity);

      return result;
    } catch (error) {
      // Cancel the parent-child relationship
      this.unregisterParentChildRelationship(subgraphEntity);
      throw error;
    }
  }

  /**
   * Asynchronous execution
   * @param subgraphEntity: Sub-workflow entity
   * @param timeout: Timeout period
   * @returns: Task submission result
   */
  private executeAsync(subgraphEntity: WorkflowExecutionEntity, timeout: number): TaskSubmissionResult {
    const executionId = subgraphEntity.id;

    // First, register the task with the global TaskRegistry.
    const taskId = this.taskRegistry.register(subgraphEntity, "thread", this, timeout);

    // Submit to the task queue
    const submissionResult = this.taskQueueManager.submitAsync(taskId, subgraphEntity, timeout);

    // Register the callback directly without creating a Promise.
    // This can prevent memory leaks caused by uncleaned Promise references.
    this.callbackState.registerCallback(
      executionId,
      (result: ExecutedSubgraphResult) => this.handleSubgraphCompleted(executionId, result),
      (error: Error) => this.handleSubgraphFailed(executionId, error),
    );

    // Store task information for subsequent cleanup.
    this.activeTasks.set(executionId, {
      taskId,
      executionId,
      submitTime: now(),
      timeout,
    });

    return {
      taskId: submissionResult.taskId,
      status: submissionResult.status,
      message: "Triggered subgraph submitted",
      submitTime: submissionResult.submitTime,
    };
  }

  /**
   * Sub-workflow processing completed.
   * @param executionId: Execution ID
   * @param result: Execution result
   */
  private handleSubgraphCompleted(executionId: string, result: ExecutedSubgraphResult): void {
    // Trigger the completion event
    this.emitCompletedEvent(executionId, result);

    // Cancel the parent-child relationship
    const subgraphEntity = this.workflowExecutionRegistry.get(executionId);
    if (subgraphEntity) {
      this.unregisterParentChildRelationship(subgraphEntity);
    }

    // Clean up the task records in TaskRegistry.
    const taskInfo = this.taskRegistry
      .getAll()
      .find(t => t.instanceType === "thread" && t.instance.id === executionId);
    if (taskInfo) {
      this.taskRegistry.delete(taskInfo.id);
    }

    // Clean up active task information
    this.activeTasks.delete(executionId);

    // Finally trigger the callback (the callback will be cleaned up internally).
    // The `triggerCallback` function already cleans up the callbacks internally, so there is no need to call the `cleanupCallback` function as well.
    this.callbackState.triggerCallback(executionId, result);
  }

  /**
   * Sub-workflow processing failed.
   * @param executionId: Execution ID
   * @param error: Error message
   */
  private handleSubgraphFailed(executionId: string, error: Error): void {
    // Trigger a failure event.
    this.emitFailedEvent(executionId, error);

    // Cancel the parent-child relationship
    const subgraphEntity = this.workflowExecutionRegistry.get(executionId);
    if (subgraphEntity) {
      this.unregisterParentChildRelationship(subgraphEntity);
    }

    // Clean up task records in the TaskRegistry.
    const taskInfo = this.taskRegistry
      .getAll()
      .find(t => t.instanceType === "thread" && t.instance.id === executionId);
    if (taskInfo) {
      this.taskRegistry.delete(taskInfo.id);
    }

    // Clean up active task information
    this.activeTasks.delete(executionId);

    // The final error callback is triggered (the callback will be cleaned up internally).
    // The `triggerErrorCallback` function already cleans up the callback internally, so there is no need to call the `cleanupCallback` function again.
    this.callbackState.triggerErrorCallback(executionId, error);
  }

  /**
   * Cancel the parent-child relationship
   * @param subgraphEntity Sub-workflow entity
   */
  private unregisterParentChildRelationship(subgraphEntity: WorkflowExecutionEntity): void {
    const parentThreadId = subgraphEntity.getParentThreadId();
    const childThreadId = subgraphEntity.id;

    if (parentThreadId) {
      const parentEntity = this.workflowExecutionRegistry.get(parentThreadId);
      if (parentEntity) {
        parentEntity.unregisterChildThread(childThreadId);
      }
    }
  }

  /**
   * Trigger the start event
   * @param task: Task that triggers the sub-workflow
   * @param subgraphEntity: Sub-workflow entity
   */
  private async emitStartedEvent(
    task: TriggeredSubgraphTask,
    _subgraphEntity: WorkflowExecutionEntity,
  ): Promise<void> {
    const startedEvent = buildTriggeredSubgraphStartedEvent({
      executionId: task.mainThreadEntity.id,
      workflowId: task.mainThreadEntity.getWorkflowId(),
      subgraphId: task.subgraphId,
      triggerId: task.triggerId,
      input: task.input,
    });
    await emit(this.eventManager, startedEvent);
  }

  /**
   * Trigger the completion event
   * @param executionId: Execution ID
   * @param result: Execution result
   */
  private async emitCompletedEvent(
    executionId: string,
    result: ExecutedSubgraphResult,
  ): Promise<void> {
    const subgraphEntity = this.workflowExecutionRegistry.get(executionId);
    if (!subgraphEntity) {
      return;
    }

    const completedEvent = buildTriggeredSubgraphCompletedEvent({
      executionId: subgraphEntity.id,
      workflowId: subgraphEntity.getWorkflowId(),
      subgraphId: subgraphEntity.getTriggeredSubworkflowId() || "",
      triggerId: "",
      output: subgraphEntity.getOutput(),
      executionTime: result.executionTime,
    });
    await emit(this.eventManager, completedEvent);
  }

  /**
   * Trigger a failure event
   * @param executionId: Execution ID
   * @param error: Error message
   */
  private async emitFailedEvent(executionId: string, error: Error): Promise<void> {
    const subgraphEntity = this.workflowExecutionRegistry.get(executionId);
    if (!subgraphEntity) {
      return;
    }

    const failedEvent = buildTriggeredSubgraphFailedEvent({
      executionId: subgraphEntity.id,
      workflowId: subgraphEntity.getWorkflowId(),
      subgraphId: subgraphEntity.getTriggeredSubworkflowId() || "",
      triggerId: "",
      error: getErrorOrNew(error),
    });
    await emit(this.eventManager, failedEvent);
  }

  /**
   * Query task status (implementing the TaskManager interface)
   * @param taskId Task ID
   * @returns Task information
   */
  getTaskStatus(taskId: string) {
    return this.taskRegistry.get(taskId);
  }

  /**
   * Cancel a task (implementing the TaskManager interface)
   * @param taskId Task ID
   * @returns Whether the cancellation was successful
   */
  async cancelTask(taskId: string): Promise<boolean> {
    const success = this.taskQueueManager.cancelTask(taskId);

    if (success) {
      const taskInfo = this.taskRegistry.get(taskId);
      if (taskInfo && taskInfo.instanceType === "thread") {
        // Cancel the parent-child relationship.
        this.unregisterParentChildRelationship(taskInfo.instance as WorkflowExecutionEntity);
      }
    }

    return success;
  }

  /**
   * Get queue statistics information
   * @returns Queue statistics information
   */
  getQueueStats() {
    return this.taskQueueManager.getQueueStats();
  }

  /**
   * Get thread pool statistics
   * @returns Thread pool statistics
   */
  getPoolStats() {
    return this.threadPoolService.getStats();
  }

  /**
   * Retrieve task registry statistics
   * @returns Task registry statistics
   */
  getTaskStats() {
    return this.taskRegistry.getStats();
  }

  /**
   * Close the manager.
   */
  async shutdown(): Promise<void> {
    // Cancel all active tasks.
    for (const [executionId, task] of this.activeTasks.entries()) {
      try {
        await this.cancelTask(task.taskId);
      } catch (error) {
        const errorObj = getErrorOrNew(error);
        const sdkError = new SDKError(
          `Failed to cancel task ${task.taskId}`,
          "warning",
          { executionId, taskId: task.taskId },
          errorObj,
        );
        logError(sdkError, { executionId, taskId: task.taskId });
        emitErrorEvent(this.eventManager, {
          executionId,
          workflowId: "",
          error: sdkError,
        });
      }
    }

    // Clean up active task information
    this.activeTasks.clear();

    // Clean up the callback state
    this.callbackState.cleanup();

    // Wait for all tasks to complete.
    await this.taskQueueManager.drain();

    // Close the thread pool.
    await this.threadPoolService.shutdown();

    // Clean up task registry entries.
    this.taskRegistry.cleanup();
  }

  /**
   * Clean up expired tasks
   * @param retentionTime Retention time in milliseconds
   * @returns Number of tasks that were cleaned up
   */
  async cleanupExpiredTasks(retentionTime?: number): Promise<number> {
    return this.taskRegistry.cleanup(retentionTime);
  }
}
