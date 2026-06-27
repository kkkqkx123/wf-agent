import type {
  BaseCheckpoint,
  CleanupPolicy,
  CleanupResult,
  CheckpointStorageMetadata,
  CheckpointInfo,
  BaseEvent,
} from "@wf-agent/types";
import type { EventRegistry } from "../../registry/event-registry.js";
import { StateCodec } from "@wf-agent/common-utils";
import { BaseDiffCalculator } from "./base-diff-calculator.js";
import type { DeltaMap } from "./base-diff-calculator.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import type { CheckpointStorageAdapter } from "../types.js";
import { CheckpointMetricsCollector } from "./metrics-collector.js";
import type { CheckpointMetricsConfig } from "@wf-agent/types";
import { createCleanupStrategy } from "../utils/cleanup-policy.js";
import { buildDependencyGraph, computeProtectedCheckpoints } from "../checkpoint-graph.js";

const logger = createContextualLogger({ component: "BaseCheckpointStateManager" });

/**
 * Interface to extract entity ID from checkpoint in a type-safe way
 * Replaces unsafe 'as unknown as Record<string, string>' pattern
 */
export interface ICheckpointWithEntity {
  /**
   * Extract the entity ID from checkpoint
   * Returns the identifier of the entity that owns this checkpoint
   */
  getEntityId(): string;
}

/**
 * Function type for extracting entity ID from a checkpoint
 * Provides abstraction over field names that vary by checkpoint type
 *
 * Examples:
 * - Workflow checkpoints: extract 'executionId'
 * - Agent checkpoints: extract 'agentLoopId'
 */
export type EntityIdExtractor<TCheckpoint> = (checkpoint: TCheckpoint) => string;

/**
 * Retry policy configuration for checkpoint operations
 * Implements exponential backoff strategy
 */
export interface RetryPolicy {
  /** Maximum number of retry attempts (default: 3) */
  maxRetries: number;
  /** Initial delay in milliseconds (default: 100) */
  initialDelayMs: number;
  /** Maximum delay in milliseconds (default: 5000) */
  maxDelayMs: number;
  /** Backoff multiplier for exponential increase (default: 2) */
  backoffMultiplier: number;
  /** Whether to add jitter to delays (default: true) */
  useJitter: boolean;
}

/**
 * Restore operation options with timeout and fallback support
 */
export interface RestoreOptions {
  /** Timeout in milliseconds for restore operation (default: 30000 = 30s) */
  timeoutMs?: number;
  /** Whether to skip corrupted checkpoints in delta chain (default: true) */
  skipCorrupted?: boolean;
  /** Maximum depth for delta chain traversal (default: 1000) */
  maxChainDepth?: number;
  /** Whether to fallback to nearest FULL checkpoint on error (default: true) */
  allowFallback?: boolean;
}

const DEFAULT_RETRY_POLICY: RetryPolicy = {
  maxRetries: 3,
  initialDelayMs: 100,
  maxDelayMs: 5000,
  backoffMultiplier: 2,
  useJitter: true,
};

export const DEFAULT_RESTORE_OPTIONS: RestoreOptions = {
  timeoutMs: 30000,
  skipCorrupted: true,
  maxChainDepth: 1000,
  allowFallback: true,
};

/**
 * Utility function for exponential backoff with jitter
 */
function calculateBackoffDelay(
  attempt: number,
  policy: RetryPolicy,
): number {
  const exponentialDelay = Math.min(
    policy.initialDelayMs * Math.pow(policy.backoffMultiplier, attempt),
    policy.maxDelayMs,
  );

  if (!policy.useJitter) {
    return exponentialDelay;
  }

  // Add random jitter: ±20% of the calculated delay
  const jitterAmount = exponentialDelay * 0.2;
  const jitter = (Math.random() - 0.5) * 2 * jitterAmount;
  return Math.max(1, exponentialDelay + jitter);
}



export abstract class BaseCheckpointStateManager<
  TCheckpoint extends BaseCheckpoint<unknown, unknown>,
