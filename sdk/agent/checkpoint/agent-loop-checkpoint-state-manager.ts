/**
 * Agent Loop Checkpoint State Manager
 *
 * Manages the lifecycle of agent loop checkpoints including creation, retrieval, deletion, and cleanup.
 * Mirrors the CheckpointState implementation for workflow checkpoints.
 */

import type { CleanupPolicy, CleanupResult } from "@wf-agent/types";
import type { AgentLoopCheckpoint, AgentCheckpointMetadata, AgentCheckpointListOptions } from "@wf-agent/types";
import type { EventRegistry } from "../../core/registry/event-registry.js";
import type { AgentLoopCheckpointStorageAdapter } from "@wf-agent/storage";
import { SerializationRegistry } from "../../core/serialization/serialization-registry.js";
import {
  AgentLoopCheckpointSerializer,
  registerAgentLoopCheckpointSerializer,
} from "../../core/serialization/entities/agent-loop-checkpoint-serializer.js";
import { createCleanupStrategy } from "../../core/utils/checkpoint/cleanup-policy.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ operation: "AgentLoopCheckpointStateManager" });

/**
 * Agent Loop Checkpoint State Manager
 *
 * Provides state management for agent loop checkpoints with support for:
 * - CRUD operations
 * - Cleanup policies
 * - Event emission
 * - Serialization/deserialization
 */
export class AgentLoopCheckpointStateManager {
  private storageAdapter: AgentLoopCheckpointStorageAdapter;
  private eventManager?: EventRegistry;
  private serializationRegistry: SerializationRegistry;
  private checkpointSerializer: AgentLoopCheckpointSerializer;
  private checkpointSizes: Map<string, number> = new Map();
  private cleanupPolicy?: CleanupPolicy;

  constructor(
    storageAdapter: AgentLoopCheckpointStorageAdapter,
    eventManager?: EventRegistry,
  ) {
    this.storageAdapter = storageAdapter;
    this.eventManager = eventManager;
    this.serializationRegistry = SerializationRegistry.getInstance();

    // Get serializer from registry or create if not registered
    const serializer = this.serializationRegistry.getSerializer("agentLoopCheckpoint");
    if (serializer instanceof AgentLoopCheckpointSerializer) {
      this.checkpointSerializer = serializer;
    } else {
      // Fallback: create a new instance and register it
      this.checkpointSerializer = new AgentLoopCheckpointSerializer();
      registerAgentLoopCheckpointSerializer();
    }
  }

  /**
   * Set up a cleanup policy
   *
   * @param policy Configuration for the cleanup policy
   */
  setCleanupPolicy(policy: CleanupPolicy): void {
    this.cleanupPolicy = policy;
    logger.info("Cleanup policy set for agent loop checkpoints", { policy });
  }

  /**
   * Get the cleanup policy
   *
   * @returns Cleanup policy configuration
   */
  getCleanupPolicy(): CleanupPolicy | undefined {
    return this.cleanupPolicy;
  }

  /**
   * Save a checkpoint
   *
   * @param checkpoint The checkpoint to save
   * @returns The checkpoint ID
   */
  async saveCheckpoint(checkpoint: AgentLoopCheckpoint): Promise<string> {
    try {
      logger.debug("Saving agent loop checkpoint", {
        checkpointId: checkpoint.id,
        agentLoopId: checkpoint.agentLoopId,
        type: checkpoint.type,
      });

      const serializedData = await this.checkpointSerializer.serializeCheckpoint(checkpoint);

      const metadata: AgentCheckpointMetadata = {
        agentLoopId: checkpoint.agentLoopId,
        timestamp: checkpoint.timestamp,
        type: checkpoint.type,
        version: 1,
      };

      await this.storageAdapter.save(checkpoint.id, serializedData, metadata);
      this.checkpointSizes.set(checkpoint.id, serializedData.length);

      // Emit event
      if (this.eventManager) {
        // TODO: Create agent loop checkpoint created event builder
        // For now, we'll skip event emission until event builders are created
        logger.debug("Checkpoint saved successfully", { checkpointId: checkpoint.id });
      }

      logger.info("Agent loop checkpoint saved", {
        checkpointId: checkpoint.id,
        agentLoopId: checkpoint.agentLoopId,
        dataSize: serializedData.length,
      });

      return checkpoint.id;
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      logger.error("Failed to save agent loop checkpoint", {
        checkpointId: checkpoint.id,
        error: errorMessage,
      });
      throw error;
    }
  }

