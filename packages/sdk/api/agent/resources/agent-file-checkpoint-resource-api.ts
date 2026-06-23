/**
 * AgentFileCheckpointResourceAPI - Agent File Checkpoint Resource Management API
 *
 * Provides APIs for managing agent loop file state checkpoints.
 * Uses FileCheckpointManager from the storage layer for actual checkpoint operations.
 * This API is only available when file checkpointing is enabled in SDKOptions.
 */

import { SimplifiedCrudResourceAPI } from "../../shared/resources/generic-resource-api.js";
import type { APIDependencyManager } from "../../shared/core/sdk-dependencies.js";
import type { FileCheckpointMetadata, FileCheckpointInfo } from "@wf-agent/types";
import type {
  FileCheckpointManager,
  FileCheckpointCreateResult,
  FileCheckpointRestoreResult,
} from "@wf-agent/common-utils";
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
 */
export class AgentFileCheckpointResourceAPI extends SimplifiedCrudResourceAPI<
  FileCheckpointMetadata,
  string,
  AgentFileCheckpointFilter
> {
  private deps: APIDependencyManager;

  constructor(deps: APIDependencyManager) {
    super();
    this.deps = deps;
  }

  /**
   * Get the FileCheckpointManager instance
   * Throws if file checkpointing is not configured
   */
  private getManager(): FileCheckpointManager {
    const manager = this.deps.getFileCheckpointManager();
    if (!manager) {
      throw new Error(
        "File checkpoint is not enabled. Set fileCheckpoint.enabled = true in SDKOptions.",
      );
    }
    return manager;
  }

  /**
   * Check whether file checkpointing is enabled
   */
  isEnabled(): boolean {
    return this.deps.getFileCheckpointManager() !== undefined;
  }

  /**
   * Create a file checkpoint for the given agent loop
   *
   * @param agentLoopId - Agent Loop ID
   * @returns The created checkpoint ID and metadata
   */
  async createFileCheckpoint(agentLoopId: ID): Promise<FileCheckpointCreateResult> {
    const manager = this.getManager();
    return manager.createCheckpoint(agentLoopId);
  }

  /**
   * Restore workspace files from a file checkpoint
   *
   * @param agentLoopId - Agent Loop ID associated with the checkpoint
   * @param checkpointId - Checkpoint ID to restore from
   * @returns Restore result with counts
   */
  async restoreFileCheckpoint(
    agentLoopId: ID,
    checkpointId: string,
  ): Promise<FileCheckpointRestoreResult> {
    const manager = this.getManager();
    return manager.restoreCheckpoint(agentLoopId, checkpointId);
  }

  // ============================================================================
  // Implement SimplifiedCrudResourceAPI abstract methods
  // ============================================================================

  protected async getResource(id: string): Promise<FileCheckpointMetadata | null> {
    const manager = this.getManager();
    const storage = manager.getStorage();
    const result = await storage.load(id);
    return result?.metadata ?? null;
  }

  protected async getAllResources(): Promise<FileCheckpointMetadata[]> {
    const manager = this.getManager();
    const storage = manager.getStorage();
    const ids = await storage.list();
    const results: FileCheckpointMetadata[] = [];
    for (const id of ids) {
      const checkpoint = await storage.load(id);
      if (checkpoint) {
        results.push(checkpoint.metadata);
      }
    }
    return results;
  }

  protected async createResource(resource: FileCheckpointMetadata): Promise<void> {
    const manager = this.getManager();
    const storage = manager.getStorage();
    const checkpointId = `manual_${Date.now()}`;
    await storage.save(checkpointId, resource, new Map());
  }

  protected async updateResource(
    id: string,
    updates: Partial<FileCheckpointMetadata>,
  ): Promise<void> {
    const manager = this.getManager();
    const storage = manager.getStorage();
    const existing = await storage.load(id);
    if (!existing) {
      throw new Error(`File checkpoint not found: ${id}`);
    }
    const updated: FileCheckpointMetadata = {
      ...existing.metadata,
      ...updates,
    } as FileCheckpointMetadata;
    await storage.save(id, updated, existing.files);
  }

  protected async deleteResource(id: string): Promise<void> {
    const manager = this.getManager();
    const storage = manager.getStorage();
    await storage.delete(id);
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
    const manager = this.getManager();
    const storage = manager.getStorage();
    const result = await storage.listByEntity(agentLoopId, options);
    return result.map(r => ({
      checkpointId: r.id,
      metadata: r.metadata,
    }));
  }

  /**
   * Delete all file checkpoints for a specific agent loop
   *
   * @param agentLoopId - Agent Loop ID
   * @param keepLatest - Number of latest checkpoints to keep
   * @returns Number of deleted checkpoints
   */
  async deleteByAgentLoop(agentLoopId: ID, keepLatest?: number): Promise<number> {
    const manager = this.getManager();
    const storage = manager.getStorage();
    return storage.deleteByEntity(agentLoopId, keepLatest);
  }

  /**
   * Initialize the file checkpoint manager
   * Must be called before any checkpoint operations
   */
  async initialize(): Promise<void> {
    const manager = this.getManager();
    await manager.initialize();
  }

  /**
   * Close the file checkpoint manager and release resources
   */
  async close(): Promise<void> {
    const manager = this.getManager();
    await manager.close();
  }
}
