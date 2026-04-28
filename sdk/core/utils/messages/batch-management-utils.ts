/**
 * Batch Management Tool Functions
 * Responsible for managing message batches, including creating new batches, reverting to historical batches, and other operations.
 *
 * All functions are pure functions and do not hold any state.
 */

import type { MessageMarkMap, BatchCheckpointInfo } from "@wf-agent/types";
import { ExecutionError } from "@wf-agent/types";

/**
 * Start a new batch
 * @param markMap Message marker mapping
 * @param boundaryIndex Boundary index
 * @returns Updated message marker mapping
 * @throws ExecutionError Throws an exception when the boundary index is invalid
 */
export function startNewBatch(markMap: MessageMarkMap, boundaryIndex: number): MessageMarkMap {
  // Verify the boundary index range.
  if (boundaryIndex < 0 || boundaryIndex > markMap.originalIndices.length) {
    throw new ExecutionError(
      `Invalid boundary index: ${boundaryIndex}. Must be between 0 and ${markMap.originalIndices.length}`,
    );
  }

  // Verify the incrementality of boundary indices.
  const lastBoundary = markMap.batchBoundaries[markMap.batchBoundaries.length - 1];
  if (lastBoundary !== undefined && boundaryIndex < lastBoundary) {
    throw new ExecutionError(
      `Boundary index must be non-decreasing. Last boundary: ${lastBoundary}, new boundary: ${boundaryIndex}`,
    );
  }

  // Create a new copy of the marker mapping.
  const newMarkMap: MessageMarkMap = {
    ...markMap,
    originalIndices: [...markMap.originalIndices],
    batchBoundaries: [...markMap.batchBoundaries],
    boundaryToBatch: [...markMap.boundaryToBatch],
  };

  // Add new boundaries
  newMarkMap.batchBoundaries.push(boundaryIndex);

  // Assign a new batch number
  const newBatch = markMap.currentBatch + 1;
  newMarkMap.boundaryToBatch.push(newBatch);
  newMarkMap.currentBatch = newBatch;

  return newMarkMap;
}

/**
 * Back to the specified batch
 * @param markMap Message marker mapping
 * @param targetBatch Target batch number
 * @returns Updated message marker mapping
 * @throws ExecutionError Throws an exception if the target batch does not exist
 */
export function rollbackToBatch(markMap: MessageMarkMap, targetBatch: number): MessageMarkMap {
  if (!markMap.boundaryToBatch.includes(targetBatch)) {
    throw new ExecutionError(
      `Target batch ${targetBatch} not found. Available batches: ${markMap.boundaryToBatch.join(", ")}`,
    );
  }

  const targetBoundaryIndex = markMap.boundaryToBatch.indexOf(targetBatch);

  const newMarkMap: MessageMarkMap = {
    ...markMap,
    originalIndices: [...markMap.originalIndices],
    batchBoundaries: markMap.batchBoundaries.slice(0, targetBoundaryIndex + 1),
    boundaryToBatch: markMap.boundaryToBatch.slice(0, targetBoundaryIndex + 1),
    currentBatch: targetBatch,
  };

  return newMarkMap;
}

/**
 * Merge Batches
 * @param markMap Message marker mapping
 * @param fromBatch Start batch number
 * @param toBatch End batch number
 * @returns Updated message marker mapping
 * @throws ExecutionError Throws an exception if the batch does not exist or is invalid
 */
export function mergeBatches(
  markMap: MessageMarkMap,
  fromBatch: number,
  toBatch: number,
): MessageMarkMap {
  if (!markMap.boundaryToBatch.includes(fromBatch)) {
    throw new ExecutionError(`Source batch ${fromBatch} not found`);
  }

  if (!markMap.boundaryToBatch.includes(toBatch)) {
    throw new ExecutionError(`Target batch ${toBatch} not found`);
  }

  const fromIndex = markMap.boundaryToBatch.indexOf(fromBatch);
  const toIndex = markMap.boundaryToBatch.indexOf(toBatch);

  if (fromIndex >= toIndex) {
    throw new ExecutionError(
      `Invalid batch range: fromBatch (${fromBatch}) must be before toBatch (${toBatch})`,
    );
  }

  // Remove the intermediate batch boundaries.
  const newMarkMap: MessageMarkMap = {
    ...markMap,
    originalIndices: [...markMap.originalIndices],
    batchBoundaries: [
      ...markMap.batchBoundaries.slice(0, fromIndex + 1),
      ...markMap.batchBoundaries.slice(toIndex + 1),
    ],
    boundaryToBatch: [
      ...markMap.boundaryToBatch.slice(0, fromIndex + 1),
      ...markMap.boundaryToBatch.slice(toIndex + 1),
    ],
    currentBatch: markMap.currentBatch,
  };

  return newMarkMap;
}

