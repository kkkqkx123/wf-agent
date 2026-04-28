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
import type { CheckpointStorageCallback } from "@wf-agent/storage";
import type { EventRegistry } from "../../core/registry/event-registry.js";
import { LifecycleCapable } from "../../core/types/lifecycle-capable.js";
import {
  serializeCheckpoint,
  deserializeCheckpoint,
  createCleanupStrategy,
} from "../execution/utils/index.js";
import { getErrorOrNew } from "@wf-agent/common-utils";
import { safeEmit } from "../execution/utils/index.js";
import { StateManagementError } from "@wf-agent/types";
import { mergeMetadata } from "../../utils/metadata-utils.js";
import type { Metadata } from "@wf-agent/types";
import {
  buildCheckpointCreatedEvent,
  buildCheckpointFailedEvent,
  buildCheckpointDeletedEvent,
} from "../execution/utils/event/index.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import { logError, emitErrorEvent } from "../../core/utils/error-utils.js";

const logger = createContextualLogger({ operation: "checkpoint-state-manager" });

/**
 * Extract storage metadata from the checkpoint.
 */
function extractStorageMetadata(checkpoint: Checkpoint): CheckpointStorageMetadata {
  return {
    threadId: checkpoint.threadId,
    workflowId: checkpoint.workflowId,
    timestamp: checkpoint.timestamp,
    tags: checkpoint.metadata?.tags,
    customFields: checkpoint.metadata?.customFields,
  };
}

/**
 * Checkpoint State Manager
 */
export class CheckpointState implements LifecycleCapable<void> {
  private storageCallback: CheckpointStorageCallback;
  private cleanupPolicy?: CleanupPolicy;
  private checkpointSizes: Map<string, number> = new Map(); // checkpointId -> size in bytes
  private eventManager?: EventRegistry;

  /**
   * Constructor
   * @param storageCallback: Storage callback interface (implemented by the application layer)
   * @param eventManager: Event manager (optional)
   */
  constructor(storageCallback: CheckpointStorageCallback, eventManager?: EventRegistry) {
    this.storageCallback = storageCallback;
    this.eventManager = eventManager;
  }

  /**
   * Set up a cleaning policy
   *
   * @param policy Configuration for the cleaning policy
   */
  setCleanupPolicy(policy: CleanupPolicy): void {
    this.cleanupPolicy = policy;
  }

  /**
   * Get the cleanup strategy
   *
   * @returns Cleanup strategy configuration
   */
  getCleanupPolicy(): CleanupPolicy | undefined {
    return this.cleanupPolicy;
  }

  /**
   * Execute the cleanup strategy
   *
   * Automatically clean up expired checkpoints based on the configured cleanup strategy
   *
   * @returns Cleanup results
   */
  async executeCleanup(): Promise<CleanupResult> {
    if (!this.cleanupPolicy) {
      return {
        deletedCheckpointIds: [],
        deletedCount: 0,
        freedSpaceBytes: 0,
        remainingCount: 0,
      };
    }

    logger.debug("Executing cleanup policy", { policy: this.cleanupPolicy });

    // Get all checkpoint IDs
    const checkpointIds = await this.storageCallback.list();

    // Get the metadata and size of all checkpoints.
    const checkpointInfoArray: Array<{
      checkpointId: string;
      metadata: CheckpointStorageMetadata;
    }> = [];
    for (const checkpointId of checkpointIds) {
      const data = await this.storageCallback.load(checkpointId);
      if (data) {
        const checkpoint = deserializeCheckpoint(data);
        const metadata = extractStorageMetadata(checkpoint);
        checkpointInfoArray.push({ checkpointId, metadata });
        this.checkpointSizes.set(checkpointId, data.length);
      }
    }

    // Create a cleanup policy instance
    const strategy = createCleanupStrategy(this.cleanupPolicy, this.checkpointSizes);

    // Execute the cleanup strategy.
    const toDeleteIds = strategy.execute(checkpointInfoArray);

    // Delete checkpoints
    let freedSpaceBytes = 0;
    for (const checkpointId of toDeleteIds) {
      const size = this.checkpointSizes.get(checkpointId) || 0;
      await this.storageCallback.delete(checkpointId);
      freedSpaceBytes += size;
      this.checkpointSizes.delete(checkpointId);
    }

    logger.info("Cleanup policy executed", {
      deletedCount: toDeleteIds.length,
      freedSpaceBytes,
      remainingCount: checkpointIds.length - toDeleteIds.length,
    });

    return {
      deletedCheckpointIds: toDeleteIds,
      deletedCount: toDeleteIds.length,
      freedSpaceBytes,
      remainingCount: checkpointIds.length - toDeleteIds.length,
    };
  }

  /**
   * Clean up all checkpoints for the specified thread
   *
   * @param threadId Thread ID
   * @returns Number of checkpoints deleted
   */
  async cleanupThreadCheckpoints(threadId: string): Promise<number> {
    logger.info("Cleaning up thread checkpoints", { threadId });

    const checkpointIds = await this.storageCallback.list({ threadId });

    for (const checkpointId of checkpointIds) {
      await this.delete(checkpointId, "cleanup");
    }

    logger.info("Thread checkpoints cleaned up", { threadId, deletedCount: checkpointIds.length });

    return checkpointIds.length;
  }

