/**
 * BatchManagementUtils Unit Tests
 * Tests for batch management and checkpoint memory optimization functions
 */

import { describe, it, expect } from "vitest";
import type { MessageMarkMap, BatchCheckpointInfo } from "@wf-agent/types";
import {
  startNewBatch,
  startNewBatchWithCheckpoint,
  rollbackToBatch,
  mergeBatches,
  getBatchInfo,
  getAllBatchesInfo,
  getBatchesToRelease,
  updateBatchCheckpoint,
  getBatchCheckpointId,
  isBatchInMemory,
  rebuildIndicesAfterRelease,
} from "../batch-management-utils.js";

// Helper function to create a mock MessageMarkMap
function createMockMarkMap(
  currentBatch: number = 0,
  batchBoundaries: number[] = [0],
  originalIndices: number[] = [],
  batchToCheckpoint?: BatchCheckpointInfo[],
): MessageMarkMap {
  return {
    originalIndices:
      originalIndices.length > 0 ? originalIndices : Array.from({ length: 10 }, (_, i) => i),
    batchBoundaries: [...batchBoundaries],
    boundaryToBatch: batchBoundaries.map((_, i) => i),
    currentBatch,
    batchToCheckpoint,
  };
}

describe("startNewBatch", () => {
  it("should create a new batch correctly", () => {
    const markMap = createMockMarkMap(0, [0]);
    const newMarkMap = startNewBatch(markMap, 5);

    expect(newMarkMap.currentBatch).toBe(1);
    expect(newMarkMap.batchBoundaries).toHaveLength(2);
    expect(newMarkMap.batchBoundaries[1]).toBe(5);
    expect(newMarkMap.boundaryToBatch).toContain(1);
  });

  it("should throw error for invalid boundary index", () => {
    const markMap = createMockMarkMap(0, [0], [0, 1, 2, 3, 4]);
    expect(() => startNewBatch(markMap, -1)).toThrow();
    expect(() => startNewBatch(markMap, 10)).toThrow();
  });

  it("should throw error for non-increasing boundary", () => {
    const markMap = createMockMarkMap(1, [0, 5]);
    expect(() => startNewBatch(markMap, 3)).toThrow();
  });
});

describe("startNewBatchWithCheckpoint", () => {
  it("should create a new batch with checkpoint mapping", () => {
    const markMap = createMockMarkMap(
      0,
      [0],
      Array.from({ length: 10 }, (_, i) => i),
    );
    const checkpointId = "checkpoint-123";
    const newMarkMap = startNewBatchWithCheckpoint(markMap, 5, checkpointId);

    expect(newMarkMap.currentBatch).toBe(1);
    expect(newMarkMap.batchToCheckpoint).toBeDefined();
    expect(newMarkMap.batchToCheckpoint).toHaveLength(1);
    expect(newMarkMap.batchToCheckpoint![0]!.batchId).toBe(0);
    expect(newMarkMap.batchToCheckpoint![0]!.checkpointId).toBe(checkpointId);
    expect(newMarkMap.batchToCheckpoint![0]!.messageCount).toBe(5);
  });

  it("should initialize batchToCheckpoint if not exists", () => {
    const markMap = createMockMarkMap(0, [0]);
    delete (markMap as any).batchToCheckpoint;

    const newMarkMap = startNewBatchWithCheckpoint(markMap, 5, "cp-1");
    expect(newMarkMap.batchToCheckpoint).toBeDefined();
  });

  it("should update memory range correctly", () => {
    const markMap = createMockMarkMap(
      0,
      [0],
      Array.from({ length: 10 }, (_, i) => i),
    );
    const newMarkMap = startNewBatchWithCheckpoint(markMap, 5, "cp-1");

    expect(newMarkMap.memoryRange).toBeDefined();
    expect(newMarkMap.memoryRange!.startBatch).toBe(0);
    expect(newMarkMap.memoryRange!.endBatch).toBe(1);
  });
});

