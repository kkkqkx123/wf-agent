/**
 * ExecuteWorkflowCommand - Execute Workflow Command
 *
 * Responsibilities:
 * - Receives workflow ID and execution options as input
 * - Delegates to WorkflowLifecycleCoordinator to execute workflow
 * - Returns WorkflowExecutionResult as execution result
 *
 * Design Principles:
 * - Follows Command pattern, inherits BaseCommand
 * - Uses dependency injection for ExecutionContext and WorkflowLifecycleCoordinator
 * - Parameter validation is completed in validate() method
 * - Actual execution logic is implemented in executeInternal()
 *
 * Note:
 * - This command is only responsible for executing workflows, not for registering workflow definitions
 * - Workflow registration should be done through a separate API
 * - WorkflowExecution is an execution instance of workflow template
 */

import {
  BaseCommand,
  CommandValidationResult,
  validationSuccess,
  validationFailure,
} from "../../../shared/types/command.js";
import type { WorkflowExecutionResult, ThreadOptions } from "@wf-agent/types";
import { APIDependencyManager } from "../../../shared/core/sdk-dependencies.js";

/**
 * Execute workflow command parameters
 */
export interface ExecuteWorkflowParams {
  /** Workflow ID (required) */
  workflowId: string;
  /** Execution options */
  options?: ThreadOptions;
}

/**
 * Execute Workflow Command
 *
 * Workflow:
 * 1. Validate parameters (workflowId is required)
 * 2. Execute workflow using WorkflowLifecycleCoordinator
 * 3. Return WorkflowExecutionResult
 */
export class ExecuteWorkflowCommand extends BaseCommand<WorkflowExecutionResult> {
  constructor(
    private readonly params: ExecuteWorkflowParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
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
    const errors: string[] = [];

    // Verification: The workflowId must be provided.
    if (!this.params.workflowId || this.params.workflowId.trim().length === 0) {
      errors.push("The workflowId must be provided.");
    }

    return errors.length > 0 ? validationFailure(errors) : validationSuccess();
  }
}
