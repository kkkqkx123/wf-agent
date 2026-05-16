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
import type { GlobalContext } from "../../../core/global-context.js";
import * as Identifiers from "../../../core/di/service-identifiers.js";
import type { ExecutionHierarchyRegistry } from "../../../core/registry/execution-hierarchy-registry.js";
import { CheckpointCoordinator } from "../../checkpoint/checkpoint-coordinator.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import { PauseTimeoutManager } from "../utils/pause-timeout-manager.js";
import type { MetricsRegistry } from "../../../core/metrics/metrics-registry.js";

const logger = createContextualLogger({ component: "WorkflowLifecycleCoordinator" });

/**
 * Workflow Lifecycle Coordinator
 *
 * Responsible for high-level process orchestration and coordination, organizing multiple components to complete complex workflow execution lifecycle operations.
 */
export class WorkflowLifecycleCoordinator {
  private pauseTimeoutManager?: PauseTimeoutManager;

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
   * Initialize pause timeout manager (optional)
   * Call this to enable automatic timeout for paused workflows
   */
  initializePauseTimeout(config?: {
    maxPauseDuration?: number;
    warningThreshold?: number;
  }): void {
    const eventManager = this.globalContext.eventRegistry;
    
    this.pauseTimeoutManager = new PauseTimeoutManager(
      this.workflowExecutionRegistry,
      eventManager,
      config,
    );
    
    logger.info("Pause timeout manager initialized", {
      maxPauseDuration: config?.maxPauseDuration || 24 * 60 * 60 * 1000,
      warningThreshold: config?.warningThreshold || 60 * 60 * 1000,
    });
  }

  /**
   * Execute Workflow
   *
   * @param workflowId: Workflow ID
   * @param options: Execution options
   * @returns: Execution result
   */
  async execute(workflowId: string, options: WorkflowExecutionOptions = {}): Promise<WorkflowExecutionResult> {
    const startTime = Date.now();
    
    // Step 1: Construct the WorkflowExecutionEntity
    const { workflowExecutionEntity } = await this.workflowExecutionBuilder.build(workflowId, options);
    const executionId = workflowExecutionEntity.id;
    const workflowVersion = workflowExecutionEntity.getWorkflowVersion();

    // Record workflow execution start in metrics
    const workflowCollector = this.metricsRegistry.getWorkflowCollector();
    if (workflowCollector) {
      workflowCollector.recordExecutionStart(
        workflowId,
        executionId,
        {
          version: workflowVersion,
          executionType: 'MAIN',
        }
      );
    }

    // Step 2: Register WorkflowExecutionEntity
    this.workflowExecutionRegistry.register(workflowExecutionEntity);
    
    // Step 2.5: Register with ExecutionHierarchyRegistry for unified hierarchy management
    const hierarchyRegistry = this.globalContext.container.get(
      Identifiers.ExecutionHierarchyRegistry,
    ) as ExecutionHierarchyRegistry;
    hierarchyRegistry.register(workflowExecutionEntity);

    // Step 3: Start the Workflow Execution
    await this.workflowStateTransitor.startWorkflowExecution(workflowExecutionEntity);

    // Step 4: Execute the Workflow
    const result = await this.workflowExecutor.executeWorkflow(workflowExecutionEntity);
    const duration = Date.now() - startTime;

    // Step 5: Update the workflow execution status based on the execution results.
    const status = result.metadata?.status;
    const isSuccess = status === "COMPLETED";
    const nodeCount = workflowExecutionEntity.getNodeResults().length;

    try {
      if (isSuccess) {
        await this.workflowStateTransitor.completeWorkflowExecution(workflowExecutionEntity, result);
      } else {
        // Get the first error from the errors array
        const errors = workflowExecutionEntity.getErrors();
        const lastError =
          errors.length > 0 ? (errors[errors.length - 1] as Error) : new Error("Execution failed");
        await this.workflowStateTransitor.failWorkflowExecution(workflowExecutionEntity, lastError);
      }
    } finally {
      // Step 6: Cleanup execution-scoped event listeners (ensure cleanup in all cases)
      const cleanedCount = this.globalContext.eventRegistry.cleanupExecutionListeners(workflowExecutionEntity.id);
      logger.info('Cleaned up event listeners after execution', { 
        executionId: workflowExecutionEntity.id, 
        cleanedCount,
        status 
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

    // Start monitoring for pause timeout (if enabled)
    if (this.pauseTimeoutManager) {
      this.pauseTimeoutManager.startMonitoring(executionId);
      logger.debug("Started pause timeout monitoring", { executionId });
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

    // Stop pause timeout monitoring (if enabled)
    if (this.pauseTimeoutManager) {
      this.pauseTimeoutManager.stopMonitoring(executionId);
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
      const { workflowExecutionEntity: restoredEntity } = await CheckpointCoordinator.restoreFromCheckpoint(
        latestCheckpointId,
        checkpointDeps,
      );

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
    logger.info('Cleaned up event listeners after resume', { 
      executionId, 
      cleanedCount,
      status: result.metadata?.status 
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

    // Stop pause timeout monitoring (if enabled)
    if (this.pauseTimeoutManager) {
      this.pauseTimeoutManager.stopMonitoring(executionId);
      logger.debug("Stopped pause timeout monitoring", { executionId });
    }

    // Fully delegate the state transitions and event triggering to the Manager.
    await this.workflowStateTransitor.cancelWorkflowExecution(workflowExecutionEntity, "user_requested");

    // 3. Cascading cancellation of child workflow executions
    await this.workflowStateTransitor.cascadeCancel(executionId);

    // 4. Cleanup child AgentLoops
    await this.cleanupChildAgentLoops(executionId);
    
    // 5. Cleanup execution-scoped event listeners
    const cleanedCount = this.globalContext.eventRegistry.cleanupExecutionListeners(executionId);
    logger.info('Cleaned up event listeners', { executionId, cleanedCount });
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
  async forceSetWorkflowExecutionStatus(executionId: string, status: WorkflowExecutionStatus): Promise<void> {
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
        await this.workflowStateTransitor.cancelWorkflowExecution(workflowExecutionEntity, "forced");
        break;
      default:
        throw new RuntimeValidationError(`Unsupported status for forceSetWorkflowExecutionStatus: ${status}`, {
          operation: "forceSetWorkflowExecutionStatus",
          field: "status",
          value: status,
        });
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

    await this.workflowStateTransitor.cancelWorkflowExecution(workflowExecutionEntity, reason || "forced_cancel");
  }
}
