/**
 * AgentFileCheckpointResourceAPI - Agent File Checkpoint Resource Management API
 *
 * Provides APIs for managing agent loop file state checkpoints.
 * Extends the shared BaseFileCheckpointResourceAPI with agent-specific type annotations.
 * This API is only available when file checkpointing is enabled in SDKOptions.
 */

import { BaseFileCheckpointResourceAPI } from "../../shared/resources/file-checkpoint-base.js";
import type { APIDependencyManager } from "../../shared/core/sdk-dependencies.js";
import type { FileCheckpointMetadata, FileCheckpointInfo } from "@wf-agent/types";
import type { ID } from "@wf-agent/types";

/**
 * Agent file checkpoint filter options
 */
export interface AgentFileCheckpointFilter {
  /** Filter by agent loop ID */
  agentLoopId?: ID;
  /** Filter by checkpoint type */
  type?: "full" | "incremental";
  /** Maximum number of results */
  limit?: number;
}

/**
 * AgentFileCheckpointResourceAPI - Agent file checkpoint resource management API
 *
 * Handles all CRUD operations, createFileCheckpoint, restoreFileCheckpoint,
 * listByAgentLoop, deleteByAgentLoop, initialize, and close.
 */
export class AgentFileCheckpointResourceAPI extends BaseFileCheckpointResourceAPI<AgentFileCheckpointFilter> {
  constructor(deps: APIDependencyManager) {
    super(deps);
  }

  protected override applyFilter(
    checkpoints: FileCheckpointMetadata[],
    filter: AgentFileCheckpointFilter,
  ): FileCheckpointMetadata[] {
    return checkpoints.filter(cp => {
      if (filter.agentLoopId && cp.entityId !== filter.agentLoopId) {
        return false;
      }
      if (filter.type && cp.type !== filter.type) {
        return false;
      }
      return true;
    });
  }

  /**
   * List file checkpoints for a specific agent loop
   *
   * @param agentLoopId - Agent Loop ID to list checkpoints for
   * @param options - Additional options (limit)
   * @returns Array of checkpoint info
   */
  async listByAgentLoop(
    agentLoopId: ID,
    options?: { limit?: number },
  ): Promise<FileCheckpointInfo[]> {
    return this.listByEntity(agentLoopId, options);
  }

  /**
   * Delete all file checkpoints for a specific agent loop
   *
   * @param agentLoopId - Agent Loop ID
   * @param keepLatest - Number of latest checkpoints to keep
   * @returns Number of deleted checkpoints
   */
  async deleteByAgentLoop(agentLoopId: ID, keepLatest?: number): Promise<number> {
    return this.deleteByEntity(agentLoopId, keepLatest);
  }
}