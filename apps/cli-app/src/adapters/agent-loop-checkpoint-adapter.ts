/**
 * Agent Loop Checkpoint Adapter
 * Encapsulates SDK API calls related to Agent Loop Checkpoints
 */

import { BaseAdapter } from "./base-adapter.js";
import {
  createCheckpoint,
  restoreFromCheckpoint,
  type CheckpointDependencies,
  type CreateCheckpointOptions,
} from "@wf-agent/sdk";
import { AgentLoopCheckpointResourceAPI } from "@wf-agent/sdk";
import type { AgentLoopEntity } from "@wf-agent/sdk";
import type { AgentLoopCheckpoint } from "@wf-agent/types";
import { CLINotFoundError } from "../types/cli-types.js";

/**
 * Agent Loop Checkpoint Adapter
 */
export class AgentLoopCheckpointAdapter extends BaseAdapter {
  private checkpointAPI: AgentLoopCheckpointResourceAPI;

  constructor() {
    super();
    this.checkpointAPI = new AgentLoopCheckpointResourceAPI();
  }

  /**
   * Create Agent Loop checkpoint
   * @param entity Agent Loop entity
   * @param dependencies Checkpoint dependencies
   * @param options Creation options
   */
  async createCheckpoint(
    entity: AgentLoopEntity,
    dependencies: CheckpointDependencies,
    options?: CreateCheckpointOptions,
  ): Promise<string> {
    return this.executeWithErrorHandling(async () => {
      const checkpointId = await createCheckpoint(entity, dependencies, options);
      this.output.infoLog(`Checkpoint created: ${checkpointId}`);
      return checkpointId;
    }, "Creating an Agent Loop Checkpoint");
  }

  /**
   * Restore Agent Loop from checkpoint
   * @param checkpointId Checkpoint ID
   * @param dependencies Checkpoint dependencies
   */
  async restoreCheckpoint(
    checkpointId: string,
    dependencies: CheckpointDependencies,
  ): Promise<AgentLoopEntity> {
    return this.executeWithErrorHandling(async () => {
      const entity = await restoreFromCheckpoint(checkpointId, dependencies);
      this.output.infoLog(`Agent Loop restored from checkpoint: ${checkpointId}`);
      return entity;
    }, "Restoring Agent Loop from Checkpoints");
  }

  /**
   * Get all checkpoints for an Agent Loop
   * @param agentLoopId Agent Loop ID
   */
  async listCheckpoints(agentLoopId: string): Promise<any[]> {
    return this.executeWithErrorHandling(async () => {
      const checkpoints = await this.checkpointAPI.getAgentLoopCheckpoints(agentLoopId);

      // Conversion to summary format
      const summaries = checkpoints.map((cp: AgentLoopCheckpoint) => ({
        id: cp.id,
        agentLoopId: cp.agentLoopId,
        timestamp: cp.timestamp,
        type: cp.type,
        metadata: cp.metadata,
      }));

      return summaries;
    }, "List the Agent Loop checkpoints");
  }

  /**
   * Get checkpoint details
   * @param checkpointId Checkpoint ID
   */
  async getCheckpoint(checkpointId: string): Promise<any> {
    return this.executeWithErrorHandling(async () => {
      const checkpoint = await this.checkpointAPI.get(checkpointId);
      if (!checkpoint) {
        throw new CLINotFoundError(
          `Checkpoint not found: ${checkpointId}`,
          "AgentLoopCheckpoint",
          checkpointId,
        );
      }

      return checkpoint;
    }, "Get Agent Loop checkpoint details");
  }

  /**
   * Delete checkpoint
   * @param checkpointId Checkpoint ID
   */
  async deleteCheckpoint(checkpointId: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      await this.checkpointAPI.delete(checkpointId);
      this.output.infoLog(`Checkpoint deleted: ${checkpointId}`);
    }, "Delete the Agent Loop checkpoint");
  }

  /**
   * Get the latest checkpoint
   * @param agentLoopId Agent Loop ID
   */
  async getLatestCheckpoint(agentLoopId: string): Promise<any> {
    return this.executeWithErrorHandling(async () => {
      const checkpoint = await this.checkpointAPI.getLatestCheckpoint(agentLoopId);
      if (!checkpoint) {
        throw new CLINotFoundError(
          `No checkpoint found for Agent Loop: ${agentLoopId}`,
          "AgentLoopCheckpoint",
          agentLoopId,
        );
      }

      return checkpoint;
    }, "Get the latest Agent Loop checkpoint");
  }

  /**
   * Get checkpoint chain
   * @param checkpointId Checkpoint ID
   */
  async getCheckpointChain(checkpointId: string): Promise<any[]> {
    return this.executeWithErrorHandling(async () => {
      const chain = await this.checkpointAPI.getCheckpointChain(checkpointId);
      return chain;
    }, "Get the Agent Loop checkpoint chain");
  }

  /**
   * Get checkpoint statistics
   */
  async getStatistics(): Promise<any> {
    return this.executeWithErrorHandling(async () => {
      const stats = await this.checkpointAPI.getCheckpointStatistics();
      return stats;
    }, "Get Agent Loop checkpoint statistics");
  }

  /**
   * Delete all checkpoints for an Agent Loop
   * @param agentLoopId Agent Loop ID
   */
  async deleteAllCheckpoints(agentLoopId: string): Promise<number> {
    return this.executeWithErrorHandling(async () => {
      const count = await this.checkpointAPI.deleteAgentLoopCheckpoints(agentLoopId);
      this.output.infoLog(`Deleted ${count} checkpoint(s) for Agent Loop: ${agentLoopId}`);
      return count;
    }, "Delete all Agent Loop checkpoints");
  }
}
