/**
 * Workflow Lifecycle Coordinator
 *
 * Responsibilities:
 * - Coordinate the full lifecycle management of workflow executions
 * - Schedule complex operations such as workflow execution creation, execution, suspension, resumption, and termination
 * - Handle multi-step processes and event waiting logic
 *
 * Design Principles:
 * - Stateless design: Does not hold any instance variables
 * - Dependency injection: Receives dependencies through the constructor
 * - Process orchestration: Manages complex multi-step operations and event synchronization
 * - Delegation pattern: Uses the WorkflowStateTransitor for atomic state operations
 *
 * Call Path:
 * - External calls: WorkflowExecutorAPI -> WorkflowLifecycleCoordinator
 * - Trigger handling functions should call the Coordinator, not the Manager
 * - The Manager is only used internally as implementation detail for the Coordinator
 */

import { WorkflowExecutionNotFoundError, RuntimeValidationError } from "@wf-agent/types";
import type { WorkflowExecutionOptions, WorkflowExecutionResult } from "@wf-agent/types";
import { WorkflowExecutionStatus } from "@wf-agent/types";
import { WorkflowExecutionBuilder } from "../factories/workflow-execution-builder.js";
import { WorkflowExecutor } from "../executors/workflow-executor.js";
import { WorkflowStateTransitor } from "./workflow-state-transitor.js";
import type { WorkflowExecutionRegistry } from "../../stores/workflow-execution-registry.js";
import type { GlobalContext } from "../../../shared/global-context.js";
import * as Identifiers from "../../../di/service-identifiers.js";
import type { ExecutionHierarchyRegistry } from "../../../shared/registry/execution-hierarchy-registry.js";
import { CheckpointCoordinator } from "../../checkpoint/checkpoint-coordinator.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import type { MetricsRegistry } from "../../../metrics/metrics-registry.js";
import type { TimeoutHandle } from "../../../shared/types/timeout.js";

const logger = createContextualLogger({ component: "WorkflowLifecycleCoordinator" });

/**
 * Workflow Lifecycle Coordinator
 *
 * Responsible for high-level process orchestration and coordination, organizing multiple components to complete complex workflow execution lifecycle operations.
 */
export class WorkflowLifecycleCoordinator {
  /** Phase 3: Track pause timeout handles for each execution */
  private pauseTimeoutHandles: Map<string, TimeoutHandle> = new Map();

  constructor(
    private readonly workflowExecutionRegistry: WorkflowExecutionRegistry,
    private readonly workflowStateTransitor: WorkflowStateTransitor,
    private readonly workflowExecutionBuilder: WorkflowExecutionBuilder,
    private readonly workflowExecutor: WorkflowExecutor,
    private readonly globalContext: GlobalContext,
  ) {
    // Get metrics registry from global context
    this.metricsRegistry = this.globalContext.metricsRegistry;
  }

  private readonly metricsRegistry: MetricsRegistry;

  /**
   * Execute Workflow
   *
   * Unified execution path supporting both root and triggered executions:
   * - Root: Direct execution via executeRootWorkflow()
   * - Triggered: Queue-based execution via ExecutionQueueManager
   *
   * @param workflowId: Workflow ID
   * @param options: Execution options
   * @returns: Execution result
   */
  async execute(
    workflowId: string,
    options: WorkflowExecutionOptions = {},
  ): Promise<WorkflowExecutionResult> {
    // Step 1: Construct the WorkflowExecutionEntity
    const { workflowExecutionEntity, stateCoordinator } = await this.workflowExecutionBuilder.build(
      workflowId,
      options,
    );
    const executionId = workflowExecutionEntity.id;

    // Set node timeout from options (if provided)
    if (options.nodeTimeout !== undefined) {
      workflowExecutionEntity.setNodeTimeout(options.nodeTimeout);
    }

    // Set max pause duration from options (if provided)
    if (options.maxPauseDuration !== undefined) {
      workflowExecutionEntity.setMaxPauseDuration(options.maxPauseDuration);
    }

    // Set default node retry from options (if provided)
    if (options.defaultNodeRetry !== undefined) {
      workflowExecutionEntity.setDefaultNodeRetry(options.defaultNodeRetry);
    }

    // Initialize retry budget from options (if provided)
    if (options.retryBudget !== undefined) {
      workflowExecutionEntity.initializeRetryBudget({
        maxRetries: options.retryBudget.maxRetries,
        timeBudgetMs: options.retryBudget.timeBudgetMs,
        name: `workflow-${executionId}`,
      });
    }

    // Step 1.5: Check if this is a triggered execution
    const parentContext = workflowExecutionEntity.getParentContext();
    if (parentContext) {
      logger.info("Routing triggered workflow execution to TriggeredWorkflowExecutionManager", {
        executionId,
        parentId: parentContext.parentId,
        parentType: parentContext.parentType,
      });

      // Triggered execution: route to TriggeredWorkflowExecutionManager
      const triggeredWorkflowManager = this.globalContext.container.get(
        Identifiers.TriggeredWorkflowExecutionManager,
      );
      return await (triggeredWorkflowManager as any).submitTriggeredExecution(
        {
          executionId,
          parentEntity: workflowExecutionEntity,
          parentType: "WORKFLOW",
          waitForCompletion: true,
        },
        workflowExecutionEntity,
      );
    }

    // Root execution: proceed with standard execution path
    return await this.executeRootWorkflow(
      workflowExecutionEntity,
      stateCoordinator,
      workflowId,
      options.maxExecutionTime,
    );
  }

