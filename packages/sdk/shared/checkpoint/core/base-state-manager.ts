import type {
  BaseCheckpoint,
  CleanupPolicy,
  CleanupResult,
  CheckpointStorageRecord,
  CheckpointStorageMetadata,
  CheckpointInfo,
  BaseEvent,
} from "@wf-agent/types";
import type { EventRegistry } from "../../registry/event-registry.js";
import { StateCodec } from "@wf-agent/common-utils";
import { BaseDiffCalculator } from "./base-diff-calculator.js";
import type { DeltaMap } from "./base-diff-calculator.js";
import { createCleanupStrategy } from "../utils/cleanup-policy.js";
import { buildDependencyGraph, computeProtectedCheckpoints } from "../checkpoint-graph.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import type { CheckpointStorageAdapter } from "../types.js";
import { CheckpointMetricsCollector } from "./metrics-collector.js";
import type { CheckpointMetricsConfig } from "@wf-agent/types";

const logger = createContextualLogger({ component: "BaseCheckpointStateManager" });

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
  private cleanupLocks = new Map<string, Promise<void>>();

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

      this.checkpointSizes.set(checkpoint.id, serializedData.length);

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

      if (this.eventManager) {
        const failedEvent = this.buildFailedEvent(checkpoint.id, error, "create");
        await this.eventManager.emit(failedEvent as BaseEvent);
      }

      throw error;
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

      const entityId = (checkpoint as unknown as Record<string, string>)['executionId'] || (checkpoint as unknown as Record<string, string>)['agentLoopId'] || "unknown";

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
     return this.withEntityLock(entityId, () =>
       this._executeCleanupForEntityInternal(entityId, entityType, excludeCheckpointId, policy),
     );
   }

  private async _executeCleanupForEntityInternal(
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

    let checkpointInfoArray = await this.storageAdapter.listByEntityWithMetadata(
      entityId,
      entityType,
    );

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

    for (const info of checkpointInfoArray) {
      const metadata = info.metadata as CheckpointStorageMetadata;
      const size = metadata.blobSize ?? 0;
      if (size > 0) {
        this.checkpointSizes.set(info.id, size);
      }
    }

    const checkpointInfo: CheckpointInfo[] = checkpointInfoArray
      .filter(info => info.id !== excludeCheckpointId)
      .map(info => ({
        checkpointId: info.id,
        metadata: info.metadata as CheckpointStorageMetadata,
      }));

     const dependencyGraph = buildDependencyGraph(checkpointInfoArray);

     const strategy = createCleanupStrategy(targetPolicy, this.checkpointSizes);
     const candidateDeleteIds = strategy.execute(checkpointInfo, dependencyGraph);

     const candidateSet = new Set(candidateDeleteIds);
     const protectedByDependency = computeProtectedCheckpoints(
       candidateSet,
       dependencyGraph,
       new Set(checkpointInfoArray.map(c => c.id)),
     );

     const filteredDeleteIds = candidateDeleteIds.filter(id => !protectedByDependency.has(id));

     const sortedByTimestamp = [...checkpointInfoArray].sort(
       (a, b) => b.metadata.timestamp - a.metadata.timestamp,
     );
     const latestCheckpointId = sortedByTimestamp[0]?.id;

     let toDeleteIds = filteredDeleteIds;
     if (latestCheckpointId && toDeleteIds.includes(latestCheckpointId)) {
       toDeleteIds = toDeleteIds.filter(id => id !== latestCheckpointId);
       logger.debug("Protected latest checkpoint from deletion", {
         checkpointId: latestCheckpointId,
       });
     }

     const referencedBySurvivors = new Map<string, string[]>();
     const toDeleteSet = new Set(toDeleteIds);

     for (const info of checkpointInfoArray) {
       if (toDeleteSet.has(info.id)) continue;
       const prevId = info.metadata.previousCheckpointId;
       if (prevId && toDeleteSet.has(prevId)) {
         const refs = referencedBySurvivors.get(prevId) || [];
         refs.push(info.id);
         referencedBySurvivors.set(prevId, refs);
       }
     }

     for (const candidateId of [...toDeleteIds]) {
       if (referencedBySurvivors.has(candidateId)) {
         toDeleteIds = toDeleteIds.filter(id => id !== candidateId);
         logger.debug("Protected checkpoint referenced by surviving delta", {
           checkpointId: candidateId,
           referencedBy: referencedBySurvivors.get(candidateId),
         });
       }
     }

     logger.info("Cleanup protection applied", {
       originalCandidates: candidateDeleteIds.length,
       finalDeletions: toDeleteIds.length,
       protectedCount: candidateDeleteIds.length - toDeleteIds.length,
     });

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

  protected abstract extractStorageMetadata(checkpoint: TCheckpoint): CheckpointStorageRecord;

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

     const currentLock = previousLock.then(
       () => operation(),
       () => operation(),
     );

     this.cleanupLocks.set(
       entityId,
       currentLock.then(
         () => undefined,
         () => undefined,
       ),
     );

     try {
       return await currentLock;
     } finally {
       const storedLock = this.cleanupLocks.get(entityId);
       if (storedLock) {
         storedLock.then(() => {
           if (this.cleanupLocks.get(entityId) === storedLock) {
             this.cleanupLocks.delete(entityId);
           }
         }).catch(() => {
           if (this.cleanupLocks.get(entityId) === storedLock) {
             this.cleanupLocks.delete(entityId);
           }
         });
       }
     }
   }
}
