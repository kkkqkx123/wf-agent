/**
 * Checkpoint Status Manager
 * A stateful service that maintains the internal status of checkpoints
 */

import type {
  Checkpoint,
  CheckpointStorageMetadata,
} from "@wf-agent/types";
import type { CheckpointStorageAdapter as StorageAdapter } from "@wf-agent/storage";
import type { EventRegistry } from "../../core/registry/event-registry.js";
import type { CheckpointStorageAdapter } from "../../core/checkpoint/types.js";
import { BaseCheckpointStateManager } from "../../core/checkpoint/base-checkpoint-state-manager.js";
import {
  buildCheckpointCreatedEvent,
  buildCheckpointDeletedEvent,
  buildCheckpointFailedEvent,
} from "../../core/utils/event/builders/index.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ operation: "checkpoint-state-manager" });

/**
 * Checkpoint State Manager
 * Extends BaseCheckpointStateManager with workflow-specific event building and metadata extraction.
 * Entity-based design for efficient checkpoint management.
 */
export class CheckpointState extends BaseCheckpointStateManager<Checkpoint> {
  /**
   * Constructor
   * @param storageAdapter Storage adapter interface (implemented by the application layer)
   * @param eventManager Event manager (optional)
   */
  constructor(
    storageAdapter: StorageAdapter,
    eventManager?: EventRegistry
  ) {
    // Adapt storage adapter to the expected interface
    const adaptedAdapter = {
      save: async (id: string, data: Uint8Array, metadata: unknown) => {
        await storageAdapter.save(id, data, metadata as CheckpointStorageMetadata);
      },
      load: async (id: string) => {
        return await storageAdapter.load(id);
      },
      delete: async (id: string) => {
        await storageAdapter.delete(id);
      },
      list: async (options?: { parentId?: string; limit?: number }) => {
        return await storageAdapter.list(options);
      },
      listWithMetadata: async (options?: Record<string, unknown>) => {
        return await storageAdapter.listWithMetadata(options);
      },
      listByEntityWithMetadata: async (entityId: string, entityType: string, options?: Record<string, unknown>) => {
        // Type assertion needed until @wf-agent/storage types are updated
        return await (storageAdapter as unknown as CheckpointStorageAdapter).listByEntityWithMetadata(entityId, entityType, options);
      },
      getLatestByEntity: async (entityId: string, entityType: string, count?: number, includeData?: boolean) => {
        return await (storageAdapter as unknown as CheckpointStorageAdapter).getLatestByEntity(entityId, entityType, count, includeData);
      },
      deleteByEntity: async (entityId: string, entityType: string, options?: Record<string, unknown>) => {
        return await (storageAdapter as unknown as CheckpointStorageAdapter).deleteByEntity(entityId, entityType, options);
      },
      initialize: storageAdapter.initialize?.bind(storageAdapter),
      close: storageAdapter.close?.bind(storageAdapter),
    };

    super(adaptedAdapter, eventManager);
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
      throw new Error('workflowExecutionId is required for cleanupWorkflowExecutionCheckpoints');
    }
    
    logger.info("Cleaning up workflow execution checkpoints", { workflowExecutionId });

    // Use optimized entity-level batch deletion
    const deletedCount = await (this.storageAdapter as unknown as CheckpointStorageAdapter).deleteByEntity(
      workflowExecutionId,
      'workflow'
    );

    logger.info("Workflow execution checkpoints cleaned up", { 
      workflowExecutionId, 
      deletedCount 
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
    
    // Execute entity-specific cleanup after saving
    if (this.cleanupPolicy && checkpointData.executionId) {
      await this.executeCleanupForEntity(checkpointData.executionId, 'workflow');
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
  override async list(options?: import("@wf-agent/types").CheckpointListOptions): Promise<string[]> {
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
    reason: "manual" | "cleanup" | "policy" = "manual"
  ): Promise<void> {
    // Call parent implementation which handles deletion and events
    await super.delete(checkpointId, reason);
  }

  // ============================================================================
  // Abstract Methods Implementation
  // ============================================================================

  protected extractStorageMetadata(checkpoint: Checkpoint): CheckpointStorageMetadata {
    if (!checkpoint.executionId) {
      throw new Error('checkpoint.executionId is required for storage metadata');
    }
    
    return {
      entityType: 'workflow',
      entityId: checkpoint.executionId,
      timestamp: checkpoint.timestamp,
      tags: checkpoint.metadata?.tags,
      customFields: checkpoint.metadata?.customFields,
    };
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
    reason?: "manual" | "cleanup" | "policy"
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
    operation: "create" | "restore" | "delete" = "create"
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
