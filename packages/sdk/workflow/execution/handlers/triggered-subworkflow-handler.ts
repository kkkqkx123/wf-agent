/**
 * TriggeredSubworkflowHandler - Triggered Sub-workflow Manager (Global Singleton Service)
 *
 * Responsibilities:
 * - Manages the entire lifecycle of triggered sub-workflows
 * - Coordinates the creation and execution of sub-workflows
 * - Uses a task queue and workflow execution pool for execution management
 * - Supports both synchronous and asynchronous execution modes
 * - Provides functionality for querying task status and canceling tasks
 *
 * Design Principles:
 * - Acts as a global singleton service, managed by a Dependency Injection (DI) container
 * - Handles resources shared across executions (execution pool, task queue)
 * - No workflow-execution isolation required; all triggered sub-workflows share the same instance
 * - Implements the TaskManager interface for use in conjunction with TaskRegistry
 */

import type { WorkflowExecutionEntity } from "../../entities/index.js";
import type { WorkflowStateCoordinator } from "../../state-managers/workflow-state-coordinator.js";
import type { ConversationSession } from "../../../shared/messaging/conversation-session.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "TriggeredSubworkflowHandler" });
import type { ExecuteTriggeredSubworkflowActionConfig } from "@wf-agent/types";
import type {
  ChildExecutionType,
  ChildExecutionConfig,
} from "../factories/workflow-execution-builder.js";
import { getErrorOrNew, now } from "@wf-agent/common-utils";
import { TaskRegistry, type TaskManager } from "../../../shared/registry/task-registry.js";
import type { WorkflowExecutionPool } from "../workflow-execution-pool.js";
import { AsyncCompletionManager } from "../utils/async-completion-manager.js";
import type { EventRegistry } from "../../../shared/registry/event-registry.js";
import type { IAgentExecutionRegistry } from "../../../agent/registry/agent-execution-registry.js";
import {
  type TriggeredSubworkflowTask,
  type ExecutedSubworkflowResult,
  type TaskSubmissionResult,
  type ResolvedDataSource,
} from "../types/triggered-subworkflow.types.js";
import { emit } from "../../../shared/events/emit-event.js";
import {
  buildTriggeredSubgraphStartedEvent,
  buildTriggeredSubgraphCompletedEvent,
  buildTriggeredSubgraphFailedEvent,
} from "../../../shared/events/builders/index.js";
import { RuntimeValidationError, SDKError } from "@wf-agent/types";
import { logError, emitErrorEvent } from "../../../shared/utils/error-utils.js";
import { cleanupChildExecution } from "../utils/child-execution-cleanup.js";
import type { MessageContextRegistry, WorkflowExecution } from "@wf-agent/types";

/**
 * Workflow Execution Build Result (simplified interface for TriggeredSubworkflowHandler)
 */
interface WorkflowExecutionBuildResultSimple {
  workflowExecutionEntity: WorkflowExecutionEntity;
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
   * Workflow Execution Pool Service
   */
  private workflowExecutionPool: WorkflowExecutionPool;

  /**
   * Task Queue Manager (deprecated, kept for backward compatibility)
   */
  private taskQueueManager?: any;

  /**
   * Event Manager
   */
  private eventManager: EventRegistry;

  /**
   * Workflow Execution Registry
   */
  private workflowExecutionRegistry: {
    register: (entity: WorkflowExecutionEntity) => void;
    registerStateCoordinator: (
      executionId: string,
      stateCoordinator: WorkflowStateCoordinator,
    ) => void;
    get: (id: string) => WorkflowExecutionEntity | undefined;
  };

  /**
   * Workflow Execution Builder
   */
  private executionBuilder: {
    build: (
      subgraphId: string,
      options: { input: Record<string, unknown> },
    ) => Promise<WorkflowExecutionBuildResultSimple>;
    createChildExecution?: (
      parent: WorkflowExecutionEntity,
      options: { type: ChildExecutionType; config: ChildExecutionConfig },
    ) => Promise<WorkflowExecutionBuildResultSimple>;
  };

