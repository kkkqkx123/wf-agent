/**
 * RestoreFromCheckpointCommand - Restore a thread from a checkpoint
 */

import {
  BaseCommand,
  CommandValidationResult,
  validationSuccess,
  validationFailure,
} from "../../../shared/types/command.js";
import { CheckpointCoordinator } from "../../../../graph/checkpoint/checkpoint-coordinator.js";
import type { Thread } from "@wf-agent/types";

import type { APIDependencyManager } from "../../../shared/core/sdk-dependencies.js";

/**
 * Restore parameters from the checkpoint.
 */
export interface RestoreFromCheckpointParams {
  /** Checkpoint ID */
  checkpointId: string;
}

/**
 * RestoreFromCheckpointCommand - Restore a thread from a checkpoint
 */
export class RestoreFromCheckpointCommand extends BaseCommand<Thread> {
  constructor(
    private readonly params: RestoreFromCheckpointParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  /**
   * Verify command parameters
   */
  validate(): CommandValidationResult {
    const errors: string[] = [];

    if (!this.params.checkpointId || this.params.checkpointId.trim() === "") {
      errors.push("Checkpoint ID cannot be null");
    }

    return errors.length > 0 ? validationFailure(errors) : validationSuccess();
  }

  /**
   * Execute the command
   */
  protected async executeInternal(): Promise<Thread> {
    const dependencies = {
      threadRegistry: this.dependencies.getThreadRegistry(),
      checkpointStateManager: this.dependencies.getCheckpointStateManager(),
      workflowRegistry: this.dependencies.getWorkflowRegistry(),
      graphRegistry: this.dependencies.getGraphRegistry(),
    };

    const { threadEntity } = await CheckpointCoordinator.restoreFromCheckpoint(
      this.params.checkpointId,
      dependencies,
    );
    return threadEntity.getThread();
  }
}
