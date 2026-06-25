/**
 * Base Checkpoint Coordinator
 *
 * Coordinates the entire checkpoint lifecycle using template method pattern.
 * Subclasses implement entity-specific logic while reusing common flow.
 */

import type { BaseCheckpoint, CheckpointMetadata, DeltaStorageConfig } from "@wf-agent/types";
import { DEFAULT_DELTA_STORAGE_CONFIG } from "./utils/constants.js";
import { BaseDiffCalculator } from "./base-diff-calculator.js";
import { BaseDeltaRestorer } from "./base-delta-restorer.js";
import type { CheckpointableEntity, CheckpointDependencies } from "./types.js";
export type { CheckpointDependencies } from "./types.js";
import { generateId } from "../../utils/id-utils.js";
import { now } from "@wf-agent/common-utils";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "BaseCheckpointCoordinator" });

/**
 * Context for checkpoint restoration operations.
 * Passed through the restoration flow to avoid instance-level state.
 */
export interface RestoreContext<TEntity, TState> {
  entity: TEntity;
  snapshot: TState;
  checkpoint: BaseCheckpoint<unknown, unknown>;
}

/**
 * Base Checkpoint Coordinator
 *
 * Coordinates the entire checkpoint lifecycle using template method pattern.
 * Subclasses implement entity-specific logic while reusing common flow.
 *
 * Design Philosophy:
 * - TEntity only needs to have an 'id' property (CheckpointableEntity)
 * - State extraction and entity creation are delegated to subclasses
 * - All common checkpoint logic is centralized here
 *
 * @template TCheckpoint - The specific checkpoint type (e.g., AgentLoopCheckpoint, Checkpoint)
 * @template TEntity - Any entity with an 'id' property
 * @template TState - The state snapshot type
 */
export abstract class BaseCheckpointCoordinator<
  TCheckpoint extends BaseCheckpoint<unknown, unknown>,
  TEntity extends CheckpointableEntity,
  TState,
