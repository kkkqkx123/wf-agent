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
import {
  buildCheckpointCreatedEvent,
  buildCheckpointDeletedEvent,
  buildCheckpointFailedEvent,
} from "../../shared/utils/event/builders/index.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "AgentLoopCheckpointStateManager" });

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

    // Execute entity-specific cleanup after saving, excluding the newly created checkpoint
    if (this.cleanupPolicy) {
      await this.executeCleanupForEntity(this.agentLoopId, "agent", id);
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
   * @param entityId The entity ID
   * @param entityType The entity type ('workflow', 'agent', 'task')
   * @param excludeCheckpointId Optional checkpoint ID to exclude from cleanup
   * @returns Cleanup results
   */
  override async executeCleanupForEntity(
    entityId: string,
    entityType: string,
    excludeCheckpointId?: string,
  ): Promise<CleanupResult> {
    return await super.executeCleanupForEntity(entityId, entityType, excludeCheckpointId);
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
    const chainPositionFromMetadata = checkpoint.metadata?.customFields?.["chainPosition"] as number | undefined;

    // Validate agentLoopId matches expected entity binding
    const checkpointAgentLoopId = checkpoint.agentLoopId;
    if (checkpointAgentLoopId && checkpointAgentLoopId !== this.agentLoopId) {
      logger.warn("Checkpoint agentLoopId differs from manager binding, using checkpoint value", {
        expected: this.agentLoopId,
        actual: checkpointAgentLoopId,
        checkpointId: checkpoint.id,
      });
    }

    const record: CheckpointStorageMetadata = {
      entityType: "agent",
      entityId: checkpointAgentLoopId ?? this.agentLoopId,
      timestamp: checkpoint.timestamp,
      checkpointType: checkpoint.type,
      baseCheckpointId: checkpoint.type === "DELTA" ? checkpoint.baseCheckpointId : undefined,
      previousCheckpointId: checkpoint.type === "DELTA" ? checkpoint.previousCheckpointId : undefined,
      chainRootId: checkpoint.type === "DELTA" ? checkpoint.baseCheckpointId : checkpoint.id,
      chainPosition: checkpoint.type === "FULL" ? 0 : chainPositionFromMetadata,
    };

    // Only store user-facing metadata for FULL checkpoints to avoid
    // duplicating stable fields across delta chains
    if (checkpoint.type === "FULL") {
      record.customFields = { version: 1 };
    }

    return record;
  }

  protected buildCreatedEvent(checkpoint: AgentLoopCheckpoint): unknown {
    return buildCheckpointCreatedEvent({
      checkpointId: checkpoint.id,
      executionId: checkpoint.agentLoopId,
      description: checkpoint.metadata?.description,
    });
  }

  protected buildDeletedEvent(
    checkpointId: string,
    reason?: "manual" | "cleanup" | "policy",
  ): unknown {
    return buildCheckpointDeletedEvent({
      checkpointId,
      reason: reason || "cleanup",
    });
  }

  protected buildFailedEvent(
    checkpointId: string,
    error: unknown,
    operation: "create" | "restore" | "delete" = "create",
  ): unknown {
    const errorObj = error instanceof Error ? error : new Error(String(error));
    return buildCheckpointFailedEvent({
      checkpointId,
      operation,
      error: errorObj,
    });
  }
}
