/**
 * Agent Loop Checkpoint State Manager
 *
 * Manages the lifecycle of agent loop checkpoints including creation, retrieval, deletion, and cleanup.
 * Extends BaseCheckpointStateManager with agent-specific event building and metadata extraction.
 */

import type { CleanupResult, CheckpointStorageMetadata, CheckpointEntityType } from "@wf-agent/types";
import type { AgentLoopCheckpoint } from "@wf-agent/types";
import type { EventRegistry } from "../../core/registry/event-registry.js";
import type { CheckpointStorageAdapter as StorageAdapter } from "@wf-agent/storage";
import { BaseCheckpointStateManager } from "../../core/checkpoint/base-checkpoint-state-manager.js";
import { generateId } from "../../utils/id-utils.js";

/**
 * Agent Loop Checkpoint State Manager
 *
 * Extends BaseCheckpointStateManager with agent-specific logic.
 */
export class AgentLoopCheckpointStateManager extends BaseCheckpointStateManager<AgentLoopCheckpoint> {
  private agentLoopId?: string;

  constructor(
    storageAdapter: StorageAdapter,
    eventManager?: EventRegistry,
    agentLoopId?: string,
  ) {
    super(storageAdapter, eventManager);
    this.agentLoopId = agentLoopId;
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
  override async list(options?: import("@wf-agent/types").AgentCheckpointListOptions): Promise<string[]> {
    return await super.list(options);
  }

  /**
   * Delete a checkpoint (alias for delete)
   *
   * @param checkpointId The checkpoint ID to delete
   * @param reason Reason for deletion (manual, cleanup, or policy)
   */
  async deleteCheckpoint(checkpointId: string, reason: "manual" | "cleanup" | "policy" = "manual"): Promise<void> {
    await super.delete(checkpointId, reason);
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
      entityType: 'agent' as CheckpointEntityType,
      entityId: checkpoint.agentLoopId,
      timestamp: checkpoint.timestamp,
      customFields: {
        type: checkpoint.type,
        version: 1,
        baseCheckpointId: checkpoint.baseCheckpointId,
        previousCheckpointId: checkpoint.previousCheckpointId,
      },
    };
  }

  protected buildCreatedEvent(checkpoint: AgentLoopCheckpoint): unknown {
    return {
      id: generateId(),
      type: "CHECKPOINT_CREATED",
      timestamp: Date.now(),
      executionId: checkpoint.agentLoopId,
      data: {
        checkpointId: checkpoint.id,
        agentLoopId: checkpoint.agentLoopId,
        checkpointType: checkpoint.type,
        description: checkpoint.metadata?.description,
        tags: checkpoint.metadata?.tags,
      },
    };
  }

  protected buildDeletedEvent(
    checkpointId: string,
    reason?: "manual" | "cleanup" | "policy"
  ): unknown {
    return {
      id: generateId(),
      type: "CHECKPOINT_DELETED",
      timestamp: Date.now(),
      data: {
        checkpointId,
        reason: reason || "cleanup",
      },
    };
  }

  protected buildFailedEvent(
    checkpointId: string,
    error: unknown,
    operation: "create" | "restore" | "delete" = "create"
  ): unknown {
    const errorMessage = error instanceof Error ? error.message : String(error);
    const errorStack = error instanceof Error ? error.stack : undefined;

    return {
      id: generateId(),
      type: "CHECKPOINT_FAILED",
      timestamp: Date.now(),
      data: {
        checkpointId,
        operation,
        error: errorMessage,
        stackTrace: errorStack,
      },
    };
  }
}
