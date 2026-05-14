/**
 * Base Checkpoint State Manager
 * 
 * Provides common CRUD operations and cleanup policy execution.
 * Subclasses only need to implement event building for their specific checkpoint types.
 */

import type { BaseCheckpoint, CleanupPolicy, CleanupResult, CheckpointStorageMetadata } from "@wf-agent/types";
import type { EventRegistry } from "../registry/event-registry.js";
import { StateCodec } from "@wf-agent/common-utils";
import { createCleanupStrategy } from "../utils/checkpoint/cleanup-policy.js";
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
  TCheckpoint extends BaseCheckpoint<unknown, unknown>
> {
  protected storageAdapter: CheckpointStorageAdapter<TCheckpoint>;
  protected eventManager?: EventRegistry;
  protected cleanupPolicy?: CleanupPolicy;
  protected codec: StateCodec;
  protected checkpointSizes: Map<string, number> = new Map();

  constructor(
    storageAdapter: CheckpointStorageAdapter<TCheckpoint>,
    eventManager?: EventRegistry,
    cleanupPolicy?: CleanupPolicy
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
        await this.eventManager.emit(createdEvent as import("@wf-agent/types").BaseEvent);
      }

      logger.info("Checkpoint created", {
        checkpointId: checkpoint.id,
        dataSize: serializedData.length,
      });

      // Execute cleanup policy if configured
      if (this.cleanupPolicy) {
        await this.executeCleanup();
      }

      return checkpoint.id;
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      logger.error("Failed to create checkpoint", {
        checkpointId: checkpoint.id,
        error: errorMessage,
      });

      // Emit failed event (implemented by subclass)
      if (this.eventManager) {
        const failedEvent = this.buildFailedEvent(checkpoint.id, error);
        await this.eventManager.emit(failedEvent as import("@wf-agent/types").BaseEvent);
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
    try {
      const data = await this.storageAdapter.load(checkpointId);
      if (!data) {
        return null;
      }

      const checkpoint = await this.codec.deserialize<TCheckpoint>(data);
      this.checkpointSizes.set(checkpointId, data.length);
      return checkpoint;
    } catch (error) {
      logger.error("Failed to load checkpoint", {
        checkpointId,
        error: error instanceof Error ? error.message : String(error),
      });
      return null;
    }
  }

  /**
   * Delete a checkpoint
   * @param checkpointId Checkpoint ID
   */
  async delete(checkpointId: string): Promise<void> {
    try {
      await this.storageAdapter.delete(checkpointId);
      this.checkpointSizes.delete(checkpointId);

      logger.info("Checkpoint deleted", { checkpointId });

      // Emit deleted event (implemented by subclass)
      if (this.eventManager) {
        const deletedEvent = this.buildDeletedEvent(checkpointId);
        await this.eventManager.emit(deletedEvent as import("@wf-agent/types").BaseEvent);
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
   * Execute cleanup policy
   * @returns Cleanup result
   */
  async executeCleanup(): Promise<CleanupResult> {
    if (!this.cleanupPolicy) {
      return {
        deletedCheckpointIds: [],
        deletedCount: 0,
        freedSpaceBytes: 0,
        remainingCount: await this.storageAdapter.list().then(ids => ids.length),
      };
    }

    logger.info("Executing cleanup policy", {
      policy: this.cleanupPolicy.type,
    });

    // Load all checkpoints for cleanup evaluation
    const checkpointIds = await this.storageAdapter.list();
    const checkpointInfoArray: Array<{
      checkpointId: string;
      metadata: CheckpointStorageMetadata;
    }> = [];

    for (const checkpointId of checkpointIds) {
      try {
        const data = await this.storageAdapter.load(checkpointId);
        if (data) {
          const checkpoint = await this.codec.deserialize<TCheckpoint>(data);
          const metadata = this.extractStorageMetadata(checkpoint);
          checkpointInfoArray.push({ checkpointId, metadata });
          this.checkpointSizes.set(checkpointId, data.length);
        }
      } catch (error) {
        logger.warn("Failed to load checkpoint for cleanup", {
          checkpointId,
          error: error instanceof Error ? error.message : String(error),
        });
      }
    }

    // Execute cleanup strategy
    const strategy = createCleanupStrategy(this.cleanupPolicy, this.checkpointSizes);
    const toDeleteIds = strategy.execute(
      checkpointInfoArray
    );

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

    const remainingCount = await this.storageAdapter.list().then(ids => ids.length);

    logger.info("Cleanup completed", {
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
    if (this.storageAdapter.initialize) {
      await this.storageAdapter.initialize();
    }
  }

  /**
   * Clean up resources
   */
  async cleanup(): Promise<void> {
    logger.info("Cleaning up checkpoint state manager");
    if (this.storageAdapter.close) {
      await this.storageAdapter.close();
    }
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
   * @returns Event object
   */
  protected abstract buildDeletedEvent(checkpointId: string): unknown;

  /**
   * Build checkpoint failed event
   * @param checkpointId The failed checkpoint ID
   * @param error The error
   * @returns Event object
   */
  protected abstract buildFailedEvent(checkpointId: string, error: unknown): unknown;
}
