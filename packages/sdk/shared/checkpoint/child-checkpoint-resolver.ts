/**
 * Child Checkpoint Resolver
 *
 * Generic resolver for finding the latest checkpoint of child execution instances.
 * Provides a consistent interface for resolving child checkpoints across
 * different execution types (WORKFLOW, AGENT_LOOP).
 */

import type { CheckpointStorageAdapter } from "./types.js";
import type { CheckpointStorageMetadata, CheckpointEntityType } from "@wf-agent/types";
import type { ChildExecutionReference } from "@wf-agent/types";

/**
 * Resolved checkpoint descriptor
 */
export interface ChildCheckpointDescriptor {
  checkpointId: string;
  metadata: CheckpointStorageMetadata;
}

/**
 * Resolve the latest checkpoint for a child execution instance.
 *
 * Implementations may use different strategies:
 * - Storage-backed: queries storage adapter with proper ORDER BY
 * - Cached: uses preloaded metadata for batch operations
 */
export interface ChildCheckpointResolver {
  /**
   * Resolve the latest checkpoint for a child execution
   * @param childRef Child execution reference
   * @returns Latest checkpoint descriptor or null if not found
   */
  resolveLatestCheckpoint(
    childRef: ChildExecutionReference,
  ): Promise<ChildCheckpointDescriptor | null>;

  /**
   * Resolve latest checkpoints for multiple children in batch
   * @param childRefs Array of child execution references
   * @returns Map of child ID to checkpoint descriptor (or null)
   */
  resolveLatestCheckpoints(
    childRefs: ChildExecutionReference[],
  ): Promise<Map<string, ChildCheckpointDescriptor | null>>;
}

/**
 * Storage-backed implementation of ChildCheckpointResolver.
 * Uses the CheckpointStorageAdapter's listByEntityWithMetadata for efficient queries.
 */
export class StorageBackedChildResolver implements ChildCheckpointResolver {
  constructor(private storageAdapter: CheckpointStorageAdapter) {}

  async resolveLatestCheckpoint(
    childRef: ChildExecutionReference,
  ): Promise<ChildCheckpointDescriptor | null> {
    const results = await this.storageAdapter.listByEntityWithMetadata(
      childRef.childId,
      childRef.childType as CheckpointEntityType,
      { limit: 1 }
    );

    if (results.length === 0) {
      return null;
    }

    const latest = results[0]!;
    return {
      checkpointId: latest.id,
      metadata: latest.metadata,
    };
  }

  async resolveLatestCheckpoints(
    childRefs: ChildExecutionReference[],
  ): Promise<Map<string, ChildCheckpointDescriptor | null>> {
    const resultMap = new Map<string, ChildCheckpointDescriptor | null>();

    if (childRefs.length === 0) {
      return resultMap;
    }

    // Batch resolve each child individually
    // Note: For large batches, consider parallel execution with concurrency limit
    const promises = childRefs.map(async (childRef) => {
      const descriptor = await this.resolveLatestCheckpoint(childRef);
      return { childId: childRef.childId, descriptor };
    });

    const results = await Promise.all(promises);
    for (const { childId, descriptor } of results) {
      resultMap.set(childId, descriptor);
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
