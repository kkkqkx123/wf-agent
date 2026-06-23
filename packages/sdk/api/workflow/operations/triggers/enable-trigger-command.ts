/**
 * EnableTriggerCommand - Enable Trigger Command
 *
 * Category: Management
 * Enables a trigger for a workflow execution
 */

import {
  ManagementCommand,
  type CommandMetadataDefinition,
} from "../../../shared/types/command.js";
import { validateTriggerParams } from "../../../shared/operations/validators/workflow-validators.js";
import type { CommandValidationResult } from "../../../shared/types/command.js";
import { WorkflowExecutionNotFoundError } from "@wf-agent/types";
import type { APIDependencyManager } from "../../../shared/core/sdk-dependencies.js";

/**
 * Enable trigger command parameters
 */
export interface EnableTriggerParams {
  /** Workflow execution ID */
  executionId: string;
  /** Trigger ID to enable */
  triggerId: string;
}

/**
 * EnableTriggerCommand - Enable Trigger
 */
export class EnableTriggerCommand extends ManagementCommand<void> {
  constructor(
    private readonly params: EnableTriggerParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  protected override getMetadataDefinition(): CommandMetadataDefinition {
    return {
      name: "EnableTriggerCommand",
      description: "Enable a trigger for a workflow execution",
      category: "management",
      requiresAuth: false,
      version: "1.0.0",
      supportUndo: true,
      idempotent: false,
    };
  }

  /**
   * Verify command parameters using shared validator
   */
  validate(): CommandValidationResult {
    return validateTriggerParams(this.params.executionId, this.params.triggerId);
  }

  /**
   * Execute command
   */
  protected async executeInternal(): Promise<void> {
    const triggerManager = (await this.getTriggerManager(this.params.executionId)) as {
      enable: (triggerId: string) => void;
    };
    triggerManager.enable(this.params.triggerId);
  }

  /**
   * Get Trigger Manager
   */
  private async getTriggerManager(executionId: string) {
    const executionContext = this.dependencies.getWorkflowExecutionRegistry().get(executionId);
    if (!executionContext) {
      throw new WorkflowExecutionNotFoundError(
        `Workflow execution not found: ${executionId}`,
        executionId,
      );
    }
    return executionContext.triggerManager;
  }
}
