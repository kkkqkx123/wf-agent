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
import { getContainer } from "../../../core/di/index.js";
import * as Identifiers from "../../../core/di/service-identifiers.js";
import type { AgentLoopRegistry } from "../../../agent/loop/agent-loop-registry.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "WorkflowLifecycleCoordinator" });

/**
 * Workflow Lifecycle Coordinator
 *
 * Responsible for high-level process orchestration and coordination, organizing multiple components to complete complex workflow execution lifecycle operations.
 */
export class WorkflowLifecycleCoordinator {
  constructor(
    private readonly workflowExecutionRegistry: WorkflowExecutionRegistry,
    private readonly workflowStateTransitor: WorkflowStateTransitor,
    private readonly workflowExecutionBuilder: WorkflowExecutionBuilder,
    private readonly workflowExecutor: WorkflowExecutor,
  ) {}

  /**
   * Execute Workflow
   *
   * @param workflowId: Workflow ID
   * @param options: Execution options
   * @returns: Execution result
   */
  async execute(workflowId: string, options: WorkflowExecutionOptions = {}): Promise<WorkflowExecutionResult> {
    // Step 1: Construct the WorkflowExecutionEntity
    const { workflowExecutionEntity } = await this.workflowExecutionBuilder.build(workflowId, options);

    // Step 2: Register WorkflowExecutionEntity
    this.workflowExecutionRegistry.register(workflowExecutionEntity);

    // Step 3: Start the Workflow Execution
    await this.workflowStateTransitor.startWorkflowExecution(workflowExecutionEntity);

    // Step 4: Execute the Workflow
    const result = await this.workflowExecutor.executeWorkflow(workflowExecutionEntity);

    // Step 5: Update the workflow execution status based on the execution results.
    const status = result.metadata?.status;
    const isSuccess = status === "COMPLETED";

    if (isSuccess) {
      await this.workflowStateTransitor.completeWorkflowExecution(workflowExecutionEntity, result);
    } else {
      // Get the first error from the errors array
      const errors = workflowExecutionEntity.getErrors();
      const lastError =
        errors.length > 0 ? (errors[errors.length - 1] as Error) : new Error("Execution failed");
      await this.workflowStateTransitor.failWorkflowExecution(workflowExecutionEntity, lastError);
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
   * @param executionId: Workflow Execution ID
   * @throws NotFoundError: The workflow execution context does not exist.
   */
  async pauseWorkflowExecution(executionId: string): Promise<void> {
    const workflowExecutionEntity = this.workflowExecutionRegistry.get(executionId);
    if (!workflowExecutionEntity) {
      throw new WorkflowExecutionNotFoundError(`WorkflowExecutionEntity not found`, executionId);
    }

    // 1. Request to pause (InterruptionState will automatically trigger the AbortController)
    workflowExecutionEntity.interrupt("PAUSE");

    // 2. Fully delegate the state transition and event triggering to the Transitor.
    await this.workflowStateTransitor.pauseWorkflowExecution(workflowExecutionEntity);
  }

  /**
   * Resume Workflow Execution
   *
   * Process:
   * 1. Obtain the workflow execution context.
   * 2. Update the workflow execution status to RUNNING.
   * 3. Clear the pause flag.
   * 4. Continue executing the workflow.
   *
   * @param executionId: Workflow Execution ID
   * @returns: Execution result
   * @throws: NotFoundError: The workflow execution context does not exist.
   */
  async resumeWorkflowExecution(executionId: string): Promise<WorkflowExecutionResult> {
    const workflowExecutionEntity = this.workflowExecutionRegistry.get(executionId);
    if (!workflowExecutionEntity) {
      throw new WorkflowExecutionNotFoundError(`WorkflowExecutionEntity not found`, executionId);
    }

    // 1. Fully delegate the state transition and event triggering to the Manager.
    await this.workflowStateTransitor.resumeWorkflowExecution(workflowExecutionEntity);

    // 2. Reset the interrupt status (including the AbortController)
    workflowExecutionEntity.resetInterrupt();

    // 3. Continue execution
    return await this.workflowExecutor.executeWorkflow(workflowExecutionEntity);
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
   *
   * @param executionId: Workflow Execution ID
   * @throws NotFoundError: The workflow execution context does not exist.
   */
  async stopWorkflowExecution(executionId: string): Promise<void> {
    const workflowExecutionEntity = this.workflowExecutionRegistry.get(executionId);
    if (!workflowExecutionEntity) {
      throw new WorkflowExecutionNotFoundError(`WorkflowExecutionEntity not found`, executionId);
    }

    // 1. Request to stop (InterruptionState will automatically trigger the AbortController)
    workflowExecutionEntity.interrupt("STOP");

    // 2. Fully delegate the state transitions and event triggering to the Manager.
    await this.workflowStateTransitor.cancelWorkflowExecution(workflowExecutionEntity, "user_requested");

    // 3. Cascading cancellation of child workflow executions
    await this.workflowStateTransitor.cascadeCancel(executionId);

    // 4. Cleanup child AgentLoops
    await this.cleanupChildAgentLoops(executionId);
  }

  /**
   * Cleanup child AgentLoops associated with a workflow execution
   * @param executionId Workflow Execution ID
   */
  private async cleanupChildAgentLoops(executionId: string): Promise<void> {
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
   * @param executionId: Workflow Execution ID
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
   * @param executionId: Workflow Execution ID
   */
  async forcePauseWorkflowExecution(executionId: string): Promise<void> {
    await this.forceSetWorkflowExecutionStatus(executionId, "PAUSED");
  }

  /**
   * Forcibly cancel a workflow execution (delegated to WorkflowStateTransitor)
   * @param executionId: Workflow Execution ID
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