  /**
   * Completion Handler State
   */
  private callbackState: AsyncCompletionManager<ExecutedSubworkflowResult>;

  /**
   * Active Task Mapping
   * Used to track and manage tasks that are executed asynchronously, to prevent memory leaks.
   */
  private activeTasks: Map<
    string,
    { taskId: string; executionId: string; submitTime: number; timeout: number }
  > = new Map();

  /**
   * Agent Execution Registry - for resolving agent loop entity data
   */
  private agentExecutionRegistry: IAgentExecutionRegistry;

  /**
   * Constructor
   * @param taskRegistry Task registry (injected from DI container)
   * @param workflowExecutionRegistry WorkflowExecution registry
   * @param executionBuilder WorkflowExecution builder
   * @param taskQueueManager Task queue manager
   * @param eventManager Event manager
   * @param workflowExecutionPool Workflow execution pool service
   * @param agentExecutionRegistry Agent execution registry
   */
  constructor(
    taskRegistry: TaskRegistry,
    workflowExecutionRegistry: {
      register: (entity: WorkflowExecutionEntity) => void;
      registerStateCoordinator: (
        executionId: string,
        stateCoordinator: WorkflowStateCoordinator,
      ) => void;
      get: (id: string) => WorkflowExecutionEntity | undefined;
    },
    executionBuilder: {
      build: (
        subgraphId: string,
        options: { input: Record<string, unknown> },
      ) => Promise<WorkflowExecutionBuildResultSimple>;
      createChildExecution?: (
        parent: WorkflowExecutionEntity,
        options: { type: ChildExecutionType; config: ChildExecutionConfig },
      ) => Promise<WorkflowExecutionBuildResultSimple>;
    },
    taskQueueManager: any,
    eventManager: EventRegistry,
    workflowExecutionPool: WorkflowExecutionPool,
    agentExecutionRegistry: IAgentExecutionRegistry,
  ) {
    this.taskRegistry = taskRegistry;
    this.workflowExecutionRegistry = workflowExecutionRegistry;
    this.executionBuilder = executionBuilder;
    this.taskQueueManager = taskQueueManager;
    this.eventManager = eventManager;
    this.workflowExecutionPool = workflowExecutionPool;
    this.agentExecutionRegistry = agentExecutionRegistry;

    // Create an async completion manager with event integration
    this.callbackState = new AsyncCompletionManager<ExecutedSubworkflowResult>(eventManager);
  }

  /**
   * Execute the trigger workflow
   * @param task The trigger workflow task
   * @returns Execution result (synchronous) or task submission result (asynchronous)
   */
  async executeTriggeredSubgraph(
    task: TriggeredSubworkflowTask,
  ): Promise<ExecutedSubworkflowResult | TaskSubmissionResult> {
    // Verify parameters
    if (!task.subworkflowId) {
      throw new RuntimeValidationError("subworkflowId is required", {
        operation: "executeTriggeredSubgraph",
        field: "subworkflowId",
      });
    }

    if (!task.mainWorkflowExecutionEntity) {
      throw new RuntimeValidationError("mainWorkflowExecutionEntity is required", {
        operation: "executeTriggeredSubgraph",
        field: "mainWorkflowExecutionEntity",
      });
    }

    // Prepare the input data
    const input = await this.prepareInputData(task);

    // Create a sub-workflow WorkflowExecutionEntity using unified API if available
    let subgraphEntity: WorkflowExecutionEntity;

    let buildResult: WorkflowExecutionBuildResultSimple;

    if (this.executionBuilder.createChildExecution) {
      // Use new unified API
      buildResult = await this.executionBuilder.createChildExecution(
        task.mainWorkflowExecutionEntity,
        {
          type: "TRIGGERED",
          config: {
            subworkflowId: task.subworkflowId,
            inputMapping: task.config?.inputMapping,
            async: task.config?.waitForCompletion !== true,
          },
        },
      );
      subgraphEntity = buildResult.workflowExecutionEntity;
    } else {
      // Fallback to old API for backward compatibility during migration
      buildResult = await this.executionBuilder.build(task.subworkflowId, {
        input,
      });
      subgraphEntity = buildResult.workflowExecutionEntity;
      subgraphEntity.setExecutionType("TRIGGERED_SUBWORKFLOW");
    }

    // Register with WorkflowExecutionRegistry
    this.workflowExecutionRegistry.register(subgraphEntity);

    // Register state coordinator
    this.workflowExecutionRegistry.registerStateCoordinator(
      subgraphEntity.id,
      buildResult.stateCoordinator,
    );

    // Set triggered subworkflow ID
    subgraphEntity.setTriggeredSubworkflowId(task.subworkflowId);

    // Trigger the start event
    await this.emitStartedEvent(task, subgraphEntity);

    // Select the execution method based on the configuration.
    const waitForCompletion = task.config?.waitForCompletion !== false; // The default value is `true`.
    const timeout = task.config?.timeout || this.workflowExecutionPool.getConfig().defaultTimeout;

    if (waitForCompletion) {
      // Synchronize execution
      return await this.executeSync(subgraphEntity, timeout);
    } else {
      // Asynchronous execution
      return await this.executeAsync(subgraphEntity, timeout);
    }
  }

