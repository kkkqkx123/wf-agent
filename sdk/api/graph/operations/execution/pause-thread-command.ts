/**
 * PauseThreadCommand - Pause Thread Command
 */

import {
  BaseCommand,
  CommandValidationResult,
  validationSuccess,
  validationFailure,
} from "../../../shared/types/command.js";
import { APIDependencyManager } from "../../../shared/core/sdk-dependencies.js";

/**
 * Pause Thread Command
 */
export class PauseThreadCommand extends BaseCommand<void> {
  constructor(
    private readonly threadId: string,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  protected async executeInternal(): Promise<void> {
    const lifecycleCoordinator = this.dependencies.getThreadLifecycleCoordinator();
    await lifecycleCoordinator.pauseThread(this.threadId);
  }

  validate(): CommandValidationResult {
    const errors: string[] = [];

    if (!this.threadId || this.threadId.trim().length === 0) {
      errors.push("Thread ID cannot be empty.");
    }

    return errors.length > 0 ? validationFailure(errors) : validationSuccess();
  }
}
