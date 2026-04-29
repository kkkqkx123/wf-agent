/**
 * EnableTriggerCommand - Enable Trigger
 */

import { BaseCommand, CommandValidationResult } from "../../../shared/types/command.js";
import { WorkflowExecutionNotFoundError } from "@wf-agent/types";
import type { APIDependencyManager } from "../../../shared/core/sdk-dependencies.js";

/**
 * Enabling Trigger Parameters
 */
export interface EnableTriggerParams {
  /** Execution ID */
  executionId: string;
  /** Trigger ID */
  triggerId: string;
}

/**
 * EnableTriggerCommand - Enable Trigger
 */
export class EnableTriggerCommand extends BaseCommand<void> {
  constructor(
    private readonly params: EnableTriggerParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  /**
   * Verifying Command Parameters
   */
  validate(): CommandValidationResult {
    const errors: string[] = [];

    if (!this.params.executionId || this.params.executionId.trim() === "") {
      errors.push("Execution ID cannot be empty.");
    }

    if (!this.params.triggerId || this.params.triggerId.trim() === "") {
      errors.push("Trigger ID cannot be empty.");
    }

    return {
      valid: errors.length === 0,
      errors,
    };
  }

  /**
   * execute a command
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
      throw new WorkflowExecutionNotFoundError(`Workflow execution not found: ${executionId}`, executionId);
    }
    return executionContext.triggerManager;
  }
}
