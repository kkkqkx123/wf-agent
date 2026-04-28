/**
 * RestoreCheckpointCommand - Restore Agent Loop From Checkpoint Command
 *
 * Responsibilities:
 * - Encapsulates checkpoint restoration operation as Command pattern
 * - Provides unified API layer interface
 * - Supports parameter validation
 *
 * Design Principles:
 * - Follows Command pattern, inherits BaseCommand
 * - Uses dependency injection for CheckpointResourceAPI
 * - Parameter validation is completed in validate() method
 */

import {
  BaseCommand,
  CommandValidationResult,
  validationSuccess,
  validationFailure,
} from "../../../shared/types/command.js";
import type { AgentLoopEntity } from "../../../../agent/entities/agent-loop-entity.js";
import { AgentLoopCheckpointResourceAPI } from "../../resources/checkpoint-resource-api.js";

/**
 * Command parameters for restoring from a checkpoint
 */
export interface RestoreCheckpointParams {
  /** Checkpoint ID */
  checkpointId: string;
}

/**
 * Command to restore from a checkpoint
 *
 * Workflow:
 * 1. Verify the parameters (checkpointId is required)
 * 2. Call the CheckpointResourceAPI to restore the checkpoint
 * 3. Return the restored AgentLoopEntity
 */
export class RestoreCheckpointCommand extends BaseCommand<AgentLoopEntity> {
  private checkpointAPI: AgentLoopCheckpointResourceAPI;

  constructor(
    private readonly params: RestoreCheckpointParams,
    checkpointAPI?: AgentLoopCheckpointResourceAPI,
  ) {
    super();
    this.checkpointAPI = checkpointAPI ?? new AgentLoopCheckpointResourceAPI();
  }

  protected async executeInternal(): Promise<AgentLoopEntity> {
    // Restore from a checkpoint
    const entity = await this.checkpointAPI.restoreFromCheckpoint(this.params.checkpointId);
    return entity;
  }

  validate(): CommandValidationResult {
    const errors: string[] = [];

    // Verification: The checkpointId must be provided.
    if (!this.params.checkpointId || this.params.checkpointId.trim() === "") {
      errors.push("The checkpoint ID cannot be empty.");
    }

    return errors.length > 0 ? validationFailure(errors) : validationSuccess();
  }
}