  /**
   * Get a checkpoint by ID
   *
   * @param checkpointId The checkpoint ID
   * @returns The checkpoint or null if not found
   */
  async getCheckpoint(checkpointId: string): Promise<AgentLoopCheckpoint | null> {
    try {
      logger.debug("Loading agent loop checkpoint", { checkpointId });

      const data = await this.storageAdapter.load(checkpointId);
      if (!data) {
        logger.debug("Checkpoint not found", { checkpointId });
        return null;
      }

      const checkpoint = await this.checkpointSerializer.deserializeCheckpoint(data);

      logger.debug("Checkpoint loaded successfully", { checkpointId });
      return checkpoint;
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      logger.error("Failed to load agent loop checkpoint", {
        checkpointId,
        error: errorMessage,
      });
      throw error;
    }
  }

  /**
   * List checkpoints with optional filtering
   *
   * @param options Filter options
   * @returns Array of checkpoint IDs
   */
  async list(options?: AgentCheckpointListOptions): Promise<string[]> {
    try {
      logger.debug("Listing agent loop checkpoints", { options });
      const ids = await this.storageAdapter.list(options);
      logger.debug(`Found ${ids.length} checkpoints`);
      return ids;
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      logger.error("Failed to list agent loop checkpoints", { error: errorMessage });
      throw error;
    }
  }

  /**
   * Delete a checkpoint
   *
   * @param checkpointId The checkpoint ID to delete
   */
  async deleteCheckpoint(checkpointId: string): Promise<void> {
    try {
      logger.debug("Deleting agent loop checkpoint", { checkpointId });

      await this.storageAdapter.delete(checkpointId);
      this.checkpointSizes.delete(checkpointId);

      // Emit event
      if (this.eventManager) {
        // TODO: Create agent loop checkpoint deleted event builder
        logger.debug("Checkpoint deleted", { checkpointId });
      }

      logger.info("Agent loop checkpoint deleted", { checkpointId });
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      logger.error("Failed to delete agent loop checkpoint", {
        checkpointId,
        error: errorMessage,
      });
      throw error;
    }
  }

  /**
   * Execute the cleanup policy
   *
   * Automatically clean up expired checkpoints based on the configured cleanup strategy
   *
   * @returns Cleanup results
   */
  async executeCleanup(): Promise<CleanupResult> {
    if (!this.cleanupPolicy) {
      logger.debug("No cleanup policy configured, skipping cleanup");
      return {
        deletedCheckpointIds: [],
        deletedCount: 0,
        freedSpaceBytes: 0,
        remainingCount: 0,
      };
    }

    logger.info("Executing cleanup policy for agent loop checkpoints", {
      policy: this.cleanupPolicy,
    });

    // Get all checkpoint IDs
    const checkpointIds = await this.storageAdapter.list();

    // Get the metadata and size of all checkpoints
    const checkpointInfoArray: Array<{
      checkpointId: string;
      metadata: AgentCheckpointMetadata;
    }> = [];

    for (const checkpointId of checkpointIds) {
      try {
        const data = await this.storageAdapter.load(checkpointId);
        if (data) {
          const checkpoint = await this.checkpointSerializer.deserializeCheckpoint(data);
          const metadata: AgentCheckpointMetadata = {
            agentLoopId: checkpoint.agentLoopId,
            timestamp: checkpoint.timestamp,
            type: checkpoint.type,
            version: 1,
          };
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

    // Create a cleanup strategy instance
    // Note: We cast the metadata to CheckpointStorageMetadata for compatibility
    // The cleanup strategies only use timestamp field which both types have
    const strategy = createCleanupStrategy(
      this.cleanupPolicy,
      this.checkpointSizes,
    );

    // Execute the cleanup strategy
    // Cast checkpointInfoArray to be compatible with CheckpointInfo[]
    const toDeleteIds = strategy.execute(
      checkpointInfoArray as unknown as import("@wf-agent/types").CheckpointInfo[],
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

    const remainingCount = await this.storageAdapter.list().then((ids) => ids.length);

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
    logger.info("Initializing AgentLoopCheckpointStateManager");
    await this.storageAdapter.initialize();
  }

  /**
   * Clean up resources
   */
  async cleanup(): Promise<void> {
    logger.info("Cleaning up AgentLoopCheckpointStateManager");
    if ("close" in this.storageAdapter && typeof this.storageAdapter.close === "function") {
      await this.storageAdapter.close();
    }
  }
}