/**
 * Get batch information
 * @param markMap Message marker mapping
 * @param batchId Batch ID
 * @returns Batch information, including boundary indices and the number of visible messages
 * @throws ExecutionError Throws an exception if the batch does not exist
 */
export function getBatchInfo(
  markMap: MessageMarkMap,
  batchId: number,
): {
  boundaryIndex: number;
  visibleMessageCount: number;
  isCurrentBatch: boolean;
} {
  if (!markMap.boundaryToBatch.includes(batchId)) {
    throw new ExecutionError(`Batch ${batchId} not found`);
  }

  const boundaryIndex = markMap.batchBoundaries[batchId];
  if (boundaryIndex === undefined) {
    throw new ExecutionError(`Boundary index for batch ${batchId} is undefined`);
  }

  const visibleMessageCount = markMap.originalIndices.filter(
    index => index >= boundaryIndex,
  ).length;
  const isCurrentBatch = batchId === markMap.currentBatch;

  return {
    boundaryIndex,
    visibleMessageCount,
    isCurrentBatch,
  };
}

/**
 * Get all batch information
 * @param markMap Message marker mapping
 * @returns Array of all batch information
 */
export function getAllBatchesInfo(markMap: MessageMarkMap): Array<{
  batchId: number;
  boundaryIndex: number;
  visibleMessageCount: number;
  isCurrentBatch: boolean;
}> {
  return markMap.boundaryToBatch.map(batchId => {
    const boundaryIndex = markMap.batchBoundaries[batchId];
    if (boundaryIndex === undefined) {
      throw new ExecutionError(`Boundary index for batch ${batchId} is undefined`);
    }

    const visibleMessageCount = markMap.originalIndices.filter(
      index => index >= boundaryIndex,
    ).length;
    const isCurrentBatch = batchId === markMap.currentBatch;

    return {
      batchId,
      boundaryIndex,
      visibleMessageCount,
      isCurrentBatch,
    };
  });
}

// ============================================================
// Checkpoint Memory Optimization Functions
// ============================================================

/**
 * Start a new batch with checkpoint mapping
 * @param markMap Message marker mapping
 * @param boundaryIndex Boundary index
 * @param checkpointId Optional checkpoint ID for the previous batch
 * @returns Updated message marker mapping
 * @throws ExecutionError Throws an exception when the boundary index is invalid
 */
export function startNewBatchWithCheckpoint(
  markMap: MessageMarkMap,
  boundaryIndex: number,
  checkpointId?: string,
): MessageMarkMap {
  // First, use the standard startNewBatch logic
  const newMarkMap = startNewBatch(markMap, boundaryIndex);

  // Initialize batchToCheckpoint if not exists
  if (!newMarkMap.batchToCheckpoint) {
    newMarkMap.batchToCheckpoint = [];
  }

  // Calculate message count for the previous batch
  const previousBatch = markMap.currentBatch;
  const previousBoundary = markMap.batchBoundaries[previousBatch];
  const currentBoundary = boundaryIndex;
  const messageCount = currentBoundary - (previousBoundary ?? 0);

  // Update checkpoint mapping for the previous batch
  const existingIndex = newMarkMap.batchToCheckpoint.findIndex(
    info => info.batchId === previousBatch,
  );

  const checkpointInfo: BatchCheckpointInfo = {
    batchId: previousBatch,
    checkpointId: checkpointId ?? null,
    messageCount,
    timestamp: checkpointId ? Date.now() : undefined,
  };

  if (existingIndex >= 0) {
    newMarkMap.batchToCheckpoint[existingIndex] = checkpointInfo;
  } else {
    newMarkMap.batchToCheckpoint.push(checkpointInfo);
  }

  // Update memory range to include current batch
  newMarkMap.memoryRange = {
    startBatch: previousBatch,
    endBatch: newMarkMap.currentBatch,
  };

  return newMarkMap;
}

/**
 * Get batches that can be released from memory
 * @param markMap Message marker mapping
 * @param keepInMemory Number of recent batches to keep in memory
 * @returns Array of batch IDs that can be released
 */