  /**
   * Prepare input data using resolved data source
   * @param task: Trigger the sub-workflow task
   * @returns: Input data
   */
  private async prepareInputData(task: TriggeredSubworkflowTask): Promise<Record<string, unknown>> {
    const config = task.config as ExecuteTriggeredSubworkflowActionConfig | undefined;

    // Step 1: Resolve data source based on sourceType
    const source = await this.resolveDataSource(task);

    // Step 2: Build base input according to source type
    const baseInput: Record<string, unknown> = {
      triggerId: task.triggerId,
    };

    if (source.type === "agent") {
      baseInput["agentMessages"] = source.messages as unknown as Record<string, unknown>[];
      baseInput["agentLoopId"] = source.entityId;
      baseInput["agentIteration"] = source.agentState?.currentIteration;
      baseInput["agentToolCallCount"] = source.agentState?.toolCallCount;
      baseInput["agentStatus"] = source.agentState?.status;
      // Also include parent workflow data for agent-triggered subworkflows
      const parentEntity = task.mainWorkflowExecutionEntity;
      baseInput["parentOutput"] = parentEntity.getOutput();
      baseInput["parentInput"] = parentEntity.getInput();
    } else {
      // Workflow source: backward compatible behavior
      const mainEntity = task.mainWorkflowExecutionEntity;
      baseInput["output"] = mainEntity.getOutput();
      baseInput["input"] = mainEntity.getInput();
    }

    // Step 3: Apply input mapping if configured
    if (config?.inputMapping) {
      const mappedInput: Record<string, unknown> = { ...baseInput };

      // Map variables
      if (config.inputMapping.variables) {
        for (const [parentVar, subworkflowInput] of Object.entries(config.inputMapping.variables)) {
          const value = task.mainWorkflowExecutionEntity.getVariable(parentVar);
          if (value !== undefined) {
            mappedInput[subworkflowInput] = value;
          }
        }
      }

      // Map message contexts: parent context ID → subworkflow input name
      if (config.inputMapping.messageContexts) {
        const parentEntity = task.mainWorkflowExecutionEntity;
        const parentExecution = parentEntity.getWorkflowExecutionData() as
          WorkflowExecution & { messageContextRegistry?: MessageContextRegistry };
        const registry = parentExecution.messageContextRegistry;

        if (registry) {
          for (const [contextId, inputName] of Object.entries(config.inputMapping.messageContexts)) {
            const context = registry.get(contextId);
            if (context) {
              mappedInput[inputName] = context.messages;
            } else {
              logger.warn("Message context not found for mapping", {
                contextId,
                inputName,
              });
            }
          }
        } else {
          logger.warn("MessageContextRegistry not available on parent execution");
        }
      }

      // Add additional static parameters
      if (config.inputMapping.additionalParams) {
        Object.assign(mappedInput, config.inputMapping.additionalParams);
      }

      return mappedInput;
    }

    return baseInput;
  }