> {
  protected storageAdapter: CheckpointStorageAdapter;
  protected eventManager?: EventRegistry;
  protected cleanupPolicy?: CleanupPolicy;
  protected codec: StateCodec;
  protected checkpointSizes: Map<string, number> = new Map();
  protected extractEntityId: EntityIdExtractor<TCheckpoint>;
  private diffCalculator: BaseDiffCalculator = new BaseDiffCalculator();
  private metricsCollector?: CheckpointMetricsCollector;
  private cleanupLocks = new Map<string, Promise<unknown>>();

  constructor(
    storageAdapter: CheckpointStorageAdapter,
    eventManager?: EventRegistry,
    cleanupPolicy?: CleanupPolicy,
    metricsConfig?: CheckpointMetricsConfig,
    extractEntityId?: EntityIdExtractor<TCheckpoint>,
  ) {
    this.storageAdapter = storageAdapter;
    this.eventManager = eventManager;
    this.cleanupPolicy = cleanupPolicy;
    this.codec = new StateCodec();
    this.extractEntityId = extractEntityId ?? this.defaultExtractEntityId.bind(this);
    if (metricsConfig?.enabled) {
      this.metricsCollector = new CheckpointMetricsCollector(metricsConfig, logger);
    }
  }

  /**
   * Default entity ID extractor - tries common field names
   * Subclasses should override extractEntityId in constructor if needed
   */
  protected defaultExtractEntityId(checkpoint: TCheckpoint): string {
    const record = checkpoint as unknown as Record<string, unknown>;
    // Try common field names in priority order
    if (typeof record['executionId'] === 'string') {
      return record['executionId'];
    }
    if (typeof record['agentLoopId'] === 'string') {
      return record['agentLoopId'];
    }
    return "unknown";
  }

  async create(checkpoint: TCheckpoint, retryPolicy?: Partial<RetryPolicy>): Promise<string> {
    const policy = { ...DEFAULT_RETRY_POLICY, ...retryPolicy };
    let lastError: Error | undefined;

    for (let attempt = 0; attempt < policy.maxRetries; attempt++) {
      try {
        return await this._doCreate(checkpoint);
      } catch (error) {
        lastError = error instanceof Error ? error : new Error(String(error));

        if (attempt === policy.maxRetries - 1) {
          // Last attempt failed, throw error
          throw lastError;
        }

        // Calculate backoff and retry
        const delayMs = calculateBackoffDelay(attempt, policy);
        logger.warn("Checkpoint creation failed, retrying", {
          checkpointId: checkpoint.id,
          attempt: attempt + 1,
          maxRetries: policy.maxRetries,
          nextRetryInMs: delayMs,
          error: lastError.message,
        });

        // Wait before retrying
        await new Promise(resolve => setTimeout(resolve, delayMs));
      }
    }

    // This should never be reached, but for type safety
    throw lastError || new Error("Checkpoint creation failed");
  }

  private async _doCreate(checkpoint: TCheckpoint): Promise<string> {
    const startTime = performance.now();
    const entityId = this.extractEntityId(checkpoint);

    try {
      logger.debug("Creating checkpoint", {
        checkpointId: checkpoint.id,
        type: checkpoint.type,
      });

      const serializedData = await this.codec.serialize(checkpoint);
      const metadata = this.extractStorageMetadata(checkpoint);
      metadata.blobSize = serializedData.length;

      await this.storageAdapter.save(checkpoint.id, serializedData, metadata);

      this.checkpointSizes.set(checkpoint.id, serializedData.length);

      // Fire event asynchronously without blocking
      this.emitEventAsync(checkpoint, 'created');

      const duration = performance.now() - startTime;

      this.metricsCollector?.recordCreation({
        checkpointId: checkpoint.id,
        entityId,
        type: checkpoint.type as "FULL" | "DELTA",
        duration,
        size: serializedData.length,
        timestamp: Date.now(),
        success: true,
      });

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

      // Fire error event asynchronously
      this.emitErrorEventAsync(checkpoint.id, error, 'create');

      throw error;
    }
  }

  /**
   * Emit event asynchronously without blocking the main flow
   */
  private async emitEventAsync(checkpoint: TCheckpoint, eventType: 'created' | 'deleted'): Promise<void> {
    if (!this.eventManager) return;

    try {
      const entityId = this.extractEntityId(checkpoint);
      const emitter = this.eventManager.getEmitter(entityId);

      emitter.beginBatch();
      try {
        const event = eventType === 'created'
          ? this.buildCreatedEvent(checkpoint)
          : this.buildDeletedEvent(checkpoint.id as string, 'manual');
        await this.eventManager.emit(event as BaseEvent);
      } finally {
        await emitter.endBatch();
      }
    } catch (error) {
      logger.warn(`Failed to emit ${eventType} event (non-critical)`, {
        error: error instanceof Error ? error.message : String(error),
      });
      // Don't throw, this is non-critical
    }
  }

  /**
   * Emit error event asynchronously
   */
  private async emitErrorEventAsync(
    checkpointId: string,
    error: unknown,
    operation: 'create' | 'restore' | 'delete',
  ): Promise<void> {
    if (!this.eventManager) return;

    try {
      const failedEvent = this.buildFailedEvent(checkpointId, error, operation);
      await this.eventManager.emit(failedEvent as BaseEvent);
    } catch {
      logger.warn("Failed to emit error event (non-critical)", {
        checkpointId,
        operation,
      });
    }
  }

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
    }
  }

  async get(checkpointId: string): Promise<TCheckpoint | null> {
    const startTime = performance.now();

    let data: Uint8Array | null;
    try {
      data = await this.storageAdapter.load(checkpointId);
    } catch (error) {
      logger.error("Storage error while loading checkpoint", {
        checkpointId,
        error: error instanceof Error ? error.message : String(error),
      });

      this.metricsCollector?.recordLoad({
        checkpointId,
        entityId: "unknown",
        duration: performance.now() - startTime,
        timestamp: Date.now(),
        success: false,
        error: error instanceof Error ? error.message : String(error),
      });

      throw error;
    }

    if (!data) {
      return null;
    }

    try {
      const checkpoint = await this.codec.deserialize<TCheckpoint>(data);
      this.checkpointSizes.set(checkpointId, data.length);

      const entityId = this.extractEntityId(checkpoint);

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

  async delete(
    checkpointId: string,
    reason: "manual" | "cleanup" | "policy" = "manual",
  ): Promise<void> {
    try {
      await this.storageAdapter.delete(checkpointId);
      this.checkpointSizes.delete(checkpointId);

      logger.info("Checkpoint deleted", { checkpointId, reason });

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

  async list(options?: { parentId?: string; limit?: number }): Promise<string[]> {
    return await this.storageAdapter.list(options);
  }

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

    return this.withEntityLock(entityId, async () => {
      const startTime = performance.now();
      const checkpoints = await this.storageAdapter.listByEntityWithMetadata(entityId, entityType);
      const entityMeta = await this.storageAdapter.getEntityMetadata(entityType, entityId);
      const lastWatermark = entityMeta?.['cleanupWatermark'] as number | undefined;
      const cleanupRunCount = (entityMeta?.['cleanupRunCount'] as number) || 0;

      // Step 1: Apply watermark filtering for incremental cleanup
      const isFullScan = cleanupRunCount % 10 === 0;
      let filteredCheckpoints = checkpoints;

      if (lastWatermark !== undefined && !isFullScan) {
        const beforeCount = checkpoints.length;
        filteredCheckpoints = checkpoints.filter(
          cp => cp.metadata.timestamp > lastWatermark || cp.id === excludeCheckpointId,
        );
        logger.debug("Incremental cleanup filtered by watermark", {
          entityId,
          beforeCount,
          afterCount: filteredCheckpoints.length,
          watermark: lastWatermark,
        });
      } else {
        logger.debug("Full scan cleanup", { entityId, entityType });
      }

      // Step 2: Get deletion candidates from strategy
      for (const info of filteredCheckpoints) {
        const size = info.metadata.blobSize ?? 0;
        if (size > 0) {
          this.checkpointSizes.set(info.id, size);
        }
      }

      const checkpointInfo: CheckpointInfo[] = filteredCheckpoints
        .filter(info => info.id !== excludeCheckpointId)
        .map(info => ({
          checkpointId: info.id,
          metadata: info.metadata,
        }));

      const dependencyGraph = buildDependencyGraph(
        filteredCheckpoints.map(cp => ({ id: cp.id, metadata: cp.metadata }))
      );
      const strategy = createCleanupStrategy(targetPolicy, this.checkpointSizes);
      let candidateDeleteIds = strategy.execute(checkpointInfo, dependencyGraph);

      logger.info("Strategy execution completed", {
        entityId,
        policy: targetPolicy.type,
        candidates: candidateDeleteIds.length,
      });

      // Step 3: Apply dependency protection
      if (candidateDeleteIds.length > 0) {
        const candidateSet = new Set(candidateDeleteIds);
        const protectedIds = computeProtectedCheckpoints(
          candidateSet,
          dependencyGraph,
          new Set(filteredCheckpoints.map(c => c.id)),
        );
        candidateDeleteIds = candidateDeleteIds.filter(id => !protectedIds.has(id));

        logger.debug("Dependency protection applied", {
          entityId,
          original: candidateSet.size,
          protected: protectedIds.size,
          remaining: candidateDeleteIds.length,
        });
      }

      // Step 4: Protect latest checkpoint
      if (candidateDeleteIds.length > 0) {
        const sortedByTimestamp = [...filteredCheckpoints].sort(
          (a, b) => b.metadata.timestamp - a.metadata.timestamp,
        );
        const latestCheckpointId = sortedByTimestamp[0]?.id;

        if (latestCheckpointId && candidateDeleteIds.includes(latestCheckpointId)) {
          candidateDeleteIds = candidateDeleteIds.filter(id => id !== latestCheckpointId);
          logger.debug("Latest checkpoint protected", { entityId, checkpointId: latestCheckpointId });
        }
      }

      // Step 5: Protect checkpoints referenced by survivors
      if (candidateDeleteIds.length > 0) {
        const toDeleteSet = new Set(candidateDeleteIds);
        const referencedBySurvivors = new Map<string, string[]>();

        for (const info of filteredCheckpoints) {
          if (toDeleteSet.has(info.id)) continue;
          const prevId = info.metadata.previousCheckpointId;
          if (prevId && toDeleteSet.has(prevId)) {
            const refs = referencedBySurvivors.get(prevId) || [];
            refs.push(info.id);
            referencedBySurvivors.set(prevId, refs);
          }
        }

        const finalDeleteIds = candidateDeleteIds.filter(id => !referencedBySurvivors.has(id));

        if (referencedBySurvivors.size > 0) {
          logger.debug("Survivor reference protection applied", {
            entityId,
            protectedCount: candidateDeleteIds.length - finalDeleteIds.length,
            protectedIds: [...referencedBySurvivors.keys()],
          });
        }

        candidateDeleteIds = finalDeleteIds;
      }

      logger.info("Cleanup protection complete", {
        entityId,
        finalDeletions: candidateDeleteIds.length,
      });

      // Step 6: Execute deletions
      if (candidateDeleteIds.length === 0) {
        return {
          deletedCheckpointIds: [],
          deletedCount: 0,
          freedSpaceBytes: 0,
          remainingCount: checkpoints.length,
        };
      }

      let freedSpaceBytes = 0;
      const deletedIds: string[] = [];

      for (const checkpointId of candidateDeleteIds) {
        try {
          const size = this.checkpointSizes.get(checkpointId) || 0;
          await this.storageAdapter.delete(checkpointId);
          this.checkpointSizes.delete(checkpointId);
          freedSpaceBytes += size;
          deletedIds.push(checkpointId);
        } catch (error) {
          logger.warn("Failed to delete checkpoint", {
            checkpointId,
            error: error instanceof Error ? error.message : String(error),
          });
        }
      }

      // Update watermark
      const remainingCheckpoints = checkpoints.filter(cp => !deletedIds.includes(cp.id));
      if (remainingCheckpoints.length > 0) {
        const maxTimestamp = Math.max(...remainingCheckpoints.map(cp => cp.metadata.timestamp));
        await this.storageAdapter.setEntityMetadata(entityType, entityId, {
          cleanupWatermark: maxTimestamp,
          cleanupRunCount: cleanupRunCount + 1,
        });
      }

      const duration = performance.now() - startTime;
      logger.info("Entity cleanup completed", {
        entityId,
        entityType,
        deletedCount: deletedIds.length,
        freedSpaceBytes,
        remainingCount: remainingCheckpoints.length,
        duration: `${duration.toFixed(2)}ms`,
      });

      return {
        deletedCheckpointIds: deletedIds,
        deletedCount: deletedIds.length,
        freedSpaceBytes,
        remainingCount: remainingCheckpoints.length,
      };
    });
  }

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

    const chains: Array<Array<{ idx: number; cp: typeof deltas[0] }>> = [];
    const processed = new Set<string>();

    for (let i = 0; i < deltas.length; i++) {
      if (processed.has(deltas[i]!.id)) continue;

      const chain: Array<{ idx: number; cp: typeof deltas[0] }> = [];
      let currentIdx = i;

      while (currentIdx < deltas.length && !processed.has(deltas[currentIdx]!.id)) {
        const cp = deltas[currentIdx]!;
        const prevId = cp.metadata.previousCheckpointId;

        if (chain.length === 0 || prevId === chain[chain.length - 1]!.cp.id) {
          chain.push({ idx: currentIdx, cp });
          processed.add(cp.id);
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

    for (const chain of chains) {
      const effectiveBatchSize = batchSize ?? chain.length - 1;
      const mergeCount = Math.min(effectiveBatchSize, chain.length - 1);

      if (mergeCount < 1) continue;

      const anchor = chain[0]!.cp;
      const toMerge = chain.slice(1, mergeCount + 1);
      const lastMerged = toMerge[toMerge.length - 1]!;

      const idsToLoad = [anchor.id, ...toMerge.map(c => c.cp.id)];
      const rawDataList = await Promise.all(
        idsToLoad.map(id => this.storageAdapter.load(id)),
      );

      if (rawDataList.some(d => d === null)) {
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

      let mergedDelta = cps[0]!.delta as unknown as DeltaMap;
      for (let i = 1; i < cps.length; i++) {
        mergedDelta = this.diffCalculator.mergeDeltas(
          mergedDelta,
          cps[i]!.delta as unknown as DeltaMap,
        );
      }
      cps[0]!.delta = mergedDelta as unknown as TCheckpoint["delta"];

      const updatedData = await this.codec.serialize(cps[0]!);
      const updatedMetadata = this.extractStorageMetadata(cps[0]!);
      updatedMetadata.blobSize = updatedData.length;
      await this.storageAdapter.save(anchor.id, updatedData, updatedMetadata);
      this.checkpointSizes.set(anchor.id, updatedData.length);

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

  getMetricsCollector(): CheckpointMetricsCollector | undefined {
    return this.metricsCollector;
  }

  async listByEntityWithMetadata(
    entityId: string,
    entityType: string,
  ): Promise<Array<{ id: string; metadata: CheckpointStorageMetadata }>> {
    const records = await this.storageAdapter.listByEntityWithMetadata(entityId, entityType);
    return records.map(r => ({
      id: r.id,
      metadata: r.metadata as CheckpointStorageMetadata,
    }));
  }

  async initialize(): Promise<void> {
    logger.info("Initializing checkpoint state manager");
    await this.storageAdapter.initialize();
  }

  async cleanup(): Promise<void> {
    logger.info("Cleaning up checkpoint state manager");
    await this.storageAdapter.close();
  }

  protected abstract extractStorageMetadata(checkpoint: TCheckpoint): CheckpointStorageMetadata;

  protected abstract buildCreatedEvent(checkpoint: TCheckpoint): unknown;

  protected abstract buildDeletedEvent(
    checkpointId: string,
    reason?: "manual" | "cleanup" | "policy",
  ): unknown | Promise<unknown>;

   protected abstract buildFailedEvent(
     checkpointId: string,
     error: unknown,
     operation?: "create" | "restore" | "delete",
   ): unknown;

   private async withEntityLock<T>(
     entityId: string,
     operation: () => Promise<T>,
   ): Promise<T> {
     const previousLock = this.cleanupLocks.get(entityId) ?? Promise.resolve();

     /**
      * Sequential locking: ensure operations execute one by one.
      * CRITICAL FIX: Preserve rejection state from previous lock.
      *
      * Original buggy code:
      *   const currentLock = previousLock.then(
      *     () => operation(),
      *     () => operation(),  // ← BUG: executes even if previous failed!
      *   );
      *   cleanupLocks.set(entityId, currentLock.then(
      *     () => undefined,
      *     () => undefined,    // ← BUG: converts rejection to success!
      *   ));
      *
      * This allowed operations to execute in parallel and convert failures to success.
      * Proper sequential lock should:
      * 1. Execute operation only after previous completes (fail or success)
      * 2. Stop execution chain if previous failed
      * 3. Preserve rejection state for next operation
      */
     const currentLock = previousLock.then(() => operation());

     // CRITICAL: Store the actual currentLock, not a converted version
     // This preserves the rejection state for sequential execution
     this.cleanupLocks.set(entityId, currentLock);

     try {
       return await currentLock;
     } finally {
       const storedLock = this.cleanupLocks.get(entityId);
       if (storedLock) {
         // Clean up the lock after this operation completes (regardless of success/failure)
         storedLock.then(
           () => {
             if (this.cleanupLocks.get(entityId) === storedLock) {
               this.cleanupLocks.delete(entityId);
             }
           },
           () => {
             if (this.cleanupLocks.get(entityId) === storedLock) {
               this.cleanupLocks.delete(entityId);
             }
           }
         );
       }
     }
   }
}
