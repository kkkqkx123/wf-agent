/**
 * ExecuteWorkflowCommand - Execute Workflow Command
 *
 * Category: Execution
 * Long-running workflow execution with full lifecycle management
 */

import {
  ExecutionCommand,
  type CommandMetadataDefinition,
} from "../../../shared/types/command.js";
import { validateWorkflowExecutionParams } from "../../../shared/operations/validators/workflow-validators.js";
import type { CommandValidationResult } from "../../../shared/types/command.js";
import type { WorkflowExecutionResult, WorkflowExecutionOptions } from "@wf-agent/types";
import type { APIDependencyManager } from "../../../shared/core/sdk-dependencies.js";

/**
 * Execute workflow command parameters
 */
export interface ExecuteWorkflowParams {
  /** Workflow ID (required) */
  workflowId: string;
  /** Execution options */
  options?: WorkflowExecutionOptions;
}

/**
 * Execute Workflow Command
 * Executes a workflow and returns the execution result
 */
export class ExecuteWorkflowCommand extends ExecutionCommand<WorkflowExecutionResult> {
  constructor(
    private readonly params: ExecuteWorkflowParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  protected override getMetadataDefinition(): CommandMetadataDefinition {
    return {
      name: "ExecuteWorkflowCommand",
      description: "Execute a workflow by ID with optional execution parameters",
      category: "execution",
      requiresAuth: false,
      version: "1.0.0",
      supportCancellation: true,
      idempotent: false,
    };
  }

  protected async executeInternal(): Promise<WorkflowExecutionResult> {
    // Obtain the WorkflowLifecycleCoordinator through APIDependencyManager
    const lifecycleCoordinator = this.dependencies.getWorkflowLifecycleCoordinator();

    // Execute workflow (delegated to WorkflowLifecycleCoordinator)
    const result = await lifecycleCoordinator.execute(
      this.params.workflowId,
      this.params.options || {},
    );

    return result;
  }

  validate(): CommandValidationResult {
    return validateWorkflowExecutionParams(this.params.workflowId);
  }
}