  /**
   * Resolve data source based on task.sourceType
   *
   * Provides a unified data interface regardless of whether the source
   * is a workflow execution or an agent loop execution.
   *
   * Design:
   * - Uses lazy lookup (delayed query) to ensure the latest data snapshot
   * - Gracefully falls back to workflow source when agent entity is not found
   *
   * @param task: The triggered subworkflow task
   * @returns Resolved data source with unified interface
   */
  private async resolveDataSource(task: TriggeredSubworkflowTask): Promise<ResolvedDataSource> {
    if (task.sourceType === "agent" && task.sourceEntityId) {
      try {
        const agentEntity = await this.agentExecutionRegistry.get(task.sourceEntityId);
        if (agentEntity) {
          // Retrieve messages from the AgentStateCoordinator (single data source)
          const stateCoordinator = this.agentExecutionRegistry.getStateCoordinator(agentEntity.id);
          return {
            type: "agent",
            entityId: agentEntity.id,
            messages: stateCoordinator?.getMessages() ?? [],
            agentState: {
              currentIteration: agentEntity.state.currentIteration,
              toolCallCount: agentEntity.state.toolCallCount,
              status: agentEntity.getStatus(),
            },
          };
        }
        logger.warn(
          "Agent entity not found for data source resolution, falling back to workflow source",
          {
            entityId: task.sourceEntityId,
          },
        );
      } catch (error) {
        logger.warn("Failed to resolve agent entity data source, falling back to workflow source", {
          entityId: task.sourceEntityId,
          error: getErrorOrNew(error),
        });
      }
    }

    // Default: workflow data source
    const entity = task.mainWorkflowExecutionEntity;
    return {
      type: "workflow",
      entityId: entity.id,
      output: entity.getOutput(),
      workflowInput: entity.getInput(),
      variables: entity.getAllVariables(),
    };
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
  ): Promise<ExecutedSubworkflowResult> {
    // First, register the task with the global TaskRegistry.
    const taskId = this.taskRegistry.register(subgraphEntity, "workflowExecution", this, timeout);

    try {
      const result = await this.taskQueueManager.submitSync(taskId, subgraphEntity, timeout);

      // Cleanup on success
      await this.cleanupCompletedTask(subgraphEntity, taskId);

      return result;
    } catch (error) {
      // Cleanup on failure
      await this.cleanupFailedTask(subgraphEntity, taskId);
      throw error;
    }
  }

