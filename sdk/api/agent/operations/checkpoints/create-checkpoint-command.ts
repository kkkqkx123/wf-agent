/**
 * CreateCheckpointCommand - Create Agent Loop Checkpoint Command
 *
 * Responsibilities:
 * - Encapsulates checkpoint creation operation as Command pattern
 * - Provides unified API layer interface
 * - Supports parameter validation
 *
 * Design Principles:
 * - Follows Command pattern, inherits BaseCommand
 * - Uses dependency injection for AgentLoopRegistry and CheckpointResourceAPI
 * - Parameter validation is completed in validate() method
 */

import {
  BaseCommand,
  CommandValidationResult,
  validationSuccess,
  validationFailure,
} from "../../../shared/types/command.js";
import type { ID, CheckpointMetadata } from "@wf-agent/types";
import { AgentLoopCheckpointResourceAPI } from "../../resources/checkpoint-resource-api.js";
import type { APIDependencyManager } from "../../../shared/core/sdk-dependencies.js";

/**
 * Create Checkpoint Command Parameters
 */
export interface CreateCheckpointParams {
  /** Agent Loop ID */
  agentLoopId: ID;
  /** Checkpoint metadata */
  metadata?: CheckpointMetadata;
}

/**
 * Create Checkpoint Command
 *
 * Workflow:
 * 1. validate parameters (agentLoopId required)
 * 2. Get AgentLoopEntity
 * 3. Call CheckpointResourceAPI to create a checkpoint. 4.
 * 4. Return the checkpoint ID
 */
export class CreateCheckpointCommand extends BaseCommand<string> {
  private checkpointAPI: AgentLoopCheckpointResourceAPI;

  constructor(
    private readonly params: CreateCheckpointParams,
    private readonly dependencies: APIDependencyManager,
    checkpointAPI?: AgentLoopCheckpointResourceAPI,
  ) {
    super();
    this.checkpointAPI = checkpointAPI ?? new AgentLoopCheckpointResourceAPI();
  }

  protected async executeInternal(): Promise<string> {
    const registry = this.dependencies.getAgentLoopRegistry();

    // Getting the Agent Loop Entity
    const entity = registry.get(this.params.agentLoopId);
    if (!entity) {
      throw new Error(`Agent Loop not found: ${this.params.agentLoopId}`);
    }

    // Creating Checkpoints
    const checkpointId = await this.checkpointAPI.createCheckpoint(entity, {
      metadata: this.params.metadata,
    });

    return checkpointId;
  }

  validate(): CommandValidationResult {
    const errors: string[] = [];

    // Validation: agentLoopId must be provided
    if (!this.params.agentLoopId) {
      errors.push("Must provide agentLoopId");
    }

    return errors.length > 0 ? validationFailure(errors) : validationSuccess();
  }
}
