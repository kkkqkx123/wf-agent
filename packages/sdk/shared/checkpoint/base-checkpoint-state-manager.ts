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
  CheckpointStorageRecord,
  CheckpointStorageMetadata,
  CheckpointInfo,
  BaseEvent,
} from "@wf-agent/types";
import type { EventRegistry } from "../registry/event-registry.js";
import { StateCodec } from "@wf-agent/common-utils";
import { BaseDiffCalculator } from "./base-diff-calculator.js";
import type { DeltaMap } from "./base-diff-calculator.js";
import { createCleanupStrategy } from "./utils/cleanup-policy.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import type { CheckpointStorageAdapter } from "./types.js";
import { CheckpointMetricsCollector } from "./checkpoint-metrics-collector.js";
import type { CheckpointMetricsConfig } from "@wf-agent/types";

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
  private diffCalculator: BaseDiffCalculator = new BaseDiffCalculator();
  private metricsCollector?: CheckpointMetricsCollector;

  constructor(
    storageAdapter: CheckpointStorageAdapter,
    eventManager?: EventRegistry,
    cleanupPolicy?: CleanupPolicy,
    metricsConfig?: CheckpointMetricsConfig,
  ) {
    this.storageAdapter = storageAdapter;
    this.eventManager = eventManager;
    this.cleanupPolicy = cleanupPolicy;
    this.codec = new StateCodec();
    if (metricsConfig?.enabled) {
      this.metricsCollector = new CheckpointMetricsCollector(metricsConfig, logger);
    }
  }

  /**
   * Create a checkpoint
   * @param checkpoint The checkpoint to save
   * @returns The checkpoint ID
   */
  async create(checkpoint: TCheckpoint): Promise<string> {
    const startTime = performance.now();
    const entityId = (checkpoint as unknown as Record<string, string>)['executionId'] || (checkpoint as unknown as Record<string, string>)['agentLoopId'] || "unknown";

    try {
      logger.debug("Creating checkpoint", {
        checkpointId: checkpoint.id,
        type: checkpoint.type,
      });

      const serializedData = await this.codec.serialize(checkpoint);
      const metadata = this.extractStorageMetadata(checkpoint);
      metadata.blobSize = serializedData.length;

      await this.storageAdapter.save(checkpoint.id, serializedData, metadata);

      // Track size in memory for immediate cleanup decisions
      this.checkpointSizes.set(checkpoint.id, serializedData.length);

      // Emit created event within a batch (implemented by subclass)
      if (this.eventManager) {
        const emitter = this.eventManager.getEmitter(
          (checkpoint as unknown as Record<string, string>)['executionId'] || "unknown",
        );
        emitter.beginBatch();
        try {
          const createdEvent = this.buildCreatedEvent(checkpoint);
          await this.eventManager.emit(createdEvent as BaseEvent);
        } finally {
          await emitter.endBatch();
        }
      }

      const duration = performance.now() - startTime;

      // Record creation metrics
      this.metricsCollector?.recordCreation({
        checkpointId: checkpoint.id,
        entityId,
        type: checkpoint.type as "FULL" | "DELTA",
        duration,
        size: serializedData.length,
        timestamp: Date.now(),
        success: true,
      });

      // Record chain length metric
      this.recordChainLength(entityId);

      logger.info("Checkpoint created", {
        checkpointId: checkpoint.id,
        dataSize: serializedData.length,
        duration: `${duration.toFixed(2)}ms`,
      });

      return checkpoint.id;
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      logger.error("Failed to create checkpoint", {
        checkpointId: checkpoint.id,
        error: errorMessage,
      });

      // Record failed creation metrics
      this.metricsCollector?.recordCreation({
        checkpointId: checkpoint.id,
        entityId,
        type: checkpoint.type as "FULL" | "DELTA",
        duration: performance.now() - startTime,
        size: 0,
        timestamp: Date.now(),
        success: false,
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
   * Record chain length metric for an entity
   */
  private async recordChainLength(entityId: string): Promise<void> {
    if (!this.metricsCollector) return;

    try {
      const checkpoints = await this.storageAdapter.listByEntityWithMetadata(entityId, "");
      const fullCount = checkpoints.filter(c => c.metadata.checkpointType === "FULL").length;
      const deltaCount = checkpoints.filter(c => c.metadata.checkpointType === "DELTA").length;

      this.metricsCollector.recordChainLength({
        entityId,
        chainLength: checkpoints.length,
        fullCount,
        deltaCount,
        timestamp: Date.now(),
      });
    } catch {
      // Don't fail the main operation if metrics recording fails
    }
  }

  /**
   * Get a checkpoint by ID
   * @param checkpointId Checkpoint ID
   * @returns The checkpoint or null if not found
   */
  async get(checkpointId: string): Promise<TCheckpoint | null> {
    const startTime = performance.now();

    // Step 1: Load raw data from storage
    let data: Uint8Array | null;
    try {
      data = await this.storageAdapter.load(checkpointId);
    } catch (error) {
      logger.error("Storage error while loading checkpoint", {
        checkpointId,
        error: error instanceof Error ? error.message : String(error),
      });

      // Record failed load metrics
      this.metricsCollector?.recordLoad({
        checkpointId,
        entityId: "unknown",
        duration: performance.now() - startTime,
        timestamp: Date.now(),
        success: false,
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

      const entityId = (checkpoint as unknown as Record<string, string>)['executionId'] || (checkpoint as unknown as Record<string, string>)['agentLoopId'] || "unknown";

      // Record successful load metrics
      this.metricsCollector?.recordLoad({
        checkpointId,
        entityId,
        duration: performance.now() - startTime,
        timestamp: Date.now(),
        success: true,
      });

      return checkpoint;
    } catch (error) {
      logger.error("Failed to deserialize checkpoint (data may be corrupted)", {
        checkpointId,
        size: data.length,
        error: error instanceof Error ? error.message : String(error),
      });

      // Record failed load metrics
      this.metricsCollector?.recordLoad({
        checkpointId,
        entityId: "unknown",
        duration: performance.now() - startTime,
        timestamp: Date.now(),
        success: false,
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
   * Batch load checkpoints for efficient delta chain reconstruction
   * Uses the storage adapter's loadBatch for N+1 to single query reduction
   *
   * @param ids Array of checkpoint IDs to load
   * @returns Map of checkpoint ID to checkpoint (null if not found)
   */
  async getCheckpoints(ids: string[]): Promise<Map<string, TCheckpoint | null>> {
    if (ids.length === 0) {
      return new Map();
    }

    const startTime = performance.now();
    const results = await this.storageAdapter.loadBatch(ids);
    const map = new Map<string, TCheckpoint | null>();
    let successCount = 0;
    let failCount = 0;

    for (const { id, data } of results) {
      if (data) {
        try {
          const checkpoint = await this.codec.deserialize<TCheckpoint>(data);
          this.checkpointSizes.set(id, data.length);
          map.set(id, checkpoint);
          successCount++;
        } catch {
          logger.warn("Failed to deserialize checkpoint in batch load", { id });
          map.set(id, null);
          failCount++;
        }
      } else {
        map.set(id, null);
      }
    }

    // Record batch load metrics
    if (ids.length > 0) {
      const entityId = "unknown";
      this.metricsCollector?.recordLoad({
        checkpointId: `batch_${ids.length}`,
        entityId,
        duration: performance.now() - startTime,
        timestamp: Date.now(),
        success: failCount === 0,
        error: failCount > 0 ? `${failCount} failures` : undefined,
      });
    }

    return map;
  }

  /**
   * Execute cleanup policy for a specific entity
   * Optimized to only scan and clean checkpoints belonging to the specified entity
   * Supports incremental cleanup via watermark tracking — only processes checkpoints
   * created since the last cleanup run. Periodic full scan to correct drift.
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
    const startTime = performance.now();
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
    let checkpointInfoArray = await this.storageAdapter.listByEntityWithMetadata(
      entityId,
      entityType,
    );

    // Incremental cleanup: check watermark to skip already-evaluated checkpoints.
    // Every 10th cleanup does a full scan to correct drift.
    const entityMeta = await this.storageAdapter.getEntityMetadata(entityType, entityId);
    const lastWatermark = entityMeta?.['cleanupWatermark'] as number | undefined;
    const cleanupRunCount = (entityMeta?.['cleanupRunCount'] as number) || 0;
    const isFullScan = cleanupRunCount % 10 === 0;

    if (lastWatermark !== undefined && !isFullScan) {
      const beforeCount = checkpointInfoArray.length;
      checkpointInfoArray = checkpointInfoArray.filter(
        cp => cp.metadata.timestamp > lastWatermark || cp.id === excludeCheckpointId,
      );
      logger.debug("Incremental cleanup: filtered by watermark", {
        entityId,
        entityType,
        beforeCount,
        afterCount: checkpointInfoArray.length,
        lastWatermark,
      });
    }

    // Update checkpoint sizes from top-level metadata blobSize if available
    for (const info of checkpointInfoArray) {
      const metadata = info.metadata as CheckpointStorageMetadata;
      const size = metadata.blobSize ?? 0;
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

    // Persist cleanup watermark for incremental cleanup
    const remainingCheckpoints = checkpointInfoArray.filter(
      cp => !toDeleteIds.includes(cp.id),
    );
    if (remainingCheckpoints.length > 0) {
      const maxTimestamp = Math.max(
        ...remainingCheckpoints.map(cp => cp.metadata.timestamp),
      );
      await this.storageAdapter.setEntityMetadata(entityType, entityId, {
        cleanupWatermark: maxTimestamp,
        cleanupRunCount: cleanupRunCount + 1,
      });
    }

    const duration = performance.now() - startTime;

    // Record cleanup metrics
    this.metricsCollector?.recordCleanup({
      entityId,
      count: toDeleteIds.length,
      sizeFreed: freedSpaceBytes,
      duration,
      timestamp: Date.now(),
      success: true,
    });

    return {
      deletedCheckpointIds: toDeleteIds,
      deletedCount: toDeleteIds.length,
      freedSpaceBytes,
      remainingCount,
    };
  }

  /**
   * Compact a delta chain by merging multiple consecutive deltas in batch.
   *
   * For a chain like [FULL: A] ← [DELTA: B] ← [DELTA: C] ← [DELTA: D],
   * this merges C and D into B in a single operation: B's delta becomes
   * the combination of B, C, and D, the chain is shortened by 2.
   *
   * This is more efficient than merging only two deltas at a time,
   * especially for long delta chains.
   *
   * @param entityId The entity ID
   * @param entityType The entity type
   * @param batchSize Number of deltas to merge per batch (default: merge all consecutive)
   * @returns Number of checkpoints deleted
   */
  async compactDeltaChain(
    entityId: string,
    entityType: string,
    batchSize?: number,
  ): Promise<number> {
    const checkpoints = await this.storageAdapter.listByEntityWithMetadata(entityId, entityType);

    const deltas = checkpoints
      .filter(cp => cp.metadata.checkpointType === "DELTA")
      .sort((a, b) => a.metadata.timestamp - b.metadata.timestamp);

    if (deltas.length < 2) return 0;

    // Build chains: find all chains of consecutive deltas
    const chains: Array<Array<{ idx: number; cp: typeof deltas[0] }>> = [];
    const processed = new Set<string>();

    for (let i = 0; i < deltas.length; i++) {
      if (processed.has(deltas[i]!.id)) continue;

      const chain: Array<{ idx: number; cp: typeof deltas[0] }> = [];
      let currentIdx = i;

      while (currentIdx < deltas.length && !processed.has(deltas[currentIdx]!.id)) {
        const cp = deltas[currentIdx]!;
        const prevId = cp.metadata.previousCheckpointId;

        // Verify chain continuity: either first in chain or links to previous
        if (chain.length === 0 || prevId === chain[chain.length - 1]!.cp.id) {
          chain.push({ idx: currentIdx, cp });
          processed.add(cp.id);
          // Find next delta that references this one as previous
          const nextIdx = deltas.findIndex(
            (d, j) => j > currentIdx && d.metadata.previousCheckpointId === cp.id && !processed.has(d.id),
          );
          if (nextIdx === -1) break;
          currentIdx = nextIdx;
        } else {
          break;
        }
      }

      if (chain.length >= 2) {
        chains.push(chain);
      }
    }

    if (chains.length === 0) return 0;

    let deletedCount = 0;

    // Process each chain
    for (const chain of chains) {
      const effectiveBatchSize = batchSize ?? chain.length - 1;
      const mergeCount = Math.min(effectiveBatchSize, chain.length - 1);

      if (mergeCount < 1) continue;

      const anchor = chain[0]!.cp;
      const toMerge = chain.slice(1, mergeCount + 1);
      const lastMerged = toMerge[toMerge.length - 1]!;

      // Load all deltas to merge
      const idsToLoad = [anchor.id, ...toMerge.map(c => c.cp.id)];
      const rawDataList = await Promise.all(
        idsToLoad.map(id => this.storageAdapter.load(id)),
      );

      if (rawDataList.some(d => !d === null)) {
        logger.warn("Cannot compact delta chain: failed to load checkpoint data", {
          anchorId: anchor.id,
          mergeIds: toMerge.map(c => c.cp.id),
        });
        continue;
      }

      const cps = await Promise.all(
        rawDataList.map(d => this.codec.deserialize<TCheckpoint>(d!)),
      );

      if (cps.some(cp => !cp.delta)) {
        logger.warn("Cannot compact delta chain: checkpoint missing delta data", {
          anchorId: anchor.id,
        });
        continue;
      }

      // Merge all deltas into anchor
      let mergedDelta = cps[0]!.delta as unknown as DeltaMap;
      for (let i = 1; i < cps.length; i++) {
        mergedDelta = this.diffCalculator.mergeDeltas(
          mergedDelta,
          cps[i]!.delta as unknown as DeltaMap,
        );
      }
      cps[0]!.delta = mergedDelta as unknown as TCheckpoint["delta"];

      // Save updated anchor checkpoint
      const updatedData = await this.codec.serialize(cps[0]!);
      const updatedMetadata = this.extractStorageMetadata(cps[0]!);
      updatedMetadata.blobSize = updatedData.length;
      await this.storageAdapter.save(anchor.id, updatedData, updatedMetadata);
      this.checkpointSizes.set(anchor.id, updatedData.length);

      // Find and update successor of the last merged delta
      const successor = checkpoints.find(
        cp => cp.metadata.previousCheckpointId === lastMerged.cp.id && cp.id !== anchor.id,
      );

      if (successor) {
        const successorRaw = await this.storageAdapter.load(successor.id);
        if (successorRaw) {
          const successorCp = await this.codec.deserialize<TCheckpoint>(successorRaw);
          successorCp.previousCheckpointId = anchor.id;
          const updatedSuccData = await this.codec.serialize(successorCp);
          const updatedSuccMetadata = this.extractStorageMetadata(successorCp);
          updatedSuccMetadata.blobSize = updatedSuccData.length;
          await this.storageAdapter.save(successor.id, updatedSuccData, updatedSuccMetadata);
          this.checkpointSizes.set(successor.id, updatedSuccData.length);
        }
      }

      // Delete all merged checkpoints
      for (const { cp } of toMerge) {
        await this.storageAdapter.delete(cp.id);
        this.checkpointSizes.delete(cp.id);
        deletedCount++;
      }

      logger.info("Delta chain compacted", {
        anchorId: anchor.id,
        mergedCount: toMerge.length,
        deletedIds: toMerge.map(c => c.cp.id),
        successorId: successor?.id ?? null,
      });
    }

    return deletedCount;
  }

  /**
   * Get the metrics collector (if enabled)
   */
  getMetricsCollector(): CheckpointMetricsCollector | undefined {
    return this.metricsCollector;
  }

  /**
   * Initialize the state manager
   */
  async initialize(): Promise<void> {
    logger.info("Initializing checkpoint state manager");
    await this.storageAdapter.initialize();
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
  protected abstract extractStorageMetadata(checkpoint: TCheckpoint): CheckpointStorageRecord;

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
