import type { BaseCheckpoint, CheckpointMetadata, DeltaStorageConfig } from "@wf-agent/types";
import { BaseDiffCalculator } from "./base-diff-calculator.js";
import { BaseDeltaRestorer } from "./base-delta-restorer.js";
import type { CheckpointableEntity, CheckpointDependencies } from "../types.js";
export type { CheckpointDependencies } from "../types.js";
import { generateId } from "../../../utils/id-utils.js";
import { now } from "@wf-agent/common-utils";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import type { RestoreOptions } from "./base-state-manager.js";
import { DEFAULT_DELTA_STORAGE_CONFIG } from "../utils/constants.js";

const logger = createContextualLogger({ component: "BaseCheckpointCoordinator" });

interface LifecycleDecision {
  checkpointType: "FULL" | "DELTA";
  chainPosition: number;
  isBaseline: boolean;
}

function decideCheckpointType(
  checkpointCount: number,
  config: DeltaStorageConfig,
): LifecycleDecision {
  const effectiveConfig = { ...DEFAULT_DELTA_STORAGE_CONFIG, ...config };

  if (!effectiveConfig.enabled || checkpointCount === 0) {
    return {
      checkpointType: "FULL",
      chainPosition: 0,
      isBaseline: true,
    };
  }

  const baselineInterval = effectiveConfig.baselineInterval;
  const maxDeltaChainLength = effectiveConfig.maxDeltaChainLength;
  const effectiveInterval = Math.min(baselineInterval, maxDeltaChainLength);

  const isBaseline = checkpointCount % effectiveInterval === 0;
  const checkpointType = isBaseline ? "FULL" : "DELTA";
  const chainPosition = isBaseline ? 0 : checkpointCount % effectiveInterval || effectiveInterval;

  return { checkpointType, chainPosition, isBaseline };
}

/**
 * Options for checkpoint creation
 * Provides clear, type-safe configuration for the createCheckpoint operation
 * Replaces the vague Record<string, unknown> context parameter
 */
export interface CheckpointCreationOptions {
  /** Delta storage configuration (overrides dependencies.deltaConfig) */
  deltaConfig?: Partial<DeltaStorageConfig>;
  /** Metadata for the checkpoint */
  metadata?: CheckpointMetadata;
  /** Custom context data passed to extractState method */
  customContext?: Record<string, unknown>;
}

/**
 * Utility function to create a timeout promise
 */
function withTimeout<T>(
  promise: Promise<T>,
  timeoutMs: number,
): Promise<T> {
  return Promise.race([
    promise,
    new Promise<T>((_, reject) =>
      setTimeout(
        () => reject(new Error(`Restore operation timeout after ${timeoutMs}ms`)),
        timeoutMs,
      ),
    ),
  ]);
}

export interface RestoreContext<TEntity, TState> {
  entity: TEntity;
  snapshot: TState;
  checkpoint: BaseCheckpoint<unknown, unknown>;
}

export abstract class BaseCheckpointCoordinator<
  TCheckpoint extends BaseCheckpoint<unknown, unknown>,
  TEntity extends CheckpointableEntity,
  TState,