  /**
   * Execute root workflow (synchronous path)
   * @private
   */
  private async executeRootWorkflow(
    workflowExecutionEntity: import("../../entities/workflow-execution-entity.js").WorkflowExecutionEntity,
    stateCoordinator: import("../../state-managers/workflow-state-coordinator.js").WorkflowStateCoordinator,
    workflowId: string,
    maxExecutionTime?: number,
  ): Promise<WorkflowExecutionResult> {
    const executionId = workflowExecutionEntity.id;
    const workflowVersion = workflowExecutionEntity.getWorkflowVersion();

    // Record workflow execution start in metrics
    const workflowCollector = this.metricsRegistry.getWorkflowCollector();
    if (workflowCollector) {
      workflowCollector.recordExecutionStart(workflowId, executionId, {
        version: workflowVersion,
        executionType: "MAIN",
      });
    }

    // Step 2: Register WorkflowExecutionEntity
    this.workflowExecutionRegistry.register(workflowExecutionEntity);

    // Register state coordinator
    this.workflowExecutionRegistry.registerStateCoordinator(executionId, stateCoordinator);

    // Step 2.5: Register with ExecutionHierarchyRegistry for unified hierarchy management
    const hierarchyRegistry = this.globalContext.container.get(
      Identifiers.ExecutionHierarchyRegistry,
    ) as ExecutionHierarchyRegistry;
    hierarchyRegistry.register(workflowExecutionEntity);

    // Step 3: Start the Workflow Execution
    await this.workflowStateTransitor.startWorkflowExecution(workflowExecutionEntity);

    // Phase 4: Workflow Wall-Clock Timeout
    // Register a max execution time timeout on the entity's TimeoutManager
    let wallClockTimeoutHandle: TimeoutHandle | undefined;
    if (maxExecutionTime && maxExecutionTime > 0) {
      wallClockTimeoutHandle = workflowExecutionEntity.timeoutManager.register({
        id: `wall-clock-${executionId}`,
        duration: maxExecutionTime,
        onTimeout: () => {
          logger.warn("Workflow execution wall-clock timeout exceeded, stopping execution", {
            executionId,
            workflowId,
            maxExecutionTime,
          });
          workflowExecutionEntity.interrupt("STOP");
        },
        tag: "workflow-execution",
        metadata: {
          maxExecutionTime,
        },
        interruptionState: workflowExecutionEntity.getInterruptionState(),
      });
    }

    // Step 4: Execute the Workflow
    const result = await this.workflowExecutor.executeWorkflow(workflowExecutionEntity);

    // Step 5: Update the workflow execution status based on the execution results.
    const status = result.metadata?.status;
    const isSuccess = status === "COMPLETED";

    try {
      if (isSuccess) {
        await this.workflowStateTransitor.completeWorkflowExecution(
          workflowExecutionEntity,
          result,
        );
      } else {
        // Get the first error from the errors array
        const errors = workflowExecutionEntity.getErrors();
        const lastError =
          errors.length > 0 ? (errors[errors.length - 1] as Error) : new Error("Execution failed");
        await this.workflowStateTransitor.failWorkflowExecution(workflowExecutionEntity, lastError);
      }
    } finally {
      // Record workflow execution completion in metrics
      if (workflowCollector) {
        workflowCollector.recordExecutionComplete(workflowId, executionId, {
          success: isSuccess,
          duration: result.metadata?.executionTime ?? Date.now() - (workflowExecutionEntity.getStartTime() ?? Date.now()),
          nodeCount: result.metadata?.nodeCount ?? 0,
          errorType: isSuccess ? undefined : result.errors?.[0]?.type ?? "unknown",
        });
      }
      // Cancel the wall-clock timeout if it was registered
      if (wallClockTimeoutHandle) {
        wallClockTimeoutHandle.cancel();
      }

      // Step 6: Cleanup execution-scoped event listeners (ensure cleanup in all cases)
      const cleanedCount = this.globalContext.eventRegistry.cleanupExecutionListeners(
        workflowExecutionEntity.id,
      );
      logger.info("Cleaned up event listeners after execution", {
        executionId: workflowExecutionEntity.id,
        cleanedCount,
        status,
      });
    }

    return result;
  }

