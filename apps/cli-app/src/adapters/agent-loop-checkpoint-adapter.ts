/**
 * Agent Loop Checkpoint Adapter
 * Encapsulates SDK API calls related to Agent Loop Checkpoints
 */

import { BaseAdapter } from "./base-adapter.js";
import {
  createAgentLoopCheckpoint,
  AgentLoopCheckpointCoordinator,
  type CheckpointDependencies,
  type CheckpointOptions,
  type AgentLoopEntity,
} from "@wf-agent/sdk/agent";
import { AgentLoopCheckpointResourceAPI, type AgentLoopCheckpointStatistics } from "@wf-agent/sdk/api";
import type { AgentLoopCheckpoint, CheckpointMetadata } from "@wf-agent/types";
import { CLINotFoundError } from "../types/cli-types.js";

/**
 * Agent Loop Checkpoint Adapter
 */
export class AgentLoopCheckpointAdapter extends BaseAdapter {
  private checkpointAPI: AgentLoopCheckpointResourceAPI;

  constructor() {
    super();
    // Use the SDK's DI container to get the API instance, ensuring it
    // shares the same storage adapter as the rest of the system.
    this.checkpointAPI = this.sdk.getFactory().getDependencies().getAgentLoopCheckpointResourceAPI();
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
    options?: CheckpointOptions,
  ): Promise<string> {
    return this.executeWithErrorHandling(async () => {
      const checkpointId = await createAgentLoopCheckpoint(entity, dependencies as Parameters<typeof createAgentLoopCheckpoint>[1], options);
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
    config?: import("@wf-agent/types").AgentLoopRuntimeConfig,
  ): Promise<AgentLoopEntity> {
    return this.executeWithErrorHandling(async () => {
      const coordinator = new AgentLoopCheckpointCoordinator(config);
      const entity = await coordinator.restoreFromCheckpoint(checkpointId, dependencies);
      this.output.infoLog(`Agent Loop restored from checkpoint: ${checkpointId}`);
      return entity;
    }, "Restoring Agent Loop from Checkpoints");
  }

  /**
   * Get all checkpoints for an Agent Loop
   * @param agentLoopId Agent Loop ID
   */
  async listCheckpoints(agentLoopId: string): Promise<Array<{
    id: string;
    agentLoopId: string;
    timestamp: number;
    type: string;
    metadata?: CheckpointMetadata;
  }>> {
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
  async getCheckpoint(checkpointId: string): Promise<AgentLoopCheckpoint> {
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
  async getLatestCheckpoint(agentLoopId: string): Promise<AgentLoopCheckpoint> {
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
  async getCheckpointChain(checkpointId: string): Promise<AgentLoopCheckpoint[]> {
    return this.executeWithErrorHandling(async () => {
      const chain = await this.checkpointAPI.getCheckpointChainFrom(checkpointId);
      return chain;
    }, "Get the Agent Loop checkpoint chain");
  }

  /**
   * Get checkpoint statistics
   */
  async getStatistics(): Promise<AgentLoopCheckpointStatistics> {
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

  /**
   * Save checkpoint to storage (for dependency injection)
   * @param checkpoint Checkpoint object
   */
  async saveCheckpointToStorage(checkpoint: AgentLoopCheckpoint): Promise<string> {
    return this.executeWithErrorHandling(async () => {
      // Use the checkpointAPI's internal storage
      await this.checkpointAPI.create(checkpoint);

      return checkpoint.id;
    }, "Save checkpoint to storage");
  }

  /**
   * Get checkpoint from storage (for dependency injection)
   * @param checkpointId Checkpoint ID
   */
  async getCheckpointFromStorage(checkpointId: string): Promise<AgentLoopCheckpoint | null> {
    return this.executeWithErrorHandling(async () => {
      return await this.checkpointAPI.get(checkpointId) || null;
    }, "Get checkpoint from storage");
  }

  /**
   * List checkpoint IDs from storage (for dependency injection)
   * @param agentLoopId Agent Loop ID
   */
  async listCheckpointIdsFromStorage(agentLoopId: string): Promise<string[]> {
    return this.executeWithErrorHandling(async () => {
      // Get all checkpoints and filter by agentLoopId
      const allCheckpoints = await this.checkpointAPI.getAll();

      return allCheckpoints
        .filter((cp: AgentLoopCheckpoint) => cp.agentLoopId === agentLoopId)
        .map((cp: AgentLoopCheckpoint) => cp.id);
    }, "List checkpoint IDs from storage");
  }

  /**
   * Query checkpoints by filter
   * @param agentLoopId Agent Loop ID
   * @param filters Filter criteria
   */
  async queryCheckpoints(
    agentLoopId: string,
    filters?: {
      startTime?: number;
      endTime?: number;
      type?: "FULL" | "DELTA";
    },
  ): Promise<
    Array<{
      id: string;
      timestamp: number;
      type: string;
      metadata?: CheckpointMetadata;
    }>
  > {
    return this.executeWithErrorHandling(async () => {
      const filterObj: any = { agentLoopId };

      if (filters?.startTime !== undefined || filters?.endTime !== undefined) {
        filterObj.timestampRange = {
          start: filters?.startTime,
          end: filters?.endTime,
        };
      }

      if (filters?.type) {
        filterObj.type = filters.type;
      }

      const checkpoints = await this.checkpointAPI.query(filterObj);
      return checkpoints.map((cp) => ({
        id: cp.id,
        timestamp: cp.timestamp,
        type: cp.type,
        metadata: cp.metadata,
      }));
    }, "Query checkpoints with filters");
  }

  /**
   * Get checkpoints within time range
   * @param agentLoopId Agent Loop ID
   * @param startTime Start timestamp (ms)
   * @param endTime End timestamp (ms)
   */
  async getCheckpointsByTimeRange(
    agentLoopId: string,
    startTime: number,
    endTime: number,
  ): Promise<AgentLoopCheckpoint[]> {
    return this.executeWithErrorHandling(
      () => this.checkpointAPI.getByTimeRange(agentLoopId, startTime, endTime),
      "Get checkpoints by time range",
    );
  }

  /**
   * Get checkpoints by type
   * @param agentLoopId Agent Loop ID
   * @param type Type (FULL | DELTA)
   */
  async getCheckpointsByType(
    agentLoopId: string,
    type: "FULL" | "DELTA",
  ): Promise<AgentLoopCheckpoint[]> {
    return this.executeWithErrorHandling(
      () => this.checkpointAPI.getByType(agentLoopId, type),
      "Get checkpoints by type",
    );
  }

  /**
   * Get checkpoints by IDs
   * @param ids Checkpoint ID list
   */
  async getCheckpointsById(ids: string[]): Promise<AgentLoopCheckpoint[]> {
    return this.executeWithErrorHandling(
      () => this.checkpointAPI.getByIds(ids),
      "Get checkpoints by IDs",
    );
  }
}