> {
  protected diffCalculator: BaseDiffCalculator;

  constructor(diffCalculator?: BaseDiffCalculator) {
    this.diffCalculator = diffCalculator ?? new BaseDiffCalculator();
  }

  async createCheckpoint(
    entity: TEntity,
    dependencies: CheckpointDependencies<TCheckpoint>,
    options?: CheckpointCreationOptions,
  ): Promise<string> {
    const { saveCheckpoint, listCheckpoints, deltaConfig: depsDeltaConfig, getCheckpoint } = dependencies;
    const optionsDeltaConfig = options?.deltaConfig;

    const config = {
      ...DEFAULT_DELTA_STORAGE_CONFIG,
      ...depsDeltaConfig,
      ...optionsDeltaConfig,
    } as DeltaStorageConfig;

    logger.debug("Creating checkpoint for entity", {
      entityId: entity.id,
    });

    const currentState = this.extractState(entity, options?.customContext);
    const previousCheckpointIds = await listCheckpoints(entity.id);
    const checkpointCount = previousCheckpointIds.length;

    const decision = decideCheckpointType(checkpointCount, config);
    const { checkpointType, chainPosition } = decision;

    const checkpointId = generateId();
    const timestamp = now();

    logger.debug("Checkpoint parameters determined", {
      checkpointId,
      type: checkpointType,
      chainPosition,
      previousCount: checkpointCount,
    });

    let previousCheckpoint: TCheckpoint | null = null;
    if (checkpointCount > 0 && checkpointType === "DELTA") {
      const prevId = previousCheckpointIds[checkpointCount - 1];
      if (prevId) {
        previousCheckpoint = await getCheckpoint(prevId);
      }
    }

    // Build metadata using lifecycle decision
    const metadata = options?.metadata || {};
    const baseCheckpointId = decision.isBaseline
      ? undefined
      : this.findBaseCheckpointId(previousCheckpointIds, checkpointCount, config);

    const mergedMetadata: CheckpointMetadata & {
      checkpointType?: "FULL" | "DELTA";
      chainPosition?: number;
      isBaseline?: boolean;
      deltaConfig?: Required<DeltaStorageConfig>;
      previousCheckpointId?: string;
      baseCheckpointId?: string;
    } = {
      ...metadata,
      checkpointType: decision.checkpointType,
      chainPosition: decision.chainPosition,
      isBaseline: decision.isBaseline,
      deltaConfig: config as Required<DeltaStorageConfig>,
    };

    if (decision.checkpointType === "DELTA") {
      mergedMetadata.previousCheckpointId = previousCheckpoint?.id;
      mergedMetadata.baseCheckpointId = baseCheckpointId;
    }

    const checkpoint = await this.buildCheckpoint(
      entity,
      currentState,
      checkpointType,
      checkpointId,
      timestamp,
      previousCheckpointIds,
      dependencies,
      mergedMetadata,
      chainPosition,
    );

    const savedId = await saveCheckpoint(checkpoint);

    logger.info("Checkpoint created successfully", {
      checkpointId: savedId,
      entityId: entity.id,
      type: checkpointType,
      chainPosition,
    });

    return savedId;
  }

  private findBaseCheckpointId(
    previousCheckpointIds: string[],
    checkpointCount: number,
    config: Required<DeltaStorageConfig>,
  ): string | undefined {
    if (checkpointCount === 0) return undefined;

    const effectiveInterval = Math.min(
      config.baselineInterval,
      config.maxDeltaChainLength,
    );

    // Find the last FULL checkpoint
    let lastFullCount = 0;
    for (let idx = 0; idx < checkpointCount; idx++) {
      if (idx === 0 || idx % effectiveInterval === 0) {
        lastFullCount = idx;
      }
    }

    return previousCheckpointIds[lastFullCount] || previousCheckpointIds[0];
  }

  async restoreFromCheckpoint(
    checkpointId: string,
    dependencies: CheckpointDependencies<TCheckpoint>,
    options?: RestoreOptions,
  ): Promise<TEntity> {
    const mergedOptions = {
      timeoutMs: 30000,
      skipCorrupted: true,
      maxChainDepth: 1000,
      allowFallback: true,
      ...options,
    };

    try {
      // Try to restore with timeout protection
      if (mergedOptions.timeoutMs && mergedOptions.timeoutMs > 0) {
        return await withTimeout(
          this._doRestore(checkpointId, dependencies, mergedOptions),
          mergedOptions.timeoutMs,
        );
      } else {
        return await this._doRestore(checkpointId, dependencies, mergedOptions);
      }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);

      logger.error("Restore failed", {
        checkpointId,
        error: errorMessage,
        withFallback: mergedOptions.allowFallback,
      });

      // If fallback is allowed and this is not a timeout issue, try fallback
      if (mergedOptions.allowFallback && !errorMessage.includes("timeout")) {
        try {
          return await this._doRestoreWithFallback(checkpointId, dependencies, mergedOptions);
        } catch (fallbackError) {
          logger.error("Fallback restore also failed", {
            checkpointId,
            error: fallbackError instanceof Error ? fallbackError.message : String(fallbackError),
          });
          // Throw the original error if fallback fails
          throw error;
        }
      }

      throw error;
    }
  }

  private async _doRestore(
    checkpointId: string,
    dependencies: CheckpointDependencies<TCheckpoint>,
    options: Required<RestoreOptions>,
  ): Promise<TEntity> {
    const { getCheckpoint, getCheckpoints } = dependencies;

    logger.debug("Restoring from checkpoint", { checkpointId });

    const checkpoint = await getCheckpoint(checkpointId);
    if (!checkpoint) {
      throw new Error(`Checkpoint not found: ${checkpointId}`);
    }

    this.validateCheckpoint(checkpoint);

    const parentId = this.extractParentId(checkpoint);

    const restorer = new BaseDeltaRestorer<TCheckpoint, TState>(
      getCheckpoint,
      getCheckpoints,
    );

    try {
      const restoreResult = await restorer.restore(checkpointId);
      const { snapshot } = restoreResult;

      logger.debug("State restored from checkpoint", {
        checkpointId,
        parentId,
        snapshotType: typeof snapshot,
      });

      const entity = this.createEntityFromSnapshot(parentId, snapshot);

      logger.info("Entity restored successfully", {
        entityId: entity.id,
        checkpointId,
      });

      return entity;
    } catch (error) {
      if (options.skipCorrupted) {
        logger.warn("Checkpoint restoration failed, attempting skip recovery", {
          checkpointId,
          error: error instanceof Error ? error.message : String(error),
        });
        // If skipCorrupted is enabled and this is a corruption error,
        // try to use just the snapshot without delta chain
        if (checkpoint.snapshot) {
          logger.info("Falling back to snapshot-only restoration (skipping deltas)", {
            checkpointId,
          });
          const entity = this.createEntityFromSnapshot(parentId, checkpoint.snapshot as TState);
          return entity;
        }
      }

      throw error;
    }
  }

  private async _doRestoreWithFallback(
    checkpointId: string,
    dependencies: CheckpointDependencies<TCheckpoint>,
    options: Required<RestoreOptions>,
  ): Promise<TEntity> {
    const { listCheckpoints, getCheckpoint } = dependencies;

    logger.info("Attempting fallback restore strategy", { checkpointId });

    // Get all checkpoints for this entity to find nearest FULL checkpoint
    const parentIdGuess = "unknown"; // We don't know the parent yet
    const allCheckpointIds = await listCheckpoints(parentIdGuess);

    if (allCheckpointIds.length === 0) {
      throw new Error("No fallback checkpoints available");
    }

    // Find FULL checkpoints sorted by recency
    const fullCheckpoints: TCheckpoint[] = [];

    for (const id of allCheckpointIds) {
      if (fullCheckpoints.length >= options.maxChainDepth) {
        break;
      }

      try {
        const cp = await getCheckpoint(id);
        if (cp && cp.type === "FULL") {
          fullCheckpoints.push(cp);
        }
      } catch (error) {
        logger.warn("Failed to load checkpoint during fallback search", { id });
        continue;
      }
    }

    if (fullCheckpoints.length === 0) {
      throw new Error("No FULL checkpoints found for fallback");
    }

    // Sort by timestamp descending and try each one
    fullCheckpoints.sort((a, b) => (b.timestamp || 0) - (a.timestamp || 0));

    let lastError: Error | undefined;

    for (const fallbackCp of fullCheckpoints) {
      try {
        logger.info("Trying fallback checkpoint", { fallbackCpId: fallbackCp.id });

        // Don't use timeout for fallback attempts since we're already recovering
        return await this._doRestore(fallbackCp.id as string, dependencies, {
          ...options,
          timeoutMs: 0,
          allowFallback: false, // No nested fallback
        });
      } catch (error) {
        lastError = error instanceof Error ? error : new Error(String(error));
        logger.warn("Fallback checkpoint also failed", {
          fallbackCpId: fallbackCp.id,
          error: lastError.message,
        });
        // Continue to next fallback checkpoint
        continue;
      }
    }

    throw lastError || new Error("All fallback restore attempts failed");
  }

  /**
   * Determine checkpoint type based on count and delta config
   */
  protected determineCheckpointType(
    checkpointCount: number,
    config: DeltaStorageConfig,
  ): "FULL" | "DELTA" {
    return decideCheckpointType(checkpointCount, config).checkpointType;
  }

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

  protected abstract extractState(entity: TEntity, context?: Record<string, unknown>): TState;

  protected abstract buildCheckpoint(
    entity: TEntity,
    currentState: TState,
    checkpointType: "FULL" | "DELTA",
    checkpointId: string,
    timestamp: number,
    previousCheckpointIds: string[],
    dependencies: CheckpointDependencies<TCheckpoint>,
    metadata?: CheckpointMetadata,
    chainPosition?: number,
  ): Promise<TCheckpoint>;

  protected abstract extractParentId(checkpoint: TCheckpoint): string;

  protected abstract createEntityFromSnapshot(
    parentId: string,
    snapshot: TState,
  ): TEntity;
}
