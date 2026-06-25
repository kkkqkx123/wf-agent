/**
 * Base Checkpoint State Manager
 *
 * Provides common CRUD operations and cleanup policy execution.
 * Subclasses only need to implement event building for their specific checkpoint types.
 */

import type {
  BaseCheckpoint,
  CleanupPolicy,
  CleanupResult,
  CheckpointStorageMetadata,
  CheckpointInfo,
  BaseEvent,
} from "@wf-agent/types";
import type { EventRegistry } from "../registry/event-registry.js";
import { StateCodec } from "@wf-agent/common-utils";
import { createCleanupStrategy } from "./utils/cleanup-policy.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import type { CheckpointStorageAdapter } from "./types.js";

const logger = createContextualLogger({ component: "BaseCheckpointStateManager" });

/**
 * Base Checkpoint State Manager
 *
 * Provides common CRUD operations and cleanup policy execution.
 * Subclasses only need to implement event building for their specific checkpoint types.
 *
 * @template TCheckpoint - The specific checkpoint type (e.g., Workflow Checkpoint or Agent Loop Checkpoint)
 */
export abstract class BaseCheckpointStateManager<
  TCheckpoint extends BaseCheckpoint<unknown, unknown>,
> {
  protected storageAdapter: CheckpointStorageAdapter;
  protected eventManager?: EventRegistry;
  protected cleanupPolicy?: CleanupPolicy;
  protected codec: StateCodec;
  protected checkpointSizes: Map<string, number> = new Map();

  constructor(
    storageAdapter: CheckpointStorageAdapter,
    eventManager?: EventRegistry,
    cleanupPolicy?: CleanupPolicy,
  ) {
    this.storageAdapter = storageAdapter;
    this.eventManager = eventManager;
    this.cleanupPolicy = cleanupPolicy;
    this.codec = new StateCodec();
  }

  /**
   * Create a checkpoint
   * @param checkpoint The checkpoint to save
   * @returns The checkpoint ID
   */
  async create(checkpoint: TCheckpoint): Promise<string> {
    try {
      logger.debug("Creating checkpoint", {
        checkpointId: checkpoint.id,
        type: checkpoint.type,
      });

      const serializedData = await this.codec.serialize(checkpoint);
      const metadata = this.extractStorageMetadata(checkpoint);

      await this.storageAdapter.save(checkpoint.id, serializedData, metadata);
      this.checkpointSizes.set(checkpoint.id, serializedData.length);

      // Emit created event (implemented by subclass)
      if (this.eventManager) {
        const createdEvent = this.buildCreatedEvent(checkpoint);
        await this.eventManager.emit(createdEvent as BaseEvent);
      }

      logger.info("Checkpoint created", {
        checkpointId: checkpoint.id,
        dataSize: serializedData.length,
      });

      return checkpoint.id;
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      logger.error("Failed to create checkpoint", {
        checkpointId: checkpoint.id,
        error: errorMessage,
      });

      // Emit failed event (implemented by subclass)
      if (this.eventManager) {
        const failedEvent = this.buildFailedEvent(checkpoint.id, error, "create");
        await this.eventManager.emit(failedEvent as BaseEvent);
      }

      throw error;
    }
  }

  /**
   * Get a checkpoint by ID
   * @param checkpointId Checkpoint ID
   * @returns The checkpoint or null if not found
   */
  async get(checkpointId: string): Promise<TCheckpoint | null> {
    // Step 1: Load raw data from storage
    let data: Uint8Array | null;
    try {
      data = await this.storageAdapter.load(checkpointId);
    } catch (error) {
      logger.error("Storage error while loading checkpoint", {
        checkpointId,
        error: error instanceof Error ? error.message : String(error),
      });
      throw error; // Storage failure should propagate, not be swallowed
    }

    // Not found — return null (not an error)
    if (!data) {
      return null;
    }

    // Deserialization (data corruption is a distinct error from not-found)
    try {
      const checkpoint = await this.codec.deserialize<TCheckpoint>(data);
      this.checkpointSizes.set(checkpointId, data.length);
      return checkpoint;
    } catch (error) {
      logger.error("Failed to deserialize checkpoint (data may be corrupted)", {
        checkpointId,
        size: data.length,
        error: error instanceof Error ? error.message : String(error),
      });
      throw new Error(`Checkpoint data corrupted: ${checkpointId} (${data.length} bytes)`, {
        cause: error,
      });
    }
  }

  /**
   * Delete a checkpoint
   * @param checkpointId Checkpoint ID
   * @param reason Reason for deletion (manual, cleanup, or policy)
   */
  async delete(
    checkpointId: string,
    reason: "manual" | "cleanup" | "policy" = "manual",
  ): Promise<void> {
    try {
      await this.storageAdapter.delete(checkpointId);
      this.checkpointSizes.delete(checkpointId);

      logger.info("Checkpoint deleted", { checkpointId, reason });

      // Emit deleted event (implemented by subclass)
      if (this.eventManager) {
        const deletedEvent = await this.buildDeletedEvent(checkpointId, reason);
        await this.eventManager.emit(deletedEvent as BaseEvent);
      }
    } catch (error) {
      logger.error("Failed to delete checkpoint", {
        checkpointId,
        error: error instanceof Error ? error.message : String(error),
      });
      throw error;
    }
  }

  /**
   * List checkpoints
   * @param options List options (parentId, limit, etc.)
   * @returns Array of checkpoint IDs
   */
  async list(options?: { parentId?: string; limit?: number }): Promise<string[]> {
    return await this.storageAdapter.list(options);
  }

  /**
   * Execute cleanup policy for a specific entity
   * Optimized to only scan and clean checkpoints belonging to the specified entity
   *
   * @param entityId The entity ID
   * @param entityType The entity type ('workflow', 'agent', 'task')
   * @param excludeCheckpointId Optional checkpoint ID to exclude from cleanup (e.g., newly created)
   * @param policy Optional cleanup policy (overrides default if provided)
   * @returns Cleanup result
   */
  async executeCleanupForEntity(
    entityId: string,
    entityType: string,
    excludeCheckpointId?: string,
    policy?: CleanupPolicy,
  ): Promise<CleanupResult> {
    const targetPolicy = policy || this.cleanupPolicy;

    if (!targetPolicy) {
      return {
        deletedCheckpointIds: [],
        deletedCount: 0,
        freedSpaceBytes: 0,
        remainingCount: 0,
      };
    }

    logger.info("Executing cleanup policy for entity", {
      entityId,
      entityType,
      policy: targetPolicy.type,
      excludeCheckpointId,
    });

    // Load only this entity's checkpoints metadata (optimized query using indexes)
    const checkpointInfoArray = await this.storageAdapter.listByEntityWithMetadata(
      entityId,
      entityType,
    );

    // Update checkpoint sizes from metadata if available
    for (const info of checkpointInfoArray) {
      const metadata = info.metadata as CheckpointStorageMetadata;
      const customFields = metadata.customFields;
      const size = (customFields?.["blobSize"] as number) || 0;
      if (size > 0) {
        this.checkpointSizes.set(info.id, size);
      }
    }

    // Convert to CheckpointInfo format for strategy, excluding the specified checkpoint
    const checkpointInfo: CheckpointInfo[] = checkpointInfoArray
      .filter(info => info.id !== excludeCheckpointId)
      .map(info => ({
        checkpointId: info.id,
        metadata: info.metadata as CheckpointStorageMetadata,
      }));

    // Execute cleanup strategy
    const strategy = createCleanupStrategy(targetPolicy, this.checkpointSizes);
    const toDeleteIds = strategy.execute(checkpointInfo);

    // Delete checkpoints
    let freedSpaceBytes = 0;
    for (const checkpointId of toDeleteIds) {
      try {
        const size = this.checkpointSizes.get(checkpointId) || 0;
        await this.storageAdapter.delete(checkpointId);
        this.checkpointSizes.delete(checkpointId);
        freedSpaceBytes += size;
      } catch (error) {
        logger.warn("Failed to delete checkpoint during cleanup", {
          checkpointId,
          error: error instanceof Error ? error.message : String(error),
        });
      }
    }

    const remainingCount = checkpointInfoArray.length - toDeleteIds.length;

    logger.info("Entity cleanup completed", {
      entityId,
      entityType,
      deletedCount: toDeleteIds.length,
      freedSpaceBytes,
      remainingCount,
    });

    return {
      deletedCheckpointIds: toDeleteIds,
      deletedCount: toDeleteIds.length,
      freedSpaceBytes,
      remainingCount,
    };
  }

  /**
   * Initialize the state manager
   */
  async initialize(): Promise<void> {
    logger.info("Initializing checkpoint state manager");
    await this.storageAdapter.initialize();
    await this.loadCheckpointSizes();
  }

  /**
   * Load checkpoint sizes from storage to rebuild the in-memory cache
   * This ensures size-based cleanup policies work correctly after restart
   */
  private async loadCheckpointSizes(): Promise<void> {
    try {
      const checkpointsWithMetadata = await this.storageAdapter.listWithMetadata();
      for (const { id, metadata } of checkpointsWithMetadata) {
        const size = metadata.blobSize ?? 0;
        if (size > 0) {
          this.checkpointSizes.set(id, size);
        }
      }
      logger.info("Loaded checkpoint sizes from storage", {
        count: this.checkpointSizes.size,
      });
    } catch (error) {
      logger.warn("Failed to load checkpoint sizes from storage", {
        error: error instanceof Error ? error.message : String(error),
      });
    }
  }

  /**
   * Clean up resources
   */
  async cleanup(): Promise<void> {
    logger.info("Cleaning up checkpoint state manager");
    await this.storageAdapter.close();
  }

  // ============================================================================
  // Abstract Methods - Must be implemented by subclasses
  // ============================================================================

  /**
   * Extract storage metadata from checkpoint
   * @param checkpoint The checkpoint
   * @returns Storage metadata
   */
  protected abstract extractStorageMetadata(checkpoint: TCheckpoint): CheckpointStorageMetadata;

  /**
   * Build checkpoint created event
   * @param checkpoint The created checkpoint
   * @returns Event object
   */
  protected abstract buildCreatedEvent(checkpoint: TCheckpoint): unknown;

  /**
   * Build checkpoint deleted event
   * @param checkpointId The deleted checkpoint ID
   * @param reason Reason for deletion
   * @returns Event object (can be Promise for async implementations)
   */
  protected abstract buildDeletedEvent(
    checkpointId: string,
    reason?: "manual" | "cleanup" | "policy",
  ): unknown | Promise<unknown>;

  /**
   * Build checkpoint failed event
   * @param checkpointId The failed checkpoint ID
   * @param error The error
   * @param operation The operation that failed (create, restore, delete)
   * @returns Event object
   */
  protected abstract buildFailedEvent(
    checkpointId: string,
    error: unknown,
    operation?: "create" | "restore" | "delete",
  ): unknown;
}