  /**
   * Asynchronous execution
   * @param subgraphEntity: Sub-workflow entity
   * @param timeout: Timeout period
   * @returns: Task submission result
   */
  private async executeAsync(
    subgraphEntity: WorkflowExecutionEntity,
    timeout: number,
  ): Promise<TaskSubmissionResult> {
    const executionId = subgraphEntity.id;

    // First, register the task with the global TaskRegistry.
    const taskId = this.taskRegistry.register(subgraphEntity, "workflowExecution", this, timeout);

    // Submit to the task queue
    const submissionResult = this.taskQueueManager.submitAsync(taskId, subgraphEntity, timeout);

    // Register the completion handler directly without creating a Promise.
    // This can prevent memory leaks caused by uncleaned Promise references.
    await this.callbackState.registerHandler(
      executionId,
      (result: ExecutedSubworkflowResult) => this.handleSubgraphCompleted(executionId, result),
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
  private async handleSubgraphCompleted(
    executionId: string,
    result: ExecutedSubworkflowResult,
  ): Promise<void> {
    // Trigger the completion event
    this.emitCompletedEvent(executionId, result);

    // Get task info for cleanup
    const taskInfo = this.taskRegistry
      .getAll()
      .find(t => t.instanceType === "workflowExecution" && t.instance.id === executionId);

    if (taskInfo) {
      // Cleanup using unified function
      const subgraphEntity = taskInfo.instance as WorkflowExecutionEntity;
      const parentEntity = this.getParentEntity(subgraphEntity);

      if (parentEntity) {
        await cleanupChildExecution(subgraphEntity, parentEntity, "COMPLETED");
      }

      // Clean up task records
      this.taskRegistry.delete(taskInfo.id);
    }

    // Clean up active task information
    this.activeTasks.delete(executionId);

    // Finally trigger the completion handler (the handler will be cleaned up internally).
    // The `triggerCompletion` function already cleans up the handlers internally, so there is no need to call the `cleanupHandler` function as well.
    await this.callbackState.triggerCompletion(executionId, result);
  }

  /**
   * Sub-workflow processing failed.
   * @param executionId: Execution ID
   * @param error: Error message
   */
  private async handleSubgraphFailed(executionId: string, error: Error): Promise<void> {
    // Trigger a failure event.
    this.emitFailedEvent(executionId, error);

    // Get task info for cleanup
    const taskInfo = this.taskRegistry
      .getAll()
      .find(t => t.instanceType === "workflowExecution" && t.instance.id === executionId);

    if (taskInfo) {
      // Cleanup using unified function
      const subgraphEntity = taskInfo.instance as WorkflowExecutionEntity;
      const parentEntity = this.getParentEntity(subgraphEntity);

      if (parentEntity) {
        await cleanupChildExecution(subgraphEntity, parentEntity, "FAILED");
      }

      // Clean up task records
      this.taskRegistry.delete(taskInfo.id);
    }

    // Clean up active task information
    this.activeTasks.delete(executionId);

    // The final error handler is triggered (the handler will be cleaned up internally).
    // The `triggerError` function already cleans up the handler internally, so there is no need to call the `cleanupHandler` function again.
    await this.callbackState.triggerError(executionId, error);
  }

  /**
   * Get parent entity from child
   */
  private getParentEntity(
    childEntity: WorkflowExecutionEntity,
  ): WorkflowExecutionEntity | undefined {
    const parentContext = childEntity.getParentContext();
    if (parentContext) {
      return this.workflowExecutionRegistry.get(parentContext.parentId);
    }
    return undefined;
  }

  /**
   * Cleanup completed task using unified function
   */
  private async cleanupCompletedTask(
    entity: WorkflowExecutionEntity,
    taskId: string,
  ): Promise<void> {
    const parentEntity = this.getParentEntity(entity);

    if (parentEntity) {
      await cleanupChildExecution(entity, parentEntity, "COMPLETED");
    }

    this.taskRegistry.delete(taskId);
    this.activeTasks.delete(entity.id);
  }

  /**
   * Cleanup failed task using unified function
   */
  private async cleanupFailedTask(entity: WorkflowExecutionEntity, taskId: string): Promise<void> {
    const parentEntity = this.getParentEntity(entity);

    if (parentEntity) {
      await cleanupChildExecution(entity, parentEntity, "FAILED");
    }

    this.taskRegistry.delete(taskId);
    this.activeTasks.delete(entity.id);
  }

  /**
   * Trigger the start event
   * @param task: Task that triggers the sub-workflow
   * @param subgraphEntity: Sub-workflow entity
   */
  private async emitStartedEvent(
    task: TriggeredSubworkflowTask,
    _subgraphEntity: WorkflowExecutionEntity,
  ): Promise<void> {
    const startedEvent = buildTriggeredSubgraphStartedEvent({
      executionId: task.mainWorkflowExecutionEntity.id,
      workflowId: task.mainWorkflowExecutionEntity.getWorkflowId(),
      subgraphId: task.subworkflowId,
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
    result: ExecutedSubworkflowResult,
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
      if (taskInfo && taskInfo.instanceType === "workflowExecution") {
        // Use unified cleanup function for parent-child relationship
        const subgraphEntity = taskInfo.instance as WorkflowExecutionEntity;
        const parentEntity = this.getParentEntity(subgraphEntity);

        if (parentEntity) {
          await cleanupChildExecution(subgraphEntity, parentEntity, "CANCELLED");
        }
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
   * Get workflow execution pool statistics
   * @returns Workflow execution pool statistics
   */
  getPoolStats() {
    return this.workflowExecutionPool.getStats();
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
    await this.callbackState.cleanup();

    // Wait for all tasks to complete.
    await this.taskQueueManager.drain();

    // Close the workflow execution pool.
    await this.workflowExecutionPool.shutdown();

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