  /**
   * Pause Workflow Execution
   *
   * Process:
   * 1. Obtain the workflow execution context.
   * 2. Set the pause flag.
   * 3. Trigger the AbortController to interrupt any ongoing asynchronous operations.
   * 4. Update the workflow execution status.
   *
   * @param workflowExecutionId: Workflow Execution ID
   * @throws NotFoundError: The workflow execution context does not exist.
   */
  async pauseWorkflowExecution(executionId: string): Promise<void> {
    const workflowExecutionEntity = this.workflowExecutionRegistry.get(executionId);
    if (!workflowExecutionEntity) {
      throw new WorkflowExecutionNotFoundError(`WorkflowExecutionEntity not found`, executionId);
    }

    // Fully delegate the state transition and event triggering to the Transitor.
    await this.workflowStateTransitor.pauseWorkflowExecution(workflowExecutionEntity);

    // Phase 2: Set interrupt flag to propagate pause signal through InterruptionState
    // This triggers interruptionState.requestPause() which aborts AbortSignal,
    // emits EXECUTION_PAUSED event, and cascades to child executions.
    workflowExecutionEntity.interrupt("PAUSE");

    // Phase 3: Register pause timeout on the entity's TimeoutManager
    // (consistent with AgentLoopCoordinator.pause() pattern)
    const maxPauseDuration = workflowExecutionEntity.getMaxPauseDuration();
    if (maxPauseDuration && maxPauseDuration > 0) {
      // Cancel any existing pause timeout handle
      const existingHandle = this.pauseTimeoutHandles.get(executionId);
      if (existingHandle) {
        existingHandle.cancel();
      }

      const pauseHandle = workflowExecutionEntity.timeoutManager.register({
        id: `pause-${executionId}`,
        duration: maxPauseDuration,
        warningThreshold: Math.min(maxPauseDuration * 0.1, 3600000), // 10% of duration or 1 hour
        onWarning: async () => {
          logger.warn("Workflow execution pause timeout approaching", {
            executionId,
            maxPauseDuration,
          });
        },
        onTimeout: async () => {
          logger.warn("Workflow execution pause timeout exceeded, stopping execution", {
            executionId,
            maxPauseDuration,
          });
          // Use entity.interrupt("STOP") to properly abort AbortSignal and cascade to children
          workflowExecutionEntity.interrupt("STOP");
          // Cleanup the timeout handle from tracking
          this.pauseTimeoutHandles.delete(executionId);
        },
        tag: "workflow-pause",
        metadata: {
          executionId,
          maxPauseDuration,
        },
        interruptionState: workflowExecutionEntity.getInterruptionState(),
      });

      this.pauseTimeoutHandles.set(executionId, pauseHandle);

      logger.debug("Started pause timeout monitoring via entity.timeoutManager", {
        executionId,
        maxPauseDuration,
      });
    }
  }

