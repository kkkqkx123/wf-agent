/**
 * Checkpoint Status Manager
 * A stateful service that maintains the internal status of checkpoints
 */

import type {
  Checkpoint,
  CheckpointStorageMetadata,
  CleanupPolicy,
  CleanupResult,
} from "@wf-agent/types";
import type { CheckpointStorageAdapter as StorageAdapter } from "@wf-agent/storage";
import type { EventRegistry } from "../../core/registry/event-registry.js";
import { BaseCheckpointStateManager } from "../../core/checkpoint/base-checkpoint-state-manager.js";
import {
  buildCheckpointCreatedEvent,
  buildCheckpointFailedEvent,
  buildCheckpointDeletedEvent,
} from "../execution/utils/event/index.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import type { CheckpointOptions } from "./checkpoint-coordinator.js";

const logger = createContextualLogger({ operation: "checkpoint-state-manager" });

/**
 * Checkpoint State Manager
 * Extends BaseCheckpointStateManager with workflow-specific event building and metadata extraction.
 */
export class CheckpointState extends BaseCheckpointStateManager<Checkpoint> {
  /**
   * Constructor
   * @param storageAdapter Storage adapter interface (implemented by the application layer)
   * @param eventManager Event manager (optional)
   * @param cleanupScheduler Background cleanup scheduler (optional) - kept for backward compatibility but not used
   */
  constructor(
    storageAdapter: StorageAdapter,
    eventManager?: EventRegistry,
    _cleanupScheduler?: any // Kept for backward compatibility
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
      initialize: storageAdapter.initialize?.bind(storageAdapter),
      close: storageAdapter.close?.bind(storageAdapter),
    };

    super(adaptedAdapter, eventManager);
  }



  /**
   * Clean up all checkpoints for the specified workflow execution
   *
   * @param workflowExecutionId WorkflowExecution ID
   * @returns Number of checkpoints deleted
   */
  async cleanupWorkflowExecutionCheckpoints(workflowExecutionId: string): Promise<number> {
    logger.info("Cleaning up workflow execution checkpoints", { workflowExecutionId });

    const checkpointIds = await this.list({ parentId: workflowExecutionId });

    for (const checkpointId of checkpointIds) {
      await this.delete(checkpointId);
    }

    logger.info("Workflow execution checkpoints cleaned up", { workflowExecutionId, deletedCount: checkpointIds.length });

    return checkpointIds.length;
  }

  /**
   * Create a checkpoint
   * @param checkpointData Checkpoint data
   * @param options Checkpoint options (sync mode, timeout, etc.) - kept for backward compatibility
   * @returns Checkpoint ID
   */
  override async create(checkpointData: Checkpoint, _options?: CheckpointOptions): Promise<string> {
    // Call parent implementation which handles serialization, saving, events, and cleanup
    return await super.create(checkpointData);
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
   * @param reason Reason for deletion - kept for backward compatibility but not used
   */
  override async delete(
    checkpointId: string,
    _reason: "manual" | "cleanup" | "policy" = "manual"
  ): Promise<void> {
    // Call parent implementation which handles deletion and events
    await super.delete(checkpointId);
  }

  /**
   * Create a node-level checkpoint
   * @param checkpointData Checkpoint data
   * @param nodeId Node ID
   * @returns Checkpoint ID
   */
  async createNodeCheckpoint(checkpointData: Checkpoint, nodeId: string): Promise<string> {
    // This method is deprecated - use createCheckpoint with metadata instead
    logger.warn("createNodeCheckpoint is deprecated, use createCheckpoint with metadata");
    return this.create(checkpointData);
  }

  // ============================================================================
  // Abstract Methods Implementation
  // ============================================================================

  protected extractStorageMetadata(checkpoint: Checkpoint): unknown {
    return {
      executionId: checkpoint.executionId,
      workflowId: checkpoint.workflowId,
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

  protected buildDeletedEvent(checkpointId: string): unknown {
    // Deleted event needs executionId, which we don't have here
    // Return a minimal event structure
    return {
      type: "CHECKPOINT_DELETED" as const,
      data: { checkpointId },
      timestamp: Date.now(),
    };
  }

  protected buildFailedEvent(checkpointId: string, error: unknown): unknown {
    const errorMessage = error instanceof Error ? error : new Error(String(error));
    // Since we don't have context about which operation failed, we default to "create"
    return buildCheckpointFailedEvent({
      executionId: "", // We don't have executionId here
      operation: "create", // Default to create since most failures happen during creation
      error: errorMessage,
      checkpointId,
    });
  }
}
