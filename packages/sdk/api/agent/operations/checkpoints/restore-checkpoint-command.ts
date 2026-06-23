/**
 * RestoreCheckpointCommand - Restore Agent Loop From Checkpoint Command
 *
 * Category: Management
 * Restores an agent loop execution state from a previously created checkpoint
 */

import {
  ManagementCommand,
  type CommandMetadataDefinition,
} from "../../../shared/types/command.js";
import { validateAgentCheckpointRestorationParams } from "../../../shared/operations/validators/agent-validators.js";
import type { CommandValidationResult } from "../../../shared/types/command.js";
import type { AgentLoopEntity } from "../../../../agent/entities/agent-loop-entity.js";
import type { APIDependencyManager } from "../../../shared/core/sdk-dependencies.js";

/**
 * Restore checkpoint command parameters
 */
export interface RestoreCheckpointParams {
  /** Checkpoint ID to restore from */
  checkpointId: string;
}

/**
 * Restore Checkpoint Command
 * Restores agent loop execution state from a checkpoint
 */
export class RestoreCheckpointCommand extends ManagementCommand<AgentLoopEntity> {
  constructor(
    private readonly params: RestoreCheckpointParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  protected override getMetadataDefinition(): CommandMetadataDefinition {
    return {
      name: "RestoreCheckpointCommand",
      description: "Restore agent loop execution state from a checkpoint",
      category: "management",
      requiresAuth: false,
      version: "1.0.0",
      supportUndo: false,
      idempotent: false,
    };
  }

  protected async executeInternal(): Promise<AgentLoopEntity> {
    const checkpointAPI = this.dependencies.getAgentLoopCheckpointResourceAPI();
    // Restore from a checkpoint
    const entity = await checkpointAPI.restoreFromCheckpoint(this.params.checkpointId);
    return entity;
  }

  validate(): CommandValidationResult {
    return validateAgentCheckpointRestorationParams(this.params.checkpointId);
  }
}