  /**
   * Resume Workflow Execution
   *
   * Process:
   * 1. Obtain the workflow execution context.
   * 2. Restore from last checkpoint (if available).
   * 3. Update the workflow execution status to RUNNING.
   * 4. Clear the pause flag.
   * 5. Continue executing the workflow from restored position.
   *
   * @param workflowExecutionId: Workflow Execution ID
   * @returns: Execution result
   * @throws: NotFoundError: The workflow execution context does not exist.
   */
  async resumeWorkflowExecution(executionId: string): Promise<WorkflowExecutionResult> {
    logger.info("Resuming workflow execution", { executionId });

    // Phase 3: Cancel the pause timeout monitor (if active)
    const pauseHandle = this.pauseTimeoutHandles.get(executionId);
    if (pauseHandle) {
      pauseHandle.cancel();
      this.pauseTimeoutHandles.delete(executionId);
      logger.debug("Stopped pause timeout monitoring", { executionId });
    }

    // Restore from last checkpoint using CheckpointCoordinator
    const checkpointDeps = {
      workflowExecutionRegistry: this.workflowExecutionRegistry,
      checkpointStateManager: this.globalContext.container.get(Identifiers.CheckpointState),
      workflowRegistry: this.globalContext.container.get(Identifiers.WorkflowRegistry),
      workflowGraphRegistry: this.globalContext.container.get(Identifiers.WorkflowGraphRegistry),
      hierarchyRegistry: this.globalContext.container.get(Identifiers.ExecutionHierarchyRegistry),
    };

    // Find the latest checkpoint for this execution
    const checkpointStateManager = checkpointDeps.checkpointStateManager;
    const checkpointIds = await checkpointStateManager.list({
      parentId: executionId,
      limit: 1,
    });

    let workflowExecutionEntity: import("../../entities/workflow-execution-entity.js").WorkflowExecutionEntity;

    if (checkpointIds.length > 0 && checkpointIds[0]) {
      const latestCheckpointId = checkpointIds[0];
      logger.info("Restoring from checkpoint", {
        executionId,
        checkpointId: latestCheckpointId,
      });

      // Use CheckpointCoordinator to restore from checkpoint
      // This will create a new WorkflowExecutionEntity and register it in the registry
      const coordinator = new CheckpointCoordinator();
      const { workflowExecutionEntity: restoredEntity } =
        await coordinator.restoreWorkflowFromCheckpoint(latestCheckpointId, checkpointDeps);

      workflowExecutionEntity = restoredEntity;

      logger.info("Workflow restored from checkpoint", {
        executionId,
        restoredNodeId: workflowExecutionEntity.getCurrentNodeId(),
        checkpointId: latestCheckpointId,
      });
    } else {
      // No checkpoint found, get existing entity from registry
      const existingEntity = this.workflowExecutionRegistry.get(executionId);
      if (!existingEntity) {
        throw new WorkflowExecutionNotFoundError(`WorkflowExecutionEntity not found`, executionId);
      }
      workflowExecutionEntity = existingEntity;
      logger.warn("No checkpoint found, resuming from start", { executionId });
    }

    // 2. State transition
    await this.workflowStateTransitor.resumeWorkflowExecution(workflowExecutionEntity);

    // 3. Reset interrupt status (including AbortController)
    workflowExecutionEntity.resetInterrupt();

    // 4. Continue execution from restored position
    const result = await this.workflowExecutor.executeWorkflow(workflowExecutionEntity);

    // 5. Cleanup execution-scoped event listeners after resume completes
    const cleanedCount = this.globalContext.eventRegistry.cleanupExecutionListeners(executionId);
    logger.info("Cleaned up event listeners after resume", {
      executionId,
      cleanedCount,
      status: result.metadata?.status,
    });

    return result;
  }

  /**
   * Stop Workflow Execution
   *
   * Process:
   * 1. Obtain the workflow execution context.
   * 2. Set the stop flag.
   * 3. Trigger the AbortController to interrupt any ongoing asynchronous operations.
   * 4. Update the workflow execution status to CANCELLED.
   * 5. Cancel any child workflow executions recursively.
   * 6. Cleanup child AgentLoops.
   * 7. Cleanup execution-scoped event listeners.
   *
   * @param workflowExecutionId: Workflow Execution ID
   * @throws NotFoundError: The workflow execution context does not exist.
   */
  async stopWorkflowExecution(executionId: string): Promise<void> {
    const workflowExecutionEntity = this.workflowExecutionRegistry.get(executionId);
    if (!workflowExecutionEntity) {
      throw new WorkflowExecutionNotFoundError(`WorkflowExecutionEntity not found`, executionId);
    }

    // Phase 3: Cancel the pause timeout monitor (if active)
    const pauseHandle = this.pauseTimeoutHandles.get(executionId);
    if (pauseHandle) {
      pauseHandle.cancel();
      this.pauseTimeoutHandles.delete(executionId);
      logger.debug("Stopped pause timeout monitoring", { executionId });
    }

    // Fully delegate the state transitions and event triggering to the Manager.
    await this.workflowStateTransitor.cancelWorkflowExecution(
      workflowExecutionEntity,
      "user_requested",
    );

    // 3. Cascading cancellation of child workflow executions
    await this.workflowStateTransitor.cascadeCancel(executionId);

    // 4. Cleanup child AgentLoops
    await this.cleanupChildAgentLoops(executionId);

    // 5. Cleanup the workflow execution entity itself
    workflowExecutionEntity.cleanup();
    logger.info("Workflow execution entity cleaned up", { executionId });

    // 6. Cleanup execution-scoped event listeners
    const cleanedCount = this.globalContext.eventRegistry.cleanupExecutionListeners(executionId);
    logger.info("Cleaned up event listeners", { executionId, cleanedCount });

    // 7. Deregister from registry
    this.workflowExecutionRegistry.delete(executionId);
    logger.info("Workflow execution deregistered from registry", { executionId });
  }

