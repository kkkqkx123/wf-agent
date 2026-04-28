/**
 * ResumeWorkflowCommand - Command to resume a workflow execution
 */

import {
  BaseCommand,
  CommandValidationResult,
  validationSuccess,
  validationFailure,
} from "../../../shared/types/command.js";
import type { WorkflowExecutionResult } from "@wf-agent/types";
import { APIDependencyManager } from "../../../shared/core/sdk-dependencies.js";

/**
 * Workflow execution resume command
 */
export class ResumeWorkflowCommand extends BaseCommand<WorkflowExecutionResult> {
  constructor(
    private readonly workflowExecutionId: string,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  protected async executeInternal(): Promise<WorkflowExecutionResult> {
    const lifecycleCoordinator = this.dependencies.getWorkflowLifecycleCoordinator();
    const result = await lifecycleCoordinator.resumeWorkflowExecution(this.workflowExecutionId);
    return result;
  }

  validate(): CommandValidationResult {
    const errors: string[] = [];

    if (!this.workflowExecutionId || this.workflowExecutionId.trim().length === 0) {
      errors.push("Execution ID cannot be empty");
    }

    return errors.length > 0 ? validationFailure(errors) : validationSuccess();
  }
}
