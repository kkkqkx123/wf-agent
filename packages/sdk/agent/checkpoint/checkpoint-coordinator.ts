/**
 * Agent Loop Checkpoint Coordinator
 *
 * A service that coordinates the entire checkpoint process
 * Extends BaseCheckpointCoordinator to eliminate code duplication.
 */

import { AgentLoopEntity } from "../entities/agent-loop-entity.js";
import type { ConversationSession } from "../../shared/messaging/conversation-session.js";
import type {
  CheckpointMetadata,
  DeltaStorageConfig,
  TCheckpointType,
  AgentLoopCheckpoint,
  AgentLoopStateSnapshot,
  AgentLoopRuntimeConfig,
  DeltaCheckpoint,
  FullCheckpoint,
  AgentLoopDelta,
  AgentCheckpointContentConfig,
  CheckpointFormatVersion,
  CheckpointErrorStrategy,
  CheckpointErrorContext,
} from "@wf-agent/types";
import { AgentCheckpointError, CURRENT_CHECKPOINT_FORMAT_VERSION } from "@wf-agent/types";
import { BaseCheckpointCoordinator } from "../../shared/checkpoint/base-checkpoint-coordinator.js";
import type { CheckpointDependencies as BaseCheckpointDependencies } from "../../shared/checkpoint/types.js";
import { CheckpointVersionManager } from "../../shared/checkpoint/checkpoint-version-manager.js";
import { CheckpointErrorHandler } from "../../shared/checkpoint/checkpoint-error-handler.js";
import { buildCheckpointMetadata } from "../../shared/checkpoint/utils/metadata-builder.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import { getExecutionEventBus } from "../../shared/events/index.js";
import type { FileCheckpointManager } from "@wf-agent/common-utils";

const logger = createContextualLogger({ component: "AgentLoopCheckpointCoordinator" });

/**
 * Checkpoint creation options
 */
export interface CheckpointOptions {
  /** Checkpoint metadata */
  metadata?: CheckpointMetadata;
  /** Optional description */
  description?: string;
  /** Optional tags */
  tags?: string[];
  /** Content configuration for what to include in checkpoint */
  contentConfig?: AgentCheckpointContentConfig;
}

/**
 * Checkpoint dependencies (extends base with agent-specific fields)
 */
export interface CheckpointDependencies extends BaseCheckpointDependencies<AgentLoopCheckpoint> {
  /** Save checkpoints */
  saveCheckpoint: (checkpoint: AgentLoopCheckpoint) => Promise<string>;
  /** Get checkpoints */
  getCheckpoint: (id: string) => Promise<AgentLoopCheckpoint | null>;
  /** List the checkpoints */
  listCheckpoints: (agentLoopId: string) => Promise<string[]>;
  /** Incremental storage configuration (optional) */
  deltaConfig?: DeltaStorageConfig;
  /** Conversation manager for message persistence (optional) */
  conversationManager?: ConversationSession;
  /** File checkpoint manager for persisting external file state (optional) */
  fileCheckpointManager?: FileCheckpointManager;
}

/**
 * Agent Loop checkpoint coordinator
 *
 * Design Principles:
 * - Instance-based for dependency injection and testability
 * - Coordinates the entire checkpoint lifecycle
 * - Extends BaseCheckpointCoordinator to eliminate duplication
 * - Config must be provided at construction time (via constructor) for restore operations
 *   because AgentLoopRuntimeConfig contains callbacks that cannot be serialized
 */
export class AgentLoopCheckpointCoordinator extends BaseCheckpointCoordinator<
  AgentLoopCheckpoint,
  AgentLoopEntity,
  AgentLoopStateSnapshot
