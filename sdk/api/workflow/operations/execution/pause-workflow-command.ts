/**
 * PauseWorkflowCommand - Pause Workflow Execution Command
 */

import {
  BaseCommand,
  CommandValidationResult,
  validationSuccess,
  validationFailure,
} from "../../../shared/types/command.js";
import { APIDependencyManager } from "../../../shared/core/sdk-dependencies.js";

/**
 * Pause Workflow Execution Command
 */
export class PauseWorkflowCommand extends BaseCommand<void> {
  constructor(
    private readonly workflowExecutionId: string,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  protected async executeInternal(): Promise<void> {
    const lifecycleCoordinator = this.dependencies.getWorkflowLifecycleCoordinator();
    await lifecycleCoordinator.pauseWorkflowExecution(this.workflowExecutionId);
  }

  validate(): CommandValidationResult {
    const errors: string[] = [];

    if (!this.workflowExecutionId || this.workflowExecutionId.trim().length === 0) {
      errors.push("Execution ID cannot be empty.");
    }

    return errors.length > 0 ? validationFailure(errors) : validationSuccess();
  }
}
