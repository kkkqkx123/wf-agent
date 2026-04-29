/**
 * DisableTriggerCommand - Disable the trigger
 */

import { BaseCommand, CommandValidationResult } from "../../../shared/types/command.js";
import { WorkflowExecutionNotFoundError } from "@wf-agent/types";
import type { APIDependencyManager } from "../../../shared/core/sdk-dependencies.js";

/**
 * Disable trigger parameters
 */
export interface DisableTriggerParams {
  /** Execution ID */
  executionId: string;
  /** Trigger ID */
  triggerId: string;
}

/**
 * DisableTriggerCommand - Disables the trigger.
 */
export class DisableTriggerCommand extends BaseCommand<void> {
  constructor(
    private readonly params: DisableTriggerParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  /**
   * Verify command parameters
   */
  validate(): CommandValidationResult {
    const errors: string[] = [];

    if (!this.params.executionId || this.params.executionId.trim() === "") {
      errors.push("Execution ID cannot be null");
    }

    if (!this.params.triggerId || this.params.triggerId.trim() === "") {
      errors.push("Trigger ID cannot be null");
    }

    return {
      valid: errors.length === 0,
      errors,
    };
  }

  /**
   * Execute the command
   */
  protected async executeInternal(): Promise<void> {
    const triggerManager = (await this.getTriggerManager(this.params.executionId)) as {
      disable: (triggerId: string) => void;
    };
    triggerManager.disable(this.params.triggerId);
  }

  /**
   * Obtain the Trigger Manager
   */
  private async getTriggerManager(executionId: string) {
    const executionContext = this.dependencies.getWorkflowExecutionRegistry().get(executionId);
    if (!executionContext) {
      throw new WorkflowExecutionNotFoundError(`Workflow execution not found: ${executionId}`, executionId);
    }
    return executionContext.triggerManager;
  }
}