> {
  /**
   * Config for restore operations.
   * Required because AgentLoopRuntimeConfig contains callbacks that cannot be serialized.
   * Must be set before calling restoreFromCheckpoint().
   */
  private restoreConfig?: AgentLoopRuntimeConfig;

  /**
   * Version manager for format compatibility and migration
   */
  private versionManager: CheckpointVersionManager;

  /**
   * Error handler for checkpoint operations
   */
  private checkpointErrorHandler?: CheckpointErrorHandler;

  /**
   * Error handling strategy
   */
  private checkpointErrorStrategy: CheckpointErrorStrategy = "warn";

  /**
   * @param config AgentLoopRuntimeConfig for restoration (must be provided for restore operations)
   */
  constructor(config?: AgentLoopRuntimeConfig) {
    super();
    this.restoreConfig = config;
    this.versionManager = new CheckpointVersionManager(logger);
  }

  /**
   * Set config for restore operations
   * @param config AgentLoopRuntimeConfig for restoration
   */
  setConfig(config: AgentLoopRuntimeConfig): void {
    this.restoreConfig = config;
  }
  /**
   * Create a checkpoint
   * @param entity Agent Loop entity
   * @param dependencies dependencies
   * @param options checkpoint options
   * @returns checkpoint ID
   */
  override async createCheckpoint(
    entity: AgentLoopEntity,
    dependencies: CheckpointDependencies,
    options?: CheckpointOptions,
  ): Promise<string> {
    const mergedMetadata = buildCheckpointMetadata({
      metadata: options?.metadata,
      description: options?.description,
      tags: options?.tags,
    });

    const checkpointId = await super.createCheckpoint(
      entity,
      dependencies,
      mergedMetadata,
      { contentConfig: options?.contentConfig },
    );

    // Publish checkpoint state change event (Plan C)
    const eventBus = getExecutionEventBus();
    await eventBus.publish({
      type: "state_changed",
      executionId: entity.id,
      timestamp: Date.now(),
      previousStatus: entity.state.status,
      newStatus: entity.state.status,
      changes: {
        checkpointCreated: checkpointId,
        description: options?.description,
        tags: options?.tags,
      },
    });

    // Create file checkpoint if manager is available
    if (dependencies.fileCheckpointManager) {
      try {
        await dependencies.fileCheckpointManager.createCheckpoint(entity.id);
        logger.info("File checkpoint created alongside agent loop checkpoint", {
          agentLoopId: entity.id,
          checkpointId,
        });
      } catch (error) {
        logger.warn("File checkpoint creation failed (non-fatal, agent checkpoint saved)", {
          agentLoopId: entity.id,
          error: error instanceof Error ? error.message : String(error),
        });
      }
    }

    return checkpointId;
  }

  /**
   * Restore Agent Loop entity from checkpoint
   * @param checkpointId checkpointId
   * @param dependencies dependencies
   * @returns Recovered Agent Loop Entity
   */
  async restoreAgentLoopFromCheckpoint(
    checkpointId: string,
    dependencies: CheckpointDependencies,
  ): Promise<AgentLoopEntity> {
    try {
      // Retrieve and validate checkpoint
      const checkpoint = await dependencies.getCheckpoint(checkpointId);
      if (!checkpoint) {
        throw new Error("Checkpoint not found");
      }

      // Validate version metadata
      const formatVersion = (checkpoint.metadata?.customFields?.["formatVersion"] as CheckpointFormatVersion) || CURRENT_CHECKPOINT_FORMAT_VERSION;

      // Check compatibility and migrate if needed
      const compatibility = this.versionManager.checkCompatibility(formatVersion);
      if (!compatibility.compatible) {
        throw new Error(`Checkpoint version not compatible: ${compatibility.reason}`);
      }

      if (compatibility.requiresMigration) {
        logger.info("Checkpoint requires migration, starting migration process", {
          checkpointId,
          reason: compatibility.reason,
        });
        const migrationResult = await this.versionManager.migrateCheckpoint(checkpoint);
        if (!migrationResult.success) {
          throw new Error(`Checkpoint migration failed: ${migrationResult.errors?.join(", ")}`);
        }
      }

      // Use parent's restore logic to get the entity
      const entity = await super.restoreFromCheckpoint(checkpointId, dependencies);

      // Publish state change event for restoration (Plan C)
      const eventBus = getExecutionEventBus();
      await eventBus.publish({
        type: "state_changed",
        executionId: entity.id,
        timestamp: Date.now(),
        newStatus: entity.state.status,
        changes: {
          restored: true,
          checkpointId,
        },
      });

      // Restore file checkpoint if manager is available
      if (dependencies.fileCheckpointManager) {
        try {
          const fileCheckpoints = await dependencies.fileCheckpointManager
            .getStorage()
            .listByEntity(entity.id, { limit: 1 });
          if (fileCheckpoints.length > 0) {
            const result = await dependencies.fileCheckpointManager.restoreCheckpoint(
              entity.id,
              fileCheckpoints[0]!.id,
            );
            logger.info("File checkpoint restored alongside agent loop checkpoint", {
              agentLoopId: entity.id,
              checkpointId,
              restoredCount: result.restoredCount,
              deletedCount: result.deletedCount,
              skippedCount: result.skippedCount,
            });
          }
        } catch (error) {
          logger.warn("File checkpoint restore failed (non-fatal, agent state restored)", {
            agentLoopId: entity.id,
            error: error instanceof Error ? error.message : String(error),
          });
        }
      }

      return entity;
    } catch (error) {
      if (error instanceof Error && error.message.includes("Checkpoint not found")) {
        throw new AgentCheckpointError(
          `Checkpoint not found: ${checkpointId}`,
          "restore",
          checkpointId,
        );
      }
      throw error;
    }
  }

  // ============================================================================
  // Abstract Methods Implementation
  // ============================================================================

  /**
   * Extracting a state snapshot
   *
   * Only serializes persistent execution progress data (iteration count, status, tool calls).
   * Does NOT include:
   * - `config`: Contains callbacks, must be re-provided by application on restore
   * - `messages`: Managed by ConversationSession, not AgentLoopState
   *
   * Respects content filtering options from AgentCheckpointContentConfig:
   * - includeState: Whether to include status, iteration count, etc. (default: true)
   * - includeMessages: Whether to include message history (default: false)
   * - messageLimit: Max number of messages to include
   * - includeToolCalls: Whether to include tool call records (default: true)
   * - toolCallLimit: Max number of tool calls to include
   *
   * Plan C: Includes execution records (errors, interruptions, events) that are
   * part of AgentLoopState and automatically serialized to checkpoint.
   *
   * @param entity Agent Loop entity
   * @returns Status Snapshot
   */
  protected extractState(
    entity: AgentLoopEntity,
    context?: { contentConfig?: AgentCheckpointContentConfig },
  ): AgentLoopStateSnapshot {
    const contentConfig = context?.contentConfig;

    // Get the full snapshot from AgentLoopState (includes all new Plan C fields)
    const fullSnapshot = entity.state.createSnapshot();
    const snapshot: Record<string, unknown> = {};

    // Include execution state by default
    if (contentConfig?.includeState !== false) {
      snapshot['status'] = fullSnapshot.status;
      snapshot['currentIteration'] = fullSnapshot.currentIteration;
      snapshot['toolCallCount'] = fullSnapshot.toolCallCount;
      snapshot['startTime'] = fullSnapshot.startTime;
      snapshot['endTime'] = fullSnapshot.endTime;
      snapshot['error'] = fullSnapshot.error;

      // Plan C: Include execution records
      if (fullSnapshot.errorRecords) {
        snapshot['errorRecords'] = fullSnapshot.errorRecords;
      }
      if (fullSnapshot.interruptionRecords) {
        snapshot['interruptionRecords'] = fullSnapshot.interruptionRecords;
      }
      if (fullSnapshot.eventRecords) {
        snapshot['eventRecords'] = fullSnapshot.eventRecords;
      }
    }

    // Include tool calls by default, unless explicitly disabled
    if (contentConfig?.includeToolCalls !== false) {
      if (fullSnapshot.iterationHistory) {
        snapshot['iterationHistory'] = fullSnapshot.iterationHistory;
      }
      if (fullSnapshot.currentIterationRecord) {
        snapshot['currentIterationRecord'] = fullSnapshot.currentIterationRecord;
      }
    }

    // Include streaming state fields
    if (fullSnapshot.isStreaming) {
      snapshot['isStreaming'] = fullSnapshot.isStreaming;
    }
    if (fullSnapshot.streamMessage) {
      snapshot['streamMessage'] = fullSnapshot.streamMessage;
    }
    if (fullSnapshot.pendingToolCallIds) {
      snapshot['pendingToolCallIds'] = fullSnapshot.pendingToolCallIds;
    }

    // Include trigger state by default for tracking trigger fires and limits
    if (contentConfig?.includeState !== false) {
      snapshot['triggerState'] = entity.exportTriggerState();
    }

    return snapshot as unknown as AgentLoopStateSnapshot;
  }

  /**
   * Build checkpoint object
   */
  protected async buildCheckpoint(
    entity: AgentLoopEntity,
    currentState: AgentLoopStateSnapshot,
    checkpointType: TCheckpointType,
    checkpointId: string,
    timestamp: number,
    previousCheckpointIds: string[],
    dependencies: CheckpointDependencies,
    metadata?: CheckpointMetadata,
  ): Promise<AgentLoopCheckpoint> {
    const { getCheckpoint } = dependencies;

    if (checkpointType === "FULL") {
      return {
        id: checkpointId,
        agentLoopId: entity.id,
        timestamp,
        type: "FULL",
        snapshot: currentState,
        metadata,
      };
    }

    // Creating incremental checkpoints
    return this.buildDeltaCheckpoint(
      entity,
      currentState,
      checkpointId,
      timestamp,
      previousCheckpointIds,
      getCheckpoint,
      metadata,
    );
  }

  /**
   * Extract parent ID from checkpoint
   */
  protected extractParentId(checkpoint: AgentLoopCheckpoint): string {
    return checkpoint.agentLoopId;
  }

  /**
   * Create entity from restored state snapshot
   *
   * Uses the stored `restoreConfig` to provide AgentLoopRuntimeConfig.
   * Config must be set before calling restoreFromCheckpoint() via constructor or setConfig().
   *
   * @param parentId Parent entity ID (used as entity id)
   * @param snapshot Restored state snapshot
   * @returns Reconstructed AgentLoopEntity with config injected
   * @throws Error if restoreConfig is not set
   */
  protected createEntityFromSnapshot(
    parentId: string,
    snapshot: AgentLoopStateSnapshot,
  ): AgentLoopEntity {
    if (!this.restoreConfig) {
      throw new Error(
        "AgentLoopRuntimeConfig is required for restore. " +
          "Set it via constructor: new AgentLoopCheckpointCoordinator(config) " +
          "or via setConfig(config) before calling restoreFromCheckpoint().",
      );
    }
    return AgentLoopEntity.fromSnapshot(parentId, snapshot, this.restoreConfig);
  }

  /**
   * Build delta checkpoint
   */
  private async buildDeltaCheckpoint(
    entity: AgentLoopEntity,
    currentState: AgentLoopStateSnapshot,
    checkpointId: string,
    timestamp: number,
    previousCheckpointIds: string[],
    getCheckpoint: (id: string) => Promise<AgentLoopCheckpoint | null>,
    metadata?: CheckpointMetadata,
  ): Promise<AgentLoopCheckpoint> {
    const previousCheckpointId = previousCheckpointIds[0]!;
    const previousCheckpoint = await getCheckpoint(previousCheckpointId);

    if (!previousCheckpoint) {
      // If the previous checkpoint cannot be obtained, downgrade to the full checkpoint
      return {
        id: checkpointId,
        agentLoopId: entity.id,
        timestamp,
        type: "FULL" as const,
        snapshot: currentState,
        metadata,
      };
    }

    // Find baseline checkpoints (full checkpoints with snapshot)
    const baseCheckpoint = await this.findBaseCheckpoint(previousCheckpoint, getCheckpoint);

    // If a checkpoint containing a snapshot is still not found, downgrade to the full checkpoint
    if (!baseCheckpoint?.snapshot) {
      return {
        id: checkpointId,
        agentLoopId: entity.id,
        timestamp,
        type: "FULL" as const,
        snapshot: currentState,
        metadata,
      };
    }

    // Calculate the difference using inherited diffCalculator
    const delta = this.diffCalculator.calculateDelta(
      baseCheckpoint.snapshot as unknown as Record<string, unknown>,
      currentState as unknown as Record<string, unknown>,
    );

    // Find the baseline checkpoint ID
    const baseCheckpointId =
      previousCheckpoint.type === "FULL"
        ? previousCheckpoint.id
        : previousCheckpoint.baseCheckpointId!;

    return {
      id: checkpointId,
      agentLoopId: entity.id,
      timestamp,
      type: "DELTA",
      baseCheckpointId,
      previousCheckpointId,
      delta,
      metadata,
    };
  }

  /**
   * Find base checkpoint for delta calculation
   */
  private async findBaseCheckpoint(
    previousCheckpoint: AgentLoopCheckpoint,
    getCheckpoint: (id: string) => Promise<AgentLoopCheckpoint | null>,
  ): Promise<AgentLoopCheckpoint | null> {
    if (previousCheckpoint.type === "FULL") {
      return previousCheckpoint;
    }

    // If the previous checkpoint is delta, the nearest complete checkpoint needs to be found
    if (previousCheckpoint.baseCheckpointId) {
      const base = await getCheckpoint(previousCheckpoint.baseCheckpointId);
      if (base && base.snapshot) {
        return base;
      }
    }

    return null;
  }

  /**
   * Verify checkpoint integrity and compatibility
   */
  protected override validateCheckpoint(checkpoint: AgentLoopCheckpoint): void {
    // Call parent validation first
    super.validateCheckpoint(checkpoint);

    // Additional agent-specific validation
    if (!checkpoint.agentLoopId) {
      throw new AgentCheckpointError(
        "Invalid checkpoint: missing agentLoopId",
        "validate",
        checkpoint.id,
        checkpoint.agentLoopId,
      );
    }

    // Validation against checkpoint type
    if (checkpoint.type === "DELTA") {
      const deltaCheckpoint = checkpoint as DeltaCheckpoint<AgentLoopDelta>;
      if (!deltaCheckpoint.delta && !deltaCheckpoint.previousCheckpointId) {
        throw new AgentCheckpointError(
          "Invalid delta checkpoint: missing delta data and previous checkpoint reference",
          "validate",
          checkpoint.id,
          checkpoint.agentLoopId,
        );
      }
    } else {
      const fullCheckpoint = checkpoint as FullCheckpoint<AgentLoopStateSnapshot>;
      if (!fullCheckpoint.snapshot) {
        throw new AgentCheckpointError(
          "Invalid full checkpoint: missing state snapshot",
          "validate",
          checkpoint.id,
          checkpoint.agentLoopId,
        );
      }

      // Verify the snapshot structure
      const { snapshot } = fullCheckpoint;
      if (!snapshot.status) {
        throw new AgentCheckpointError(
          "Invalid checkpoint: incomplete state snapshot",
          "validate",
          checkpoint.id,
          checkpoint.agentLoopId,
        );
      }
    }
  }

  /**
   * Get version manager for compatibility checks and migrations
   */
  getVersionManager(): CheckpointVersionManager {
    return this.versionManager;
  }

  /**
   * Check if checkpoint needs version migration
   */
  needsVersionMigration(checkpoint: AgentLoopCheckpoint): boolean {
    const formatVersion = (checkpoint.metadata?.customFields?.["formatVersion"] as CheckpointFormatVersion) || CURRENT_CHECKPOINT_FORMAT_VERSION;
    const compatibility = this.versionManager.checkCompatibility(formatVersion);
    return compatibility.requiresMigration;
  }

  /**
   * Set checkpoint error handling configuration
   *
   * Enable custom error handling strategy for checkpoint operations.
   * Supports multiple strategies: silent, warn, strict, callback.
   *
   * @param errorStrategy Error handling strategy
   * @param onError Optional callback for "callback" strategy
   */
  setCheckpointErrorHandling(
    errorStrategy: CheckpointErrorStrategy,
    onError?: (error: Error, context: CheckpointErrorContext) => void | Promise<void>,
  ): void {
    this.checkpointErrorStrategy = errorStrategy;
    this.checkpointErrorHandler = new CheckpointErrorHandler(
      { strategy: errorStrategy, onError },
      logger,
    );

    logger.debug("Agent checkpoint error handling configured", {
      strategy: errorStrategy,
      hasCallback: !!onError,
    });
  }

  /**
   * Set checkpoint error handling strategy at runtime
   * @param strategy Error handling strategy
   */
  setCheckpointErrorStrategy(strategy: CheckpointErrorStrategy): void {
    this.checkpointErrorStrategy = strategy;

    if (!this.checkpointErrorHandler) {
      this.checkpointErrorHandler = new CheckpointErrorHandler(
        { strategy },
        logger,
      );
    } else {
      this.checkpointErrorHandler.setStrategy(strategy);
    }

    logger.debug("Agent checkpoint error strategy updated", { strategy });
  }

  /**
   * Get current checkpoint error handling strategy
   */
  getCheckpointErrorStrategy(): CheckpointErrorStrategy {
    return this.checkpointErrorStrategy;
  }

  /**
   * Get checkpoint error handler (for advanced usage)
   */
  getCheckpointErrorHandler(): CheckpointErrorHandler | undefined {
    return this.checkpointErrorHandler;
  }
}