describe("rollbackToBatch", () => {
  it("should rollback to specified batch", () => {
    const markMap = createMockMarkMap(2, [0, 5, 10]);
    const newMarkMap = rollbackToBatch(markMap, 1);

    expect(newMarkMap.currentBatch).toBe(1);
    expect(newMarkMap.batchBoundaries).toHaveLength(2);
    expect(newMarkMap.boundaryToBatch).toHaveLength(2);
  });

  it("should throw error for non-existent batch", () => {
    const markMap = createMockMarkMap(1, [0, 5]);
    expect(() => rollbackToBatch(markMap, 5)).toThrow();
  });
});

describe("mergeBatches", () => {
  it("should merge batches correctly", () => {
    const markMap = createMockMarkMap(3, [0, 5, 10, 15]);
    const newMarkMap = mergeBatches(markMap, 0, 2);

    expect(newMarkMap.batchBoundaries).toHaveLength(2);
    expect(newMarkMap.boundaryToBatch).toHaveLength(2);
  });

  it("should throw error for invalid batch range", () => {
    const markMap = createMockMarkMap(2, [0, 5, 10]);
    expect(() => mergeBatches(markMap, 1, 1)).toThrow();
    expect(() => mergeBatches(markMap, 2, 1)).toThrow();
  });
});

describe("getBatchInfo", () => {
  it("should return correct batch info", () => {
    const markMap = createMockMarkMap(
      1,
      [0, 5],
      Array.from({ length: 10 }, (_, i) => i),
    );
    const info = getBatchInfo(markMap, 0);

    expect(info.boundaryIndex).toBe(0);
    expect(info.visibleMessageCount).toBe(10);
    expect(info.isCurrentBatch).toBe(false);
  });

  it("should identify current batch correctly", () => {
    const markMap = createMockMarkMap(
      1,
      [0, 5],
      Array.from({ length: 10 }, (_, i) => i),
    );
    const info = getBatchInfo(markMap, 1);

    expect(info.isCurrentBatch).toBe(true);
  });
});

describe("getAllBatchesInfo", () => {
  it("should return info for all batches", () => {
    const markMap = createMockMarkMap(
      2,
      [0, 5, 10],
      Array.from({ length: 15 }, (_, i) => i),
    );
    const infos = getAllBatchesInfo(markMap);

    expect(infos).toHaveLength(3);
    expect(infos[0]!.batchId).toBe(0);
    expect(infos[1]!.batchId).toBe(1);
    expect(infos[2]!.batchId).toBe(2);
  });
});

describe("getBatchesToRelease", () => {
  it("should return batches that can be released", () => {
    const batchToCheckpoint: BatchCheckpointInfo[] = [
      { batchId: 0, checkpointId: "cp-0", messageCount: 5 },
      { batchId: 1, checkpointId: "cp-1", messageCount: 5 },
      { batchId: 2, checkpointId: null, messageCount: 5 },
      { batchId: 3, checkpointId: null, messageCount: 5 },
    ];
    const markMap = createMockMarkMap(3, [0, 5, 10, 15], [], batchToCheckpoint);

    const toRelease = getBatchesToRelease(markMap, 2);

    expect(toRelease).toContain(0);
    expect(toRelease).not.toContain(2);
    expect(toRelease).not.toContain(3);
  });

  it("should not release batches without checkpoint", () => {
    const batchToCheckpoint: BatchCheckpointInfo[] = [
      { batchId: 0, checkpointId: null, messageCount: 5 },
      { batchId: 1, checkpointId: "cp-1", messageCount: 5 },
    ];
    const markMap = createMockMarkMap(1, [0, 5], [], batchToCheckpoint);

    const toRelease = getBatchesToRelease(markMap, 1);

    expect(toRelease).not.toContain(0);
  });

  it("should return empty array if no batchToCheckpoint", () => {
    const markMap = createMockMarkMap(1, [0, 5]);
    const toRelease = getBatchesToRelease(markMap, 1);

    expect(toRelease).toHaveLength(0);
  });
});

