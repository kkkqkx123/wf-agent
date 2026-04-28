/**
 * ResumeThreadCommand - Command to resume a thread
 */

import {
  BaseCommand,
  CommandValidationResult,
  validationSuccess,
  validationFailure,
} from "../../../shared/types/command.js";
import type { ThreadResult } from "@wf-agent/types";
import { APIDependencyManager } from "../../../shared/core/sdk-dependencies.js";

/**
 * Thread recovery command
 */
export class ResumeThreadCommand extends BaseCommand<ThreadResult> {
  constructor(
    private readonly threadId: string,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  protected async executeInternal(): Promise<ThreadResult> {
    const lifecycleCoordinator = this.dependencies.getThreadLifecycleCoordinator();
    const result = await lifecycleCoordinator.resumeThread(this.threadId);
    return result;
  }

  validate(): CommandValidationResult {
    const errors: string[] = [];

    if (!this.threadId || this.threadId.trim().length === 0) {
      errors.push("Thread ID cannot be null");
    }

    return errors.length > 0 ? validationFailure(errors) : validationSuccess();
  }
}
