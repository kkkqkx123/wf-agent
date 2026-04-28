/**
 * EnableTriggerCommand - Enable Trigger
 */

import { BaseCommand, CommandValidationResult } from "../../../shared/types/command.js";
import { ThreadContextNotFoundError } from "@wf-agent/types";
import type { APIDependencyManager } from "../../../shared/core/sdk-dependencies.js";

/**
 * Enabling Trigger Parameters
 */
export interface EnableTriggerParams {
  /** Thread ID */
  threadId: string;
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

    if (!this.params.threadId || this.params.threadId.trim() === "") {
      errors.push("Thread ID cannot be empty.");
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
    const triggerManager = (await this.getTriggerManager(this.params.threadId)) as {
      enable: (triggerId: string) => void;
    };
    triggerManager.enable(this.params.triggerId);
  }

  /**
   * Get Trigger Manager
   */
  private async getTriggerManager(threadId: string) {
    const threadContext = this.dependencies.getThreadRegistry().get(threadId);
    if (!threadContext) {
      throw new ThreadContextNotFoundError(`Thread not found: ${threadId}`, threadId);
    }
    return threadContext.triggerManager;
  }
}