export function getBatchesToRelease(markMap: MessageMarkMap, keepInMemory: number): number[] {
  if (!markMap.batchToCheckpoint || markMap.batchToCheckpoint.length === 0) {
    return [];
  }

  const currentBatch = markMap.currentBatch;
  const batchesToRelease: number[] = [];

  for (const info of markMap.batchToCheckpoint) {
    // Keep recent batches in memory
    if (info.batchId < currentBatch - keepInMemory + 1) {
      // Only release if it has a checkpoint ID (already persisted)
      if (info.checkpointId !== null) {
        batchesToRelease.push(info.batchId);
      }
    }
  }

  return batchesToRelease;
}

/**
 * Update checkpoint ID for a batch
 * @param markMap Message marker mapping
 * @param batchId Batch ID
 * @param checkpointId Checkpoint ID
 * @returns Updated message marker mapping
 */
export function updateBatchCheckpoint(
  markMap: MessageMarkMap,
  batchId: number,
  checkpointId: string,
): MessageMarkMap {
  if (!markMap.batchToCheckpoint) {
    throw new ExecutionError(`batchToCheckpoint not initialized`);
  }

  const existingIndex = markMap.batchToCheckpoint.findIndex(info => info.batchId === batchId);

  if (existingIndex < 0) {
    throw new ExecutionError(`Batch ${batchId} not found in batchToCheckpoint`);
  }

  const newBatchToCheckpoint = [...markMap.batchToCheckpoint];
  newBatchToCheckpoint[existingIndex] = {
    ...newBatchToCheckpoint[existingIndex]!,
    checkpointId,
    timestamp: Date.now(),
  };

  const newMarkMap: MessageMarkMap = {
    ...markMap,
    originalIndices: [...markMap.originalIndices],
    batchBoundaries: [...markMap.batchBoundaries],
    boundaryToBatch: [...markMap.boundaryToBatch],
    batchToCheckpoint: newBatchToCheckpoint,
  };

  return newMarkMap;
}

/**
 * Get checkpoint ID for a batch
 * @param markMap Message marker mapping
 * @param batchId Batch ID
 * @returns Checkpoint ID or null if not found
 */
export function getBatchCheckpointId(
  markMap: MessageMarkMap,
  batchId: number,
): string | null | undefined {
  if (!markMap.batchToCheckpoint) {
    return undefined;
  }

  const info = markMap.batchToCheckpoint.find(item => item.batchId === batchId);
  return info?.checkpointId;
}

/**
 * Check if a batch is loaded in memory
 * @param markMap Message marker mapping
 * @param batchId Batch ID
 * @returns True if batch is in memory
 */
export function isBatchInMemory(markMap: MessageMarkMap, batchId: number): boolean {
  // If no checkpoint mapping exists, assume all batches are in memory
  if (!markMap.batchToCheckpoint) {
    return true;
  }

  const info = markMap.batchToCheckpoint.find(item => item.batchId === batchId);
  // If no info or checkpointId is null, batch is in memory
  return !info || info.checkpointId === null;
}

/**
 * Rebuild originalIndices after releasing batches from memory
 * @param markMap Message marker mapping
 * @param releasedBatches Array of released batch IDs
 * @returns Updated message marker mapping
 */
export function rebuildIndicesAfterRelease(
  markMap: MessageMarkMap,
  releasedBatches: number[],
): MessageMarkMap {
  if (releasedBatches.length === 0) {
    return markMap;
  }

  // Filter out indices belonging to released batches
  const releasedBatchSet = new Set(releasedBatches);
  const newOriginalIndices = markMap.originalIndices.filter((_, index) => {
    // Find which batch this index belongs to
    for (let batchId = 0; batchId < markMap.batchBoundaries.length; batchId++) {
      const startBoundary = markMap.batchBoundaries[batchId] ?? 0;
      const endBoundary = markMap.batchBoundaries[batchId + 1] ?? markMap.originalIndices.length;
      if (index >= startBoundary && index < endBoundary) {
        return !releasedBatchSet.has(batchId);
      }
    }
    return true;
  });

  // Reindex the remaining indices
  const reindexedOriginalIndices = newOriginalIndices.map((_, index) => index);

  return {
    ...markMap,
    originalIndices: reindexedOriginalIndices,
    batchBoundaries: [...markMap.batchBoundaries],
    boundaryToBatch: [...markMap.boundaryToBatch],
    batchToCheckpoint: markMap.batchToCheckpoint ? [...markMap.batchToCheckpoint] : undefined,
    memoryRange: markMap.memoryRange ? { ...markMap.memoryRange } : undefined,
  };
}
