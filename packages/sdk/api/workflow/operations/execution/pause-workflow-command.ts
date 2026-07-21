/**
 * PauseWorkflowCommand - Pause Workflow Execution Command
 *
 * Category: Management
 * Pauses a running workflow execution
 */

import {
  ManagementCommand,
  type CommandMetadataDefinition,
} from "../../../shared/types/command.js";
import { validateWorkflowLifecycleParams } from "../../../shared/operations/validators/workflow-validators.js";
import type { CommandValidationResult } from "../../../shared/types/command.js";
import type { APIDependencyManager } from "../../../shared/core/sdk-dependencies.js";

/**
 * Pause workflow command parameters
 */
export interface PauseWorkflowParams {
  /** Workflow execution ID (required) */
  executionId: string;
}

/**
 * Pause Workflow Execution Command
 */
export class PauseWorkflowCommand extends ManagementCommand<void> {
  constructor(
    private readonly params: PauseWorkflowParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  protected override getMetadataDefinition(): CommandMetadataDefinition {
    return {
      name: "PauseWorkflowCommand",
      description: "Pause a running workflow execution",
      category: "management",
      requiresAuth: false,
      version: "1.0.0",
      supportUndo: true,
      idempotent: false,
    };
  }

  protected async executeInternal(): Promise<void> {
    const lifecycleCoordinator = this.dependencies.getWorkflowLifecycleCoordinator();
    await lifecycleCoordinator.pauseWorkflowExecution(this.params.executionId);
  }

  validate(): CommandValidationResult {
    return validateWorkflowLifecycleParams(this.params.executionId);
  }
}
