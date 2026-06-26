/**
 * Checkpoint Status Manager
 * A stateful service that maintains the internal status of checkpoints
 */

import type { Checkpoint, CheckpointStorageMetadata } from "@wf-agent/types";
import type { CheckpointStorageAdapter } from "@wf-agent/storage";
import type { EventRegistry } from "../../shared/registry/event-registry.js";
import { BaseCheckpointStateManager } from "../../shared/checkpoint/core/base-state-manager.js";
import {
  buildCheckpointCreatedEvent,
  buildCheckpointDeletedEvent,
  buildCheckpointFailedEvent,
} from "../../shared/utils/event/builders/index.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ operation: "checkpoint-state-manager" });

/**
 * Checkpoint State Manager
 * Extends BaseCheckpointStateManager with workflow-specific event building and metadata extraction.
 * Entity-based design for efficient checkpoint management.
 */
export class CheckpointState extends BaseCheckpointStateManager<Checkpoint> {
  constructor(
    storageAdapter: CheckpointStorageAdapter,
    eventManager?: EventRegistry,
  ) {
    super(storageAdapter, eventManager);
  }

  /**
   * Clean up all checkpoints for the specified workflow execution
   * Optimized to use entity-level batch deletion
   *
   * @param workflowExecutionId WorkflowExecution ID (required)
   * @returns Number of checkpoints deleted
   */
  async cleanupWorkflowExecutionCheckpoints(workflowExecutionId: string): Promise<number> {
    if (!workflowExecutionId) {
      throw new Error("workflowExecutionId is required for cleanupWorkflowExecutionCheckpoints");
    }

    logger.info("Cleaning up workflow execution checkpoints", { workflowExecutionId });

    // Use optimized entity-level batch deletion
    const deletedCount = await this.storageAdapter.deleteByEntity(workflowExecutionId, "workflow");

    logger.info("Workflow execution checkpoints cleaned up", {
      workflowExecutionId,
      deletedCount,
    });

    return deletedCount;
  }

  /**
   * Create a checkpoint
   * @param checkpointData Checkpoint data
   * @returns Checkpoint ID
   */
  override async create(checkpointData: Checkpoint): Promise<string> {
    const id = await super.create(checkpointData);

    // Execute entity-specific cleanup after saving, excluding the newly created checkpoint
    if (this.cleanupPolicy && checkpointData.executionId) {
      await this.executeCleanupForEntity(checkpointData.executionId, "workflow", id);
    }

    return id;
  }

  /**
   * Get a checkpoint
   * @param checkpointId: Checkpoint ID
   * @returns: Checkpoint object
   */
  override async get(checkpointId: string): Promise<Checkpoint | null> {
    return await super.get(checkpointId);
  }

  /**
   * List of checkpoint IDs
   * @param options Query options
   * @returns Array of checkpoint IDs
   */
  override async list(
    options?: import("@wf-agent/types").CheckpointListOptions,
  ): Promise<string[]> {
    // Convert CheckpointListOptions to base list options
    const baseOptions = options
      ? {
          parentId: options.parentId,
          limit: options.limit,
        }
      : undefined;
    return await super.list(baseOptions);
  }

  /**
   * Delete a checkpoint
   * @param checkpointId Checkpoint ID
   * @param reason Reason for deletion (manual, cleanup, or policy)
   */
  override async delete(
    checkpointId: string,
    reason: "manual" | "cleanup" | "policy" = "manual",
  ): Promise<void> {
    // Call parent implementation which handles deletion and events
    await super.delete(checkpointId, reason);
  }

  // ============================================================================
  // Abstract Methods Implementation
  // ============================================================================

  protected extractStorageMetadata(checkpoint: Checkpoint): CheckpointStorageMetadata {
    if (!checkpoint.executionId) {
      throw new Error("checkpoint.executionId is required for storage metadata");
    }

    const chainPositionFromMetadata = checkpoint.metadata?.customFields?.["chainPosition"] as number | undefined;

    const record: CheckpointStorageMetadata = {
      entityType: "workflow",
      entityId: checkpoint.executionId,
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
      record.tags = checkpoint.metadata?.tags;
      record.customFields = checkpoint.metadata?.customFields;
    }

    return record;
  }

  protected buildCreatedEvent(checkpoint: Checkpoint): unknown {
    return buildCheckpointCreatedEvent({
      executionId: checkpoint.executionId,
      checkpointId: checkpoint.id,
      workflowId: checkpoint.workflowId,
      description: checkpoint.metadata?.description,
    });
  }

  protected async buildDeletedEvent(
    checkpointId: string,
    reason?: "manual" | "cleanup" | "policy",
  ): Promise<unknown> {
    // Try to get executionId from the checkpoint before deletion
    let executionId = "";
    try {
      const checkpoint = await this.get(checkpointId);
      if (checkpoint) {
        executionId = checkpoint.executionId;
      }
    } catch (error) {
      logger.warn("Failed to fetch checkpoint for deleted event", {
        checkpointId,
        error: error instanceof Error ? error.message : String(error),
      });
    }

    return buildCheckpointDeletedEvent({
      executionId,
      checkpointId,
      reason,
    });
  }

  protected buildFailedEvent(
    checkpointId: string,
    error: unknown,
    operation: "create" | "restore" | "delete" = "create",
  ): unknown {
    const errorMessage = error instanceof Error ? error : new Error(String(error));
    return buildCheckpointFailedEvent({
      executionId: "", // We don't have executionId in error context
      operation,
      error: errorMessage,
      checkpointId,
    });
  }
}
