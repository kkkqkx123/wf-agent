/**
 * CreateCheckpointCommand - Create Agent Loop Checkpoint Command
 *
 * Category: Management
 * Creates a checkpoint of the current agent loop execution state
 */

import {
  ManagementCommand,
  type CommandMetadataDefinition,
} from "../../../shared/types/command.js";
import { validateAgentCheckpointCreationParams } from "../../../shared/operations/validators/agent-validators.js";
import type { CommandValidationResult } from "../../../shared/types/command.js";
import type { ID, CheckpointMetadata } from "@wf-agent/types";
import { AgentLoopNotFoundError } from "@wf-agent/types";
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
 */
export class CreateCheckpointCommand extends ManagementCommand<string> {
  constructor(
    private readonly params: CreateCheckpointParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  protected override getMetadataDefinition(): CommandMetadataDefinition {
    return {
      name: "CreateCheckpointCommand",
      description: "Create a checkpoint of agent loop execution state",
      category: "management",
      requiresAuth: false,
      version: "1.0.0",
      supportUndo: false,
      idempotent: false,
    };
  }

  protected async executeInternal(): Promise<string> {
    const checkpointAPI = this.dependencies.getAgentLoopCheckpointResourceAPI();
    const registry = this.dependencies.getAgentLoopRegistry();

    // Getting the Agent Loop Entity
    const entity = await registry.get(this.params.agentLoopId);
    if (!entity) {
      throw new AgentLoopNotFoundError(
        `Agent Loop not found: ${this.params.agentLoopId}`,
        this.params.agentLoopId,
      );
    }

    // Creating Checkpoints
    const checkpointId = await checkpointAPI.createCheckpoint(entity, this.params.metadata);

    return checkpointId;
  }

  validate(): CommandValidationResult {
    return validateAgentCheckpointCreationParams(this.params.agentLoopId);
  }
}