  /**
   * Create a checkpoint
   * @param checkpointData Checkpoint data
   * @returns Checkpoint ID
   */
  async create(checkpointData: Checkpoint): Promise<string> {
    const threadId = checkpointData.threadId;
    const checkpointId = checkpointData.id;

    logger.debug("Creating checkpoint", {
      threadId,
      checkpointId,
      workflowId: checkpointData.workflowId,
    });

    try {
      // Use the passed checkpointData.id instead of generating a new ID
      const data = serializeCheckpoint(checkpointData);
      const storageMetadata = extractStorageMetadata(checkpointData);

      await this.storageCallback.save(checkpointId, data, storageMetadata);
      this.checkpointSizes.set(checkpointId, data.length);

      logger.info("Checkpoint created", { threadId, checkpointId, sizeBytes: data.length });

      // Trigger the checkpoint creation event.
      const createdEvent = buildCheckpointCreatedEvent({
        threadId: checkpointData.threadId,
        checkpointId,
        workflowId: checkpointData.workflowId,
        description: checkpointData.metadata?.description,
      });
      await safeEmit(this.eventManager, createdEvent);

      // Execute the cleanup strategy (if configured).
      if (this.cleanupPolicy) {
        try {
          await this.executeCleanup();
        } catch (error) {
          // Create a state management error
          const stateManagementError = new StateManagementError(
            "Error executing cleanup policy",
            "checkpoint",
            "delete",
            undefined,
            undefined,
            undefined,
            { originalError: getErrorOrNew(error) },
          );

          // Record error logs
          logError(stateManagementError, {
            threadId: checkpointData.threadId,
            workflowId: checkpointData.workflowId,
          });

          // Trigger an error event
          await emitErrorEvent(this.eventManager, {
            threadId: checkpointData.threadId,
            workflowId: checkpointData.workflowId,
            error: stateManagementError,
          });

          throw stateManagementError;
        }
      }

      return checkpointId;
    } catch (error) {
      // Triggering a checkpoint failure event.
      const failedEvent = buildCheckpointFailedEvent({
        threadId: checkpointData.threadId,
        operation: "create",
        error: getErrorOrNew(error),
        checkpointId: checkpointData.id,
        workflowId: checkpointData.workflowId,
      });
      await safeEmit(this.eventManager, failedEvent);
      throw error;
    }
  }

  /**
   * Get a checkpoint
   * @param checkpointId: Checkpoint ID
   * @returns: Checkpoint object
   */
  async get(checkpointId: string): Promise<Checkpoint | null> {
    const data = await this.storageCallback.load(checkpointId);
    if (!data) {
      return null;
    }
    return deserializeCheckpoint(data);
  }

  /**
   * List of checkpoint IDs
   * @param options Query options
   * @returns Array of checkpoint IDs
   */
  async list(options?: import("@wf-agent/types").CheckpointListOptions): Promise<string[]> {
    // Convert CheckpointListOptions to CheckpointStorageListOptions
    const storageOptions: import("@wf-agent/types").CheckpointStorageListOptions | undefined =
      options
        ? {
            threadId: options.parentId as string,
            tags: options.tags,
            limit: options.limit,
            offset: options.offset,
          }
        : undefined;
    return this.storageCallback.list(storageOptions);
  }

  /**
   * Delete a checkpoint
   * @param checkpointId Checkpoint ID
   * @param reason Reason for deletion
   */
  async delete(
    checkpointId: string,
    reason: "manual" | "cleanup" | "policy" = "manual",
  ): Promise<void> {
    logger.debug("Deleting checkpoint", { checkpointId, reason });

    try {
      // First, obtain the checkpoint information (used to trigger the event).
      const checkpoint = await this.get(checkpointId);

      await this.storageCallback.delete(checkpointId);
      this.checkpointSizes.delete(checkpointId);

      logger.info("Checkpoint deleted", { checkpointId, reason, threadId: checkpoint?.threadId });

      // Trigger a checkpoint deletion event
      if (checkpoint) {
        const deletedEvent = buildCheckpointDeletedEvent({
          threadId: checkpoint.threadId,
          checkpointId,
          workflowId: checkpoint.workflowId,
          reason,
        });
        await safeEmit(this.eventManager, deletedEvent);
      }
    } catch (error) {
      // Triggering a checkpoint failure event.
      const failedEvent = buildCheckpointFailedEvent({
        threadId: "",
        operation: "delete",
        error: getErrorOrNew(error),
        checkpointId,
      });
      await safeEmit(this.eventManager, failedEvent);
      throw error;
    }
  }

  /**
   * Create a node-level checkpoint
   * @param checkpointData Checkpoint data
   * @param nodeId Node ID
   * @returns Checkpoint ID
   */
  async createNodeCheckpoint(checkpointData: Checkpoint, nodeId: string): Promise<string> {
    const metadata = checkpointData.metadata || {};
    const nodeCheckpointData: Checkpoint = {
      ...checkpointData,
      metadata: mergeMetadata(metadata as Metadata, {
        description: `Node checkpoint for node ${nodeId}`,
        customFields: mergeMetadata((metadata.customFields || {}) as Metadata, { nodeId }),
      }),
    };
    return this.create(nodeCheckpointData);
  }

  /**
   * Clean up resources
   * Clear all checkpoints
   */
  async cleanup(): Promise<void> {
    const checkpointIds = await this.storageCallback.list();
    for (const checkpointId of checkpointIds) {
      await this.storageCallback.delete(checkpointId);
    }
    this.checkpointSizes.clear();
  }

  /**
   * Create a state snapshot
   * TheCheckpointStateManager itself does not maintain the state; it returns an empty snapshot.
   */
  createSnapshot(): void {
    // CheckpointState does not maintain its own state and therefore does not require snapshots.
  }

  /**
   * Recover from a snapshot
   * TheCheckpointStateManager itself does not maintain a state, so there is no need for recovery.
   */
  restoreFromSnapshot(): void {
    // TheCheckpointStateManager itself does not maintain any state and therefore does not require recovery.
  }
}
