import type { BaseCheckpoint, CheckpointMetadata, DeltaStorageConfig, CheckpointErrorStrategy, CheckpointErrorContext, CheckpointFormatVersion } from "@wf-agent/types";
import { CURRENT_CHECKPOINT_FORMAT_VERSION } from "@wf-agent/types";
import { DEFAULT_DELTA_STORAGE_CONFIG } from "../utils/constants.js";
import { BaseDiffCalculator } from "./base-diff-calculator.js";
import { BaseDeltaRestorer } from "./base-delta-restorer.js";
import type { CheckpointableEntity, CheckpointDependencies } from "../types.js";
export type { CheckpointDependencies } from "../types.js";
import { generateId } from "../../../utils/id-utils.js";
import { now } from "@wf-agent/common-utils";
import { CheckpointErrorHandler } from "../hierarchy/error-handler.js";
import { CheckpointVersionManager } from "../checkpoint-version-manager.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "BaseCheckpointCoordinator" });

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

  /** Checkpoint error handler for configurable error handling */
  protected checkpointErrorHandler?: CheckpointErrorHandler;

  /** Checkpoint error handling strategy */
  protected checkpointErrorStrategy: CheckpointErrorStrategy = "warn";

  /** Version manager for format compatibility and migration */
  protected versionManager: CheckpointVersionManager = new CheckpointVersionManager(logger);

  constructor(diffCalculator?: BaseDiffCalculator) {
    this.diffCalculator = diffCalculator ?? new BaseDiffCalculator();
  }

  async createCheckpoint(
    entity: TEntity,
    dependencies: CheckpointDependencies<TCheckpoint>,
    metadata?: CheckpointMetadata,
    context?: Record<string, unknown>,
  ): Promise<string> {
    const { saveCheckpoint, listCheckpoints, deltaConfig: depsDeltaConfig, getCheckpoint } = dependencies;
    const contextDeltaConfig = context?.['deltaConfig'] as DeltaStorageConfig | undefined;
    const config = { ...DEFAULT_DELTA_STORAGE_CONFIG, ...depsDeltaConfig, ...contextDeltaConfig };

    logger.debug("Creating checkpoint for entity", {
      entityId: entity.id,
    });

    const currentState = this.extractState(entity, context);

    const previousCheckpointIds = await listCheckpoints(entity.id);
    const checkpointCount = previousCheckpointIds.length;

    const checkpointType = this.determineCheckpointType(checkpointCount, config);

    const checkpointId = generateId();
    const timestamp = now();

    logger.debug("Checkpoint parameters determined", {
      checkpointId,
      type: checkpointType,
      previousCount: checkpointCount,
    });

    let previousCheckpoint: TCheckpoint | null = null;
    let chainPosition: number | undefined;
    
    if (checkpointCount > 0 && checkpointType === "DELTA") {
      const prevId = previousCheckpointIds[checkpointCount - 1];
      if (prevId) {
        previousCheckpoint = await getCheckpoint(prevId);
        if (previousCheckpoint?.type === "FULL") {
          chainPosition = 1;
        } else if (previousCheckpoint) {
          const effectiveInterval = Math.min(
            config.baselineInterval ?? 10,
            config.maxDeltaChainLength ?? 20,
          );
          let lastFullCount = 0;
          for (let idx = 0; idx < checkpointCount; idx++) {
            if (idx === 0 || idx % effectiveInterval === 0) {
              lastFullCount = idx;
            }
          }
          chainPosition = checkpointCount - lastFullCount;
        }
      }
    } else if (checkpointType === "FULL") {
      chainPosition = 0;
    }

    const checkpoint = await this.buildCheckpoint(
      entity,
      currentState,
      checkpointType,
      checkpointId,
      timestamp,
      previousCheckpointIds,
      dependencies,
      metadata,
      chainPosition,
    );

    const savedId = await saveCheckpoint(checkpoint);

    logger.info("Checkpoint created successfully", {
      checkpointId: savedId,
      entityId: entity.id,
      type: checkpointType,
    });

    return savedId;
  }

  async restoreFromCheckpoint(
    checkpointId: string,
    dependencies: CheckpointDependencies<TCheckpoint>,
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
  }

  protected determineCheckpointType(
    checkpointCount: number,
    config: DeltaStorageConfig,
  ): "FULL" | "DELTA" {
    if (!config.enabled) {
      return "FULL";
    }

    if (checkpointCount === 0) {
      return "FULL";
    }

    const baselineInterval = config.baselineInterval ?? 10;
    const maxDeltaChainLength = config.maxDeltaChainLength ?? 20;
    const effectiveInterval = Math.min(baselineInterval, maxDeltaChainLength);

    if (checkpointCount % effectiveInterval === 0) {
      return "FULL";
    }

    return "DELTA";
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

  // ============================================================
  // Checkpoint Error Handling
  // ============================================================

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

    logger.debug("Checkpoint error handling configured", {
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

    logger.debug("Checkpoint error strategy updated", { strategy });
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

  /**
   * Get version manager for compatibility checks and migrations
   */
  getVersionManager(): CheckpointVersionManager {
    return this.versionManager;
  }

  /**
   * Check if checkpoint needs version migration
   */
  needsVersionMigration(checkpoint: TCheckpoint): boolean {
    const formatVersion = (checkpoint.metadata?.customFields?.["formatVersion"] as CheckpointFormatVersion) || CURRENT_CHECKPOINT_FORMAT_VERSION;
    const compatibility = this.versionManager.checkCompatibility(formatVersion);
    return compatibility.requiresMigration;
  }
}
