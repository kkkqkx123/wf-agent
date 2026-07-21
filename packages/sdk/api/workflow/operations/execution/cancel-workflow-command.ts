/**
 * CancelWorkflowCommand - Cancel Workflow Execution Command
 *
 * Category: Management
 * Cancels a running or paused workflow execution
 */

import {
  ManagementCommand,
  type CommandMetadataDefinition,
} from "../../../shared/types/command.js";
import { validateWorkflowLifecycleParams } from "../../../shared/operations/validators/workflow-validators.js";
import type { CommandValidationResult } from "../../../shared/types/command.js";
import type { APIDependencyManager } from "../../../shared/core/sdk-dependencies.js";

/**
 * Cancel workflow command parameters
 */
export interface CancelWorkflowParams {
  /** Workflow execution ID (required) */
  executionId: string;
  /** Optional reason for cancellation */
  reason?: string;
}

/**
 * Cancel workflow execution command
 */
export class CancelWorkflowCommand extends ManagementCommand<void> {
  constructor(
    private readonly params: CancelWorkflowParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  protected override getMetadataDefinition(): CommandMetadataDefinition {
    return {
      name: "CancelWorkflowCommand",
      description: "Cancel a running or paused workflow execution",
      category: "management",
      requiresAuth: false,
      version: "1.0.0",
      idempotent: false,
    };
  }

  protected async executeInternal(): Promise<void> {
    const lifecycleCoordinator = this.dependencies.getWorkflowLifecycleCoordinator();
    await lifecycleCoordinator.stopWorkflowExecution(
      this.params.executionId,
      this.params.reason,
    );
  }

  validate(): CommandValidationResult {
    return validateWorkflowLifecycleParams(this.params.executionId);
  }
}
