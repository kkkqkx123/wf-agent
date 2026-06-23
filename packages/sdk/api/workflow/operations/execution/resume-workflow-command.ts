/**
 * ResumeWorkflowCommand - Resume Workflow Execution Command
 *
 * Category: Management
 * Resumes a paused workflow execution
 */

import {
  ManagementCommand,
  CommandValidationResult,
  validationSuccess,
  validationFailure,
  type CommandMetadataDefinition,
} from "../../../shared/types/command.js";
import { validateRequiredId } from "../../../shared/operations/validation-utils.js";
import type { WorkflowExecutionResult } from "@wf-agent/types";
import type { APIDependencyManager } from "../../../shared/core/sdk-dependencies.js";

/**
 * Resume workflow command parameters
 */
export interface ResumeWorkflowParams {
  /** Workflow execution ID (required) */
  executionId: string;
}

/**
 * Resume Workflow Execution Command
 */
export class ResumeWorkflowCommand extends ManagementCommand<WorkflowExecutionResult> {
  constructor(
    private readonly params: ResumeWorkflowParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  protected override getMetadataDefinition(): CommandMetadataDefinition {
    return {
      name: "ResumeWorkflowCommand",
      description: "Resume a paused workflow execution",
      category: "management",
      requiresAuth: false,
      version: "1.0.0",
      supportUndo: true,
      idempotent: false,
    };
  }

  protected async executeInternal(): Promise<WorkflowExecutionResult> {
    const lifecycleCoordinator = this.dependencies.getWorkflowLifecycleCoordinator();
    const result = await lifecycleCoordinator.resumeWorkflowExecution(this.params.executionId);
    return result;
  }

  validate(): CommandValidationResult {
    const errors = validateRequiredId(this.params.executionId, "Execution ID");
    return errors.length > 0 ? validationFailure(errors) : validationSuccess();
  }
}
