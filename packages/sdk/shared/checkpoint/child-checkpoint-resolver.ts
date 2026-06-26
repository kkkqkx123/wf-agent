/**
 * Child Checkpoint Resolver
 *
 * Generic resolver for finding the latest checkpoint of child execution instances.
 * Provides a consistent interface for resolving child checkpoints across
 * different execution types (WORKFLOW, AGENT_LOOP).
 *
 * Supports both individual and batch resolution for efficiency.
 * Batch resolution uses storage adapter's batch capabilities when available.
 */

import type { CheckpointStorageAdapter } from "./types.js";
import type { CheckpointStorageMetadata, CheckpointEntityType } from "@wf-agent/types";
import type { ChildExecutionReference } from "@wf-agent/types";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "ChildCheckpointResolver" });

export interface ChildCheckpointDescriptor {
  checkpointId: string;
  metadata: CheckpointStorageMetadata;
}

export interface ChildCheckpointResolver {
  resolveLatestCheckpoint(childRef: ChildExecutionReference): Promise<ChildCheckpointDescriptor | null>;
  resolveLatestCheckpoints(childRefs: ChildExecutionReference[]): Promise<Map<string, ChildCheckpointDescriptor | null>>;
}

export class StorageBackedChildResolver implements ChildCheckpointResolver {
  constructor(private storageAdapter: CheckpointStorageAdapter) {}

   async resolveLatestCheckpoint(childRef: ChildExecutionReference): Promise<ChildCheckpointDescriptor | null> {
     const results = await this.storageAdapter.listByEntityWithMetadata(childRef.childId, childRef.childType as CheckpointEntityType, { limit: 10 });
     if (results.length === 0) return null;
     const sorted = [...results].sort(
       (a, b) => b.metadata.timestamp - a.metadata.timestamp,
     );
     const latest = sorted[0]!;
     return { checkpointId: latest.id, metadata: latest.metadata };
   }

  async resolveLatestCheckpoints(childRefs: ChildExecutionReference[]): Promise<Map<string, ChildCheckpointDescriptor | null>> {
    if (childRefs.length === 0) {
      return new Map();
    }

    if (childRefs.length === 1) {
      const descriptor = await this.resolveLatestCheckpoint(childRefs[0]!);
      const resultMap = new Map();
      resultMap.set(childRefs[0]!.childId, descriptor);
      return resultMap;
    }

    if ('listByEntitiesWithMetadata' in this.storageAdapter) {
      return this.resolveBatchViaAdapter(childRefs);
    }

    return this.resolveBatchParallel(childRefs);
  }

  private async resolveBatchViaAdapter(childRefs: ChildExecutionReference[]): Promise<Map<string, ChildCheckpointDescriptor | null>> {
    const adapter = this.storageAdapter as CheckpointStorageAdapter & {
      listByEntitiesWithMetadata: (
        entityIds: string[],
        entityType: CheckpointEntityType,
        options?: { limit?: number }
      ) => Promise<Array<{ entityId: string; checkpoints: Array<{ id: string; metadata: CheckpointStorageMetadata }> }>>;
    };

    const entityIds = childRefs.map(r => r.childId);
    const entityTypes = new Set(childRefs.map(r => r.childType));

    if (entityTypes.size > 1) {
      return this.resolveBatchParallel(childRefs);
    }

    try {
      const results = await adapter.listByEntitiesWithMetadata(
        entityIds,
        Array.from(entityTypes)[0] as CheckpointEntityType,
        { limit: 1 },
      );

      const resultMap = new Map<string, ChildCheckpointDescriptor | null>();
       for (const childRef of childRefs) {
         const entityResult = results.find(r => r.entityId === childRef.childId);
         const checkpoints = entityResult?.checkpoints ?? [];
         const sorted = [...checkpoints].sort(
           (a, b) => b.metadata.timestamp - a.metadata.timestamp,
         );
         const checkpoint = sorted[0];
         if (checkpoint) {
           resultMap.set(childRef.childId, {
             checkpointId: checkpoint.id,
             metadata: checkpoint.metadata,
           });
         } else {
           resultMap.set(childRef.childId, null);
         }
       }

      return resultMap;
    } catch (error) {
      logger.warn("Batch resolution via adapter failed, falling back to parallel", {
        error: error instanceof Error ? error.message : String(error),
      });
      return this.resolveBatchParallel(childRefs);
    }
  }

  private async resolveBatchParallel(childRefs: ChildExecutionReference[]): Promise<Map<string, ChildCheckpointDescriptor | null>> {
    const BATCH_SIZE = 10;
    const resultMap = new Map();

    for (let i = 0; i < childRefs.length; i += BATCH_SIZE) {
      const batch = childRefs.slice(i, i + BATCH_SIZE);
      const promises = batch.map(async (childRef) => {
        const descriptor = await this.resolveLatestCheckpoint(childRef);
        return { childId: childRef.childId, descriptor };
      });
      const results = await Promise.all(promises);
      for (const { childId, descriptor } of results) {
        resultMap.set(childId, descriptor);
      }
    }

    return resultMap;
  }
}

/**
 * Cached resolver that uses preloaded metadata.
 * Useful when checkpoint metadata is already available in memory.
 */
export class CachedChildResolver implements ChildCheckpointResolver {
  private cache = new Map<string, ChildCheckpointDescriptor[]>();

  /**
   * Preload checkpoints for an entity
   */
  preload(
    entityId: string,
    checkpoints: ChildCheckpointDescriptor[],
  ): void {
    const sorted = [...checkpoints].sort(
      (a, b) => b.metadata.timestamp - a.metadata.timestamp
    );
    this.cache.set(entityId, sorted);
  }

  /**
   * Clear cached checkpoints for an entity
   */
  clearCache(entityId?: string): void {
    if (entityId) {
      this.cache.delete(entityId);
    } else {
      this.cache.clear();
    }
  }

  async resolveLatestCheckpoint(
    childRef: ChildExecutionReference,
  ): Promise<ChildCheckpointDescriptor | null> {
    const cached = this.cache.get(childRef.childId);
    if (!cached || cached.length === 0) {
      return null;
    }
    return cached[0]!;
  }

  async resolveLatestCheckpoints(
    childRefs: ChildExecutionReference[],
  ): Promise<Map<string, ChildCheckpointDescriptor | null>> {
    const resultMap = new Map<string, ChildCheckpointDescriptor | null>();

    for (const childRef of childRefs) {
      const descriptor = await this.resolveLatestCheckpoint(childRef);
      resultMap.set(childRef.childId, descriptor);
    }

    return resultMap;
  }
}