describe("updateBatchCheckpoint", () => {
  it("should update checkpoint ID for batch", () => {
    const batchToCheckpoint: BatchCheckpointInfo[] = [
      { batchId: 0, checkpointId: null, messageCount: 5 },
    ];
    const markMap = createMockMarkMap(0, [0], [], batchToCheckpoint);

    const newMarkMap = updateBatchCheckpoint(markMap, 0, "new-cp");

    expect(newMarkMap.batchToCheckpoint![0]!.checkpointId).toBe("new-cp");
    expect(newMarkMap.batchToCheckpoint![0]!.timestamp).toBeDefined();
  });

  it("should throw error if batchToCheckpoint not initialized", () => {
    const markMap = createMockMarkMap(0, [0]);
    expect(() => updateBatchCheckpoint(markMap, 0, "cp")).toThrow();
  });

  it("should throw error if batch not found", () => {
    const batchToCheckpoint: BatchCheckpointInfo[] = [
      { batchId: 0, checkpointId: null, messageCount: 5 },
    ];
    const markMap = createMockMarkMap(0, [0], [], batchToCheckpoint);

    expect(() => updateBatchCheckpoint(markMap, 5, "cp")).toThrow();
  });
});

describe("getBatchCheckpointId", () => {
  it("should return checkpoint ID for batch", () => {
    const batchToCheckpoint: BatchCheckpointInfo[] = [
      { batchId: 0, checkpointId: "cp-0", messageCount: 5 },
    ];
    const markMap = createMockMarkMap(0, [0], [], batchToCheckpoint);

    expect(getBatchCheckpointId(markMap, 0)).toBe("cp-0");
  });

  it("should return null for batch with no checkpoint", () => {
    const batchToCheckpoint: BatchCheckpointInfo[] = [
      { batchId: 0, checkpointId: null, messageCount: 5 },
    ];
    const markMap = createMockMarkMap(0, [0], [], batchToCheckpoint);

    expect(getBatchCheckpointId(markMap, 0)).toBeNull();
  });

  it("should return undefined if batchToCheckpoint not exists", () => {
    const markMap = createMockMarkMap(0, [0]);
    expect(getBatchCheckpointId(markMap, 0)).toBeUndefined();
  });
});

describe("isBatchInMemory", () => {
  it("should return true for batch in memory", () => {
    const batchToCheckpoint: BatchCheckpointInfo[] = [
      { batchId: 0, checkpointId: null, messageCount: 5 },
    ];
    const markMap = createMockMarkMap(0, [0], [], batchToCheckpoint);

    expect(isBatchInMemory(markMap, 0)).toBe(true);
  });

  it("should return false for batch with checkpoint", () => {
    const batchToCheckpoint: BatchCheckpointInfo[] = [
      { batchId: 0, checkpointId: "cp-0", messageCount: 5 },
    ];
    const markMap = createMockMarkMap(0, [0], [], batchToCheckpoint);

    expect(isBatchInMemory(markMap, 0)).toBe(false);
  });

  it("should return true if batchToCheckpoint not exists", () => {
    const markMap = createMockMarkMap(0, [0]);
    expect(isBatchInMemory(markMap, 0)).toBe(true);
  });

  it("should return true for unknown batch", () => {
    const batchToCheckpoint: BatchCheckpointInfo[] = [
      { batchId: 0, checkpointId: "cp-0", messageCount: 5 },
    ];
    const markMap = createMockMarkMap(0, [0], [], batchToCheckpoint);

    expect(isBatchInMemory(markMap, 5)).toBe(true);
  });
});

describe("rebuildIndicesAfterRelease", () => {
  it("should rebuild indices after releasing batches", () => {
    const markMap = createMockMarkMap(
      2,
      [0, 5, 10],
      Array.from({ length: 15 }, (_, i) => i),
    );

    const newMarkMap = rebuildIndicesAfterRelease(markMap, [0]);

    expect(newMarkMap.originalIndices).toHaveLength(10);
    expect(newMarkMap.originalIndices[0]).toBe(0);
  });

  it("should return same markMap if no batches to release", () => {
    const markMap = createMockMarkMap(1, [0, 5], [0, 1, 2, 3, 4]);
    const newMarkMap = rebuildIndicesAfterRelease(markMap, []);

    expect(newMarkMap.originalIndices).toEqual(markMap.originalIndices);
  });
});