  /**
   * Cleanup child AgentLoops associated with a workflow execution
   * @param executionId Workflow Execution ID
   */
  private async cleanupChildAgentLoops(executionId: string): Promise<void> {
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
      // Log error but don't throw - cleanup should not prevent workflow execution stopping
      logger.warn("Failed to cleanup child AgentLoops", {
        executionId,
        error: error instanceof Error ? error.message : String(error),
      });
    }
  }

  /**
   * Forcibly set the workflow execution status (directly delegated to WorkflowStateTransitor)
   *
   * Note: This is an emergency measure and should typically be used to manage workflow execution status through the normal lifecycle methods (pauseWorkflowExecution/resumeWorkflowExecution/stopWorkflowExecution). Use this method only when those methods are not functioning correctly.
   *
   * @param workflowExecutionId: Workflow Execution ID
   * @param status: New status
   *
   */
  async forceSetWorkflowExecutionStatus(
    executionId: string,
    status: WorkflowExecutionStatus,
  ): Promise<void> {
    const workflowExecutionEntity = this.workflowExecutionRegistry.get(executionId);
    if (!workflowExecutionEntity) {
      throw new WorkflowExecutionNotFoundError(`WorkflowExecutionEntity not found`, executionId);
    }

    switch (status) {
      case "PAUSED":
        await this.workflowStateTransitor.pauseWorkflowExecution(workflowExecutionEntity);
        break;
      case "RUNNING":
        await this.workflowStateTransitor.resumeWorkflowExecution(workflowExecutionEntity);
        break;
      case "CANCELLED":
        await this.workflowStateTransitor.cancelWorkflowExecution(
          workflowExecutionEntity,
          "forced",
        );
        break;
      default:
        throw new RuntimeValidationError(
          `Unsupported status for forceSetWorkflowExecutionStatus: ${status}`,
          {
            operation: "forceSetWorkflowExecutionStatus",
            field: "status",
            value: status,
          },
        );
    }
  }

  /**
   * Forcibly pause a workflow execution (delegated to WorkflowStateTransitor)
   * @param workflowExecutionId: Workflow Execution ID
   */
  async forcePauseWorkflowExecution(executionId: string): Promise<void> {
    await this.forceSetWorkflowExecutionStatus(executionId, "PAUSED");
  }

  /**
   * Forcibly cancel a workflow execution (delegated to WorkflowStateTransitor)
   * @param workflowExecutionId: Workflow Execution ID
   * @param reason: Reason for cancellation
   */
  async forceCancelWorkflowExecution(executionId: string, reason?: string): Promise<void> {
    const workflowExecutionEntity = this.workflowExecutionRegistry.get(executionId);
    if (!workflowExecutionEntity) {
      throw new WorkflowExecutionNotFoundError(`WorkflowExecutionEntity not found`, executionId);
    }

    await this.workflowStateTransitor.cancelWorkflowExecution(
      workflowExecutionEntity,
      reason || "forced_cancel",
    );
  }

  /**
   * Cleanup terminated (completed + failed + cancelled) workflow executions
   * @returns Number of instances cleaned up
   */
  cleanup(): number {
    const count = this.workflowExecutionRegistry.cleanupTerminated();
    if (count > 0) {
      logger.info("Workflow execution terminated instances cleaned up", { count });
    }
    return count;
  }

  /**
   * Destroy the coordinator and release all resources
   * Cleans up all active workflow executions and internal resources.
   */
  destroy(): void {
    logger.info("Destroying WorkflowLifecycleCoordinator");

    // Phase 3: Cancel all pause timeout monitors
    for (const [executionId, handle] of this.pauseTimeoutHandles) {
      handle.cancel();
      logger.debug("Cancelled pause timeout handle during destroy", { executionId });
    }
    this.pauseTimeoutHandles.clear();

    // Clean up all entities in the registry
    this.workflowExecutionRegistry.clear();

    logger.info("WorkflowLifecycleCoordinator destroyed");
  }
}
