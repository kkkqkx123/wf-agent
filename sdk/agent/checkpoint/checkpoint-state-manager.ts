/**
 * Agent Loop Checkpoint State Manager
 *
 * Manages the lifecycle of agent loop checkpoints including creation, retrieval, deletion, and cleanup.
 * Extends BaseCheckpointStateManager with agent-specific event building and metadata extraction.
 */

import type { CleanupPolicy, CleanupResult, CheckpointStorageMetadata } from "@wf-agent/types";
import type { AgentLoopCheckpoint, AgentCheckpointMetadata } from "@wf-agent/types";
import type { EventRegistry } from "../../core/registry/event-registry.js";
import type { AgentLoopCheckpointStorageAdapter as StorageAdapter } from "@wf-agent/storage";
import { BaseCheckpointStateManager } from "../../core/checkpoint/base-checkpoint-state-manager.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ operation: "AgentLoopCheckpointStateManager" });

/**
 * Agent Loop Checkpoint State Manager
 *
 * Extends BaseCheckpointStateManager with agent-specific logic.
 */
export class AgentLoopCheckpointStateManager extends BaseCheckpointStateManager<AgentLoopCheckpoint> {
  constructor(
    storageAdapter: StorageAdapter,
    eventManager?: EventRegistry,
  ) {
    // Adapt storage adapter to the expected interface
    const adaptedAdapter = {
      save: async (id: string, data: Uint8Array, metadata: unknown) => {
        await storageAdapter.save(id, data, metadata as AgentCheckpointMetadata);
      },
      load: async (id: string) => {
        return await storageAdapter.load(id);
      },
      delete: async (id: string) => {
        await storageAdapter.delete(id);
      },
      list: async (options?: { parentId?: string; limit?: number }) => {
        // Convert base options to agent-specific options
        const agentOptions = options?.parentId
          ? { agentLoopId: options.parentId, limit: options.limit }
          : { limit: options?.limit };
        return await storageAdapter.list(agentOptions as any);
      },
      initialize: storageAdapter.initialize?.bind(storageAdapter),
      close: storageAdapter.close?.bind(storageAdapter),
    };

    super(adaptedAdapter, eventManager);
  }



  /**
   * Save a checkpoint (alias for create)
   *
   * @param checkpoint The checkpoint to save
   * @returns The checkpoint ID
   */
  async saveCheckpoint(checkpoint: AgentLoopCheckpoint): Promise<string> {
    return await super.create(checkpoint);
  }

  /**
   * Get a checkpoint by ID (alias for get)
   *
   * @param checkpointId The checkpoint ID
   * @returns The checkpoint or null if not found
   */
  async getCheckpoint(checkpointId: string): Promise<AgentLoopCheckpoint | null> {
    return await super.get(checkpointId);
  }

  /**
   * List checkpoints with optional filtering (alias for list)
   *
   * @param options Filter options
   * @returns Array of checkpoint IDs
   */
  override async list(options?: any): Promise<string[]> {
    return await super.list(options);
  }

  /**
   * Delete a checkpoint (alias for delete)
   *
   * @param checkpointId The checkpoint ID to delete
   */
  async deleteCheckpoint(checkpointId: string): Promise<void> {
    await super.delete(checkpointId);
  }

  /**
   * Execute the cleanup policy (alias for executeCleanup)
   *
   * Automatically clean up expired checkpoints based on the configured cleanup strategy
   *
   * @returns Cleanup results
   */
  override async executeCleanup(): Promise<CleanupResult> {
    return await super.executeCleanup();
  }

  /**
   * Initialize the state manager
   */
  override async initialize(): Promise<void> {
    await super.initialize();
  }

  /**
   * Clean up resources
   */
  override async cleanup(): Promise<void> {
    await super.cleanup();
  }

  // ============================================================================
  // Abstract Methods Implementation
  // ============================================================================

  protected extractStorageMetadata(checkpoint: AgentLoopCheckpoint): CheckpointStorageMetadata {
    return {
      executionId: checkpoint.agentLoopId,
      workflowId: checkpoint.agentLoopId,
      timestamp: checkpoint.timestamp,
      customFields: {
        type: checkpoint.type,
        version: 1,
      },
    };
  }

  protected buildCreatedEvent(checkpoint: AgentLoopCheckpoint): unknown {
    // TODO: Create agent loop checkpoint created event builder
    return {
      type: "AGENT_LOOP_CHECKPOINT_CREATED",
      data: {
        checkpointId: checkpoint.id,
        agentLoopId: checkpoint.agentLoopId,
      },
      timestamp: Date.now(),
    };
  }

  protected buildDeletedEvent(checkpointId: string): unknown {
    // TODO: Create agent loop checkpoint deleted event builder
    return {
      type: "AGENT_LOOP_CHECKPOINT_DELETED",
      data: { checkpointId },
      timestamp: Date.now(),
    };
  }

  protected buildFailedEvent(checkpointId: string, error: unknown): unknown {
    const errorMessage = error instanceof Error ? error : new Error(String(error));
    // TODO: Create agent loop checkpoint failed event builder
    return {
      type: "AGENT_LOOP_CHECKPOINT_FAILED",
      data: {
        checkpointId,
        error: errorMessage.message,
      },
      timestamp: Date.now(),
    };
  }
}
