/**
 * WorkflowExecutor - Workflow Executor
 * Responsible for executing a single WorkflowExecutionEntity instance and managing the entire lifecycle of the workflow execution.
 * Supports node navigation using the workflow navigator.
 *
 * Responsibilities:
 * - Executes a single WorkflowExecutionEntity.
 * - Coordinates the execution of various components to complete the task.
 *
 * Does not handle:
 * - The creation and registration of workflow executions (handled by WorkflowLifecycleCoordinator).
 * - Lifecycle management tasks such as pausing, resuming, and stopping workflow executions (handled by WorkflowLifecycleCoordinator).
 * - Variable setting and other management operations (handled by WorkflowLifecycleCoordinator).
 * - Details of node execution (handled by NodeExecutionCoordinator).
 * - Error handling (handled by ErrorHandler).
 * - Subgraph processing (handled by SubgraphHandler).
 * - Triggering sub-workflow processing (handled by TriggeredSubworkflowHandler).
 *
 * Design Principles:
 * - Stateless design; all state is managed through WorkflowExecutionEntity.
 * - Supports pausing/resuming functionality.
 * - Supports interruption control (AbortController).
 * - Consistent architecture with LLMExecutor and ToolCallExecutor.
 */

import type { WorkflowExecutionResult } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../entities/workflow-execution-entity.js";
import type { WorkflowGraphRegistry } from "../../stores/workflow-graph-registry.js";
import type { WorkflowExecutionCoordinator } from "../coordinators/workflow-execution-coordinator.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "workflow-executor" });

/**
 * WorkflowExecutionCoordinator factory interface
 */
interface WorkflowExecutionCoordinatorFactory {
  create(workflowExecutionEntity: WorkflowExecutionEntity): WorkflowExecutionCoordinator;
}

/**
 * WorkflowExecutor dependency configuration
 */
export interface WorkflowExecutorDependencies {
  /** Workflow Graph Registry */
  workflowGraphRegistry: WorkflowGraphRegistry;
  /** WorkflowExecutionCoordinator factory */
  workflowExecutionCoordinatorFactory: WorkflowExecutionCoordinatorFactory;
}

/**
 * WorkflowExecutor - A stateless workflow executor
 *
 * Focuses on executing a single WorkflowExecutionEntity, without being responsible for workflow execution creation, registration, or management.
 * Delegates specific responsibilities to dedicated components through the coordinator pattern.
 *
 * Design principles:
 * - Stateless design: Does not retain any internal state.
 * - All state is passed in through the WorkflowExecutionEntity parameters.
 * - Dependencies are injected through the constructor (Dependency Inversion).
 * - Keeps a lightweight design, focusing solely on execution coordination.
 * - Does not directly depend on the DI container, which facilitates testing.
 * - Its lifecycle is managed by the DI container.
 */
export class WorkflowExecutor {
  private workflowGraphRegistry: WorkflowGraphRegistry;
  private workflowExecutionCoordinatorFactory: WorkflowExecutionCoordinatorFactory;

  constructor(deps: WorkflowExecutorDependencies) {
    this.workflowGraphRegistry = deps.workflowGraphRegistry;
    this.workflowExecutionCoordinatorFactory = deps.workflowExecutionCoordinatorFactory;
  }

  /**
   * Execute WorkflowExecutionEntity
   * @param workflowExecutionEntity: An instance of WorkflowExecutionEntity
   * @returns: The execution result
   */
  async executeWorkflow(workflowExecutionEntity: WorkflowExecutionEntity): Promise<WorkflowExecutionResult> {
    const executionId = workflowExecutionEntity.id;
    const workflowId = workflowExecutionEntity.getWorkflowId();

    logger.info("Starting workflow execution", { executionId, workflowId });

    // Verify the existence of the workflow graph.
    const workflowGraph = this.workflowGraphRegistry.get(workflowId);
    if (!workflowGraph) {
      throw new Error(`Workflow graph not found for workflow: ${workflowId}`);
    }

    // Create a WorkflowExecutionCoordinator using a factory and execute it.
    const workflowExecutionCoordinator = this.workflowExecutionCoordinatorFactory.create(workflowExecutionEntity);

    // Execute Workflow
    const result = await workflowExecutionCoordinator.execute();

    logger.info("Workflow execution completed", {
      executionId,
      workflowId,
      nodeCount: result.nodeResults?.length ?? 0,
    });

    return result;
  }
}
