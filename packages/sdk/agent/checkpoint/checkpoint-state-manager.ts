/**
 * Agent Loop Checkpoint State Manager
 *
 * Manages the lifecycle of agent loop checkpoints including creation, retrieval, deletion, and cleanup.
 * Extends BaseCheckpointStateManager with agent-specific event building and metadata extraction.
 * Entity-based design for efficient checkpoint management.
 */

import type {
  CleanupResult,
  CheckpointStorageMetadata,
  AgentCheckpointListOptions,
} from "@wf-agent/types";
import type { AgentLoopCheckpoint } from "@wf-agent/types";
import type { EventRegistry } from "../../shared/registry/event-registry.js";
import type { CheckpointStorageAdapter as StorageAdapter } from "@wf-agent/storage";
import { BaseCheckpointStateManager } from "../../shared/checkpoint/base-checkpoint-state-manager.js";
import { generateId } from "../../utils/id-utils.js";

/**
 * Agent Loop Checkpoint State Manager
 *
 * Extends BaseCheckpointStateManager with agent-specific logic.
 * Enforces entity binding for efficient checkpoint management.
 */
export class AgentLoopCheckpointStateManager extends BaseCheckpointStateManager<AgentLoopCheckpoint> {
  private readonly agentLoopId: string;

  constructor(
    agentLoopId: string, // Required: entity ID for this manager
    storageAdapter: StorageAdapter,
    eventManager?: EventRegistry,
  ) {
    super(storageAdapter, eventManager);
    if (!agentLoopId) {
      throw new Error("agentLoopId is required for AgentLoopCheckpointStateManager");
    }
    this.agentLoopId = agentLoopId;
  }

  /**
   * Save a checkpoint (alias for create)
   *
   * @param checkpoint The checkpoint to save
   * @returns The checkpoint ID
   */
  async saveCheckpoint(checkpoint: AgentLoopCheckpoint): Promise<string> {
    const id = await super.create(checkpoint);

    // Execute entity-specific cleanup after saving
    if (this.cleanupPolicy) {
      await this.executeCleanupForEntity(this.agentLoopId, "agent");
    }

    return id;
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
  override async list(options?: AgentCheckpointListOptions): Promise<string[]> {
    return await super.list(options);
  }

  /**
   * Delete a checkpoint (alias for delete)
   *
   * @param checkpointId The checkpoint ID to delete
   * @param reason Reason for deletion (manual, cleanup, or policy)
   */
  async deleteCheckpoint(
    checkpointId: string,
    reason: "manual" | "cleanup" | "policy" = "manual",
  ): Promise<void> {
    await super.delete(checkpointId, reason);
  }

  /**
   * Execute the cleanup policy (alias for executeCleanup)
   *
   * Automatically clean up expired checkpoints based on the configured cleanup strategy
   *
   * @returns Cleanup results
   */
  override async executeCleanupForEntity(
    entityId: string,
    entityType: string,
  ): Promise<CleanupResult> {
    return await super.executeCleanupForEntity(entityId, entityType);
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
      entityType: "agent",
      entityId: checkpoint.agentLoopId || this.agentLoopId,
      timestamp: checkpoint.timestamp,
      customFields: {
        version: 1,
      },
      checkpointType: checkpoint.type,
      baseCheckpointId: checkpoint.type === "DELTA" ? checkpoint.baseCheckpointId : undefined,
      previousCheckpointId: checkpoint.type === "DELTA" ? checkpoint.previousCheckpointId : undefined,
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
    reason?: "manual" | "cleanup" | "policy",
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
    operation: "create" | "restore" | "delete" = "create",
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