> {
  protected diffCalculator: BaseDiffCalculator;

  constructor(diffCalculator?: BaseDiffCalculator) {
    this.diffCalculator = diffCalculator ?? new BaseDiffCalculator();
  }

  /**
   * Create a checkpoint
   *
   * Template method that defines the common checkpoint creation flow:
   * 1. Extract current state from entity
   * 2. Retrieve previous checkpoints
   * 3. Determine checkpoint type (FULL vs DELTA)
   * 4. Build checkpoint object
   * 5. Save checkpoint
   *
   * @param entity The entity to checkpoint
   * @param dependencies Checkpoint dependencies
   * @param metadata Optional checkpoint metadata
   * @returns Checkpoint ID
   */
  async createCheckpoint(
    entity: TEntity,
    dependencies: CheckpointDependencies<TCheckpoint>,
    metadata?: CheckpointMetadata,
    context?: Record<string, unknown>,
  ): Promise<string> {
    const { saveCheckpoint, listCheckpoints, deltaConfig } = dependencies;
    const config = { ...DEFAULT_DELTA_STORAGE_CONFIG, ...deltaConfig };

    logger.debug("Creating checkpoint for entity", {
      entityId: entity.id,
    });

    // Step 1: Extract current state (delegated to subclass)
    const currentState = this.extractState(entity, context);

    // Step 2: Retrieve previous checkpoints
    const previousCheckpointIds = await listCheckpoints(entity.id);
    const checkpointCount = previousCheckpointIds.length;

    // Step 3: Determine checkpoint type
    const checkpointType = this.determineCheckpointType(checkpointCount, config);

    // Step 4: Generate checkpoint ID and timestamp
    const checkpointId = generateId();
    const timestamp = now();

    logger.debug("Checkpoint parameters determined", {
      checkpointId,
      type: checkpointType,
      previousCount: checkpointCount,
    });

    // Step 5: Build checkpoint (delegated to subclass)
    const checkpoint = await this.buildCheckpoint(
      entity,
      currentState,
      checkpointType,
      checkpointId,
      timestamp,
      previousCheckpointIds,
      dependencies,
      metadata,
    );

    // Step 6: Save checkpoint
    const savedId = await saveCheckpoint(checkpoint);

    logger.info("Checkpoint created successfully", {
      checkpointId: savedId,
      entityId: entity.id,
      type: checkpointType,
    });

    return savedId;
  }

  /**
   * Restore entity from checkpoint
   *
   * Template method that defines the common restoration flow:
   * 1. Load checkpoint
   * 2. Validate checkpoint integrity
   * 3. Restore full state (handling delta chains)
   * 4. Create entity from restored state
   *
   * @param checkpointId The checkpoint ID to restore from
   * @param dependencies Checkpoint dependencies
   * @returns Restored entity
   */
  async restoreFromCheckpoint(
    checkpointId: string,
    dependencies: CheckpointDependencies<TCheckpoint>,
  ): Promise<TEntity> {
    const { getCheckpoint, getCheckpoints } = dependencies;

    logger.debug("Restoring from checkpoint", { checkpointId });

    // Step 1: Load checkpoint
    const checkpoint = await getCheckpoint(checkpointId);
    if (!checkpoint) {
      throw new Error(`Checkpoint not found: ${checkpointId}`);
    }

    // Step 2: Validate checkpoint integrity
    this.validateCheckpoint(checkpoint);

    // Step 3: Determine parent entity ID for restoration
    const parentId = this.extractParentId(checkpoint);

    // Step 4: Restore full state (handles delta chains automatically)
    // Use batch loader for efficient chain reconstruction when available
    const restorer = new BaseDeltaRestorer<TCheckpoint, TState>(
      getCheckpoint,
      getCheckpoints,
    );
    const restoreResult = await restorer.restore(checkpointId);
    const { snapshot } = restoreResult;

    logger.debug("State restored from checkpoint", {
      checkpointId,
      parentId,
      snapshotType: typeof snapshot,
    });

    // Step 5: Create entity from snapshot (delegated to subclass)
    const entity = this.createEntityFromSnapshot(parentId, snapshot);

    logger.info("Entity restored successfully", {
      entityId: entity.id,
      checkpointId,
    });

    return entity;
  }

  /**
   * Determine checkpoint type based on configuration and history
   *
   * This method can be overridden by subclasses to implement custom strategies.
   * Default implementation: Creates FULL checkpoint at the effective interval,
   * which is the minimum of baselineInterval and maxDeltaChainLength.
   * This ensures the delta chain never exceeds the configured maximum length.
   *
   * @param checkpointCount Number of existing checkpoints
   * @param config Delta storage configuration
   * @returns Checkpoint type (FULL or DELTA)
   */
  protected determineCheckpointType(
    checkpointCount: number,
    config: DeltaStorageConfig,
  ): "FULL" | "DELTA" {
    // If incremental storage is disabled, always create full checkpoint
    if (!config.enabled) {
      return "FULL";
    }

    // First checkpoint must be full
    if (checkpointCount === 0) {
      return "FULL";
    }

    // Determine the effective baseline interval:
    // - baselineInterval defines the normal FULL checkpoint cadence
    // - maxDeltaChainLength is a hard limit on delta chain length
    // The effective interval is the smaller of the two, ensuring
    // the delta chain never exceeds maxDeltaChainLength.
    const baselineInterval = config.baselineInterval ?? 10;
    const maxDeltaChainLength = config.maxDeltaChainLength ?? 20;
    const effectiveInterval = Math.min(baselineInterval, maxDeltaChainLength);

    // Create baseline when checkpointCount is divisible by effectiveInterval
    if (checkpointCount % effectiveInterval === 0) {
      return "FULL";
    }

    // Otherwise create delta checkpoint
    return "DELTA";
  }

  /**
   * Validate checkpoint integrity
   * @param checkpoint The checkpoint to validate
   * @throws Error if validation fails
   */
  protected validateCheckpoint(checkpoint: TCheckpoint): void {
    if (!checkpoint.id) {
      throw new Error("Checkpoint ID is required");
    }

    if (checkpoint.type === "DELTA") {
      if (!checkpoint.baseCheckpointId) {
        throw new Error("Delta checkpoint requires baseCheckpointId");
      }
      if (!checkpoint.previousCheckpointId) {
        throw new Error("Delta checkpoint requires previousCheckpointId");
      }
      if (!checkpoint.delta) {
        throw new Error("Delta checkpoint requires delta data");
      }
    } else if (checkpoint.type === "FULL") {
      if (!checkpoint.snapshot) {
        throw new Error("Full checkpoint requires snapshot");
      }
    }
  }

  // ============================================================================
  // Abstract Methods - Must be implemented by subclasses
  // ============================================================================

  /**
   * Extract state from entity
   * @param entity The entity
   * @param context Optional context passed from createCheckpoint (e.g., conversationManager)
   * @returns State snapshot
   */
  protected abstract extractState(entity: TEntity, context?: Record<string, unknown>): TState;

  /**
   * Build checkpoint object
   * @param entity The entity
   * @param currentState Current state snapshot
   * @param checkpointType Checkpoint type
   * @param checkpointId Generated checkpoint ID
   * @param timestamp Timestamp
   * @param previousCheckpointIds IDs of previous checkpoints
   * @param dependencies Checkpoint dependencies
   * @param metadata Optional metadata
   * @returns Complete checkpoint object
   */
  protected abstract buildCheckpoint(
    entity: TEntity,
    currentState: TState,
    checkpointType: "FULL" | "DELTA",
    checkpointId: string,
    timestamp: number,
    previousCheckpointIds: string[],
    dependencies: CheckpointDependencies<TCheckpoint>,
    metadata?: CheckpointMetadata,
  ): Promise<TCheckpoint>;

  /**
   * Extract parent ID from checkpoint
   * @param checkpoint The checkpoint
   * @returns Parent entity ID
   */
  protected abstract extractParentId(checkpoint: TCheckpoint): string;

  /**
   * Create entity from restored state snapshot
   * @param parentId Parent entity ID
   * @param snapshot Restored state snapshot
   * @returns Reconstructed entity
   */
  protected abstract createEntityFromSnapshot(
    parentId: string,
    snapshot: TState,
  ): TEntity;
}
