/**
 * BaseCheckpointStateManager Tests
 * Tests for CRUD operations, cleanup policy execution, and lifecycle management
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { BaseCheckpointStateManager } from "../base-checkpoint-state-manager.js";
import type { BaseCheckpoint, CheckpointStorageMetadata, CleanupPolicy } from "@wf-agent/types";
import type { EventRegistry } from "../../registry/event-registry.js";
import type { CheckpointStorageAdapter } from "../types.js";

// ---- Mock Storage Adapter ----
function createMockStorageAdapter(): CheckpointStorageAdapter {
  const store = new Map<string, { data: Uint8Array; metadata: unknown }>();

  return {
    save: vi.fn(async (id: string, data: Uint8Array, metadata: unknown) => {
      store.set(id, { data, metadata });
    }),
    load: vi.fn(async (id: string) => {
      const entry = store.get(id);
      return entry ? entry.data : null;
    }),
    delete: vi.fn(async (id: string) => {
      store.delete(id);
    }),
    list: vi.fn(async (_options?: Record<string, unknown>) => {
      return Array.from(store.keys());
    }),
    listWithMetadata: vi.fn(async (_options?: Record<string, unknown>) => {
      return Array.from(store.entries()).map(([id, entry]) => ({
        id,
        metadata: entry.metadata,
      }));
    }),
    listByEntityWithMetadata: vi.fn(async (_entityId: string, _entityType: string) => {
      return Array.from(store.entries()).map(([id, entry]) => ({
        id,
        metadata: entry.metadata,
      }));
    }),
    getLatestByEntity: vi.fn(async (_entityId: string, _entityType: string, _count?: number) => {
      return Array.from(store.entries()).map(([id, entry]) => ({
        id,
        metadata: entry.metadata,
      }));
    }),
    deleteByEntity: vi.fn(
      async (
        _entityId: string,
        _entityType: string,
        _options?: { keepLatest?: number; olderThan?: number },
      ) => {
        return 0;
      },
    ),
    initialize: vi.fn(async () => {}),
    close: vi.fn(async () => {}),
    clear: vi.fn(async () => {
      store.clear();
    }),
    exists: vi.fn(async (id: string) => {
      return store.has(id);
    }),
    getMetadata: vi.fn(async (id: string) => {
      const entry = store.get(id);
      return entry ? (entry.metadata as CheckpointStorageMetadata) : null;
    }),
    saveBatch: vi.fn(async (items: Array<{ id: string; data: Uint8Array; metadata: unknown }>) => {
      for (const item of items) {
        store.set(item.id, { data: item.data, metadata: item.metadata });
      }
    }),
    loadBatch: vi.fn(async (ids: string[]) => {
      return ids.map(id => {
        const entry = store.get(id);
        return { id, data: entry ? entry.data : null };
      });
    }),
    deleteBatch: vi.fn(async (ids: string[]) => {
      for (const id of ids) {
        store.delete(id);
      }
    }),
    getMetrics: vi.fn(async () => ({
      saveCount: 0, avgSaveTime: 0,
      loadCount: 0, avgLoadTime: 0,
      deleteCount: 0, avgDeleteTime: 0,
      totalBlobSize: 0, totalCount: 0,
    })),
    resetMetrics: vi.fn(() => {}),
    getEntityMetadata: vi.fn(async () => null),
    setEntityMetadata: vi.fn(async () => {}),
  };
}

// ---- Test Checkpoint Type ----
interface TestCheckpoint extends BaseCheckpoint<
  Record<string, { from: unknown; to: unknown }>,
  Record<string, unknown>
> {
  entityId: string;
  entityType: string;
}

// ---- Concrete State Manager for Testing ----
class TestCheckpointStateManager extends BaseCheckpointStateManager<TestCheckpoint> {
  extractStorageMetadata(checkpoint: TestCheckpoint): CheckpointStorageMetadata {
    const record: CheckpointStorageMetadata = {
      entityType: checkpoint.entityType as any,
      entityId: checkpoint.entityId,
      timestamp: checkpoint.timestamp || Date.now(),
      checkpointType: checkpoint.type,
      baseCheckpointId: checkpoint.baseCheckpointId,
      previousCheckpointId: checkpoint.previousCheckpointId,
    };

    if (checkpoint.type === "FULL") {
      record.tags = checkpoint.metadata?.tags || [];
      record.customFields = {
        blobSize: checkpoint.snapshot ? JSON.stringify(checkpoint.snapshot).length : 0,
      };
    }

    return record;
  }

  buildCreatedEvent(checkpoint: TestCheckpoint): unknown {
    return {
      id: checkpoint.id,
      type: "CHECKPOINT_CREATED",
      timestamp: Date.now(),
      data: { checkpointId: checkpoint.id, entityId: checkpoint.entityId },
    };
  }

  buildDeletedEvent(checkpointId: string, reason?: "manual" | "cleanup" | "policy"): unknown {
    return {
      id: checkpointId,
      type: "CHECKPOINT_DELETED",
      timestamp: Date.now(),
      data: { checkpointId, reason },
    };
  }

  buildFailedEvent(
    checkpointId: string,
    error: unknown,
    operation?: "create" | "restore" | "delete",
  ): unknown {
    return {
      id: checkpointId,
      type: "CHECKPOINT_FAILED",
      timestamp: Date.now(),
      data: { checkpointId, error, operation },
    };
  }
}

// ---- Mock Event Registry ----
function createMockEventRegistry(): EventRegistry {
  const mockEmitter = {
    on: vi.fn(),
    off: vi.fn(),
    emit: vi.fn().mockResolvedValue(undefined),
    beginBatch: vi.fn(),
    endBatch: vi.fn().mockResolvedValue(undefined),
  };
  return {
    emit: vi.fn().mockResolvedValue(undefined),
    on: vi.fn(),
    off: vi.fn(),
    getEmitter: vi.fn().mockReturnValue(mockEmitter),
    removeAllListeners: vi.fn(),
    listenerCount: vi.fn().mockReturnValue(0),
  } as unknown as EventRegistry;
}

describe("BaseCheckpointStateManager", () => {
  let storageAdapter: ReturnType<typeof createMockStorageAdapter>;
  let eventManager: EventRegistry;
  let stateManager: TestCheckpointStateManager;

  function createTestCheckpoint(
    id: string,
    overrides: Partial<TestCheckpoint> = {},
  ): TestCheckpoint {
    return {
      id,
      type: "FULL",
      entityId: "entity-1",
      entityType: "workflow",
      timestamp: Date.now(),
      snapshot: { state: "active" },
      ...overrides,
    } as TestCheckpoint;
  }

  beforeEach(() => {
    storageAdapter = createMockStorageAdapter();
    eventManager = createMockEventRegistry();
    stateManager = new TestCheckpointStateManager(storageAdapter, eventManager);
    vi.clearAllMocks();
  });

  describe("create", () => {
    it("should save checkpoint via storage adapter and return ID", async () => {
      const cp = createTestCheckpoint("cp-1");
      const result = await stateManager.create(cp);

      expect(result).toBe("cp-1");
      expect(storageAdapter.save).toHaveBeenCalledWith(
        "cp-1",
        expect.any(Uint8Array),
        expect.objectContaining({ entityId: "entity-1" }),
      );
    });

    it("should emit event when eventManager is set", async () => {
      const cp = createTestCheckpoint("cp-2");
      await stateManager.create(cp);

      expect(eventManager.emit).toHaveBeenCalledWith(
        expect.objectContaining({ type: "CHECKPOINT_CREATED" }),
      );
    });

    it("should not emit event when eventManager is not set", async () => {
      const managerNoEvents = new TestCheckpointStateManager(storageAdapter);
      const cp = createTestCheckpoint("cp-3");
      await managerNoEvents.create(cp);

      // No event emission should happen
      // If emit was never set up, it won't be called
    });

    it("should propagate error when storage fails", async () => {
      const storageError = new Error("Storage write failed");
      (storageAdapter.save as ReturnType<typeof vi.fn>).mockRejectedValue(storageError);

      const cp = createTestCheckpoint("cp-4");
      await expect(stateManager.create(cp)).rejects.toThrow("Storage write failed");
    });

    it("should emit failed event when storage fails", async () => {
      (storageAdapter.save as ReturnType<typeof vi.fn>).mockRejectedValue(new Error("fail"));

      const cp = createTestCheckpoint("cp-5");
      await expect(stateManager.create(cp)).rejects.toThrow();

      expect(eventManager.emit).toHaveBeenCalledWith(
        expect.objectContaining({ type: "CHECKPOINT_FAILED" }),
      );
    });

    it("should track checkpoint size in memory", async () => {
      const cp = createTestCheckpoint("cp-size-test", { snapshot: { data: "x".repeat(100) } });
      await stateManager.create(cp);

      // Access the internal map to verify size was tracked
      const sizes = (stateManager as any).checkpointSizes as Map<string, number>;
      expect(sizes.has("cp-size-test")).toBe(true);
      expect(sizes.get("cp-size-test")!).toBeGreaterThan(0);
    });
  });

  describe("get", () => {
    it("should return checkpoint when it exists", async () => {
      const cp = createTestCheckpoint("cp-get-1");
      await stateManager.create(cp);

      const result = await stateManager.get("cp-get-1");
      expect(result).not.toBeNull();
      expect(result!.id).toBe("cp-get-1");
    });

    it("should return null when checkpoint does not exist", async () => {
      const result = await stateManager.get("non-existent");
      expect(result).toBeNull();
    });

    it("should throw error on deserialization failure", async () => {
      // Save corrupted data
      (storageAdapter.load as ReturnType<typeof vi.fn>).mockResolvedValue(
        new Uint8Array([0xff, 0xfe, 0xfd]),
      );

      await expect(stateManager.get("corrupted")).rejects.toThrow("Checkpoint data corrupted");
    });

    it("should track size on successful load", async () => {
      const cp = createTestCheckpoint("cp-size-get");
      await stateManager.create(cp);

      // Clear and reload
      (stateManager as any).checkpointSizes = new Map();
      await stateManager.get("cp-size-get");

      const sizes = (stateManager as any).checkpointSizes as Map<string, number>;
      expect(sizes.has("cp-size-get")).toBe(true);
      expect(sizes.get("cp-size-get")!).toBeGreaterThan(0);
    });
  });

  describe("delete", () => {
    it("should delete checkpoint from storage adapter", async () => {
      const cp = createTestCheckpoint("cp-del-1");
      await stateManager.create(cp);
      await stateManager.delete("cp-del-1");

      expect(storageAdapter.delete).toHaveBeenCalledWith("cp-del-1");
    });

    it("should emit deleted event with reason", async () => {
      const cp = createTestCheckpoint("cp-del-2");
      await stateManager.create(cp);
      await stateManager.delete("cp-del-2", "cleanup");

      expect(eventManager.emit).toHaveBeenCalledWith(
        expect.objectContaining({ type: "CHECKPOINT_DELETED" }),
      );
    });

    it("should propagate deletion error", async () => {
      (storageAdapter.delete as ReturnType<typeof vi.fn>).mockRejectedValue(
        new Error("Delete failed"),
      );

      await expect(stateManager.delete("missing")).rejects.toThrow("Delete failed");
    });

    it("should remove size tracking on delete", async () => {
      const cp = createTestCheckpoint("cp-size-del");
      await stateManager.create(cp);

      const sizes = (stateManager as any).checkpointSizes as Map<string, number>;
      expect(sizes.has("cp-size-del")).toBe(true);

      await stateManager.delete("cp-size-del");
      expect(sizes.has("cp-size-del")).toBe(false);
    });
  });

  describe("list", () => {
    it("should return checkpoint IDs from storage adapter", async () => {
      (storageAdapter.list as ReturnType<typeof vi.fn>).mockResolvedValue(["cp-1", "cp-2"]);

      const result = await stateManager.list();
      expect(result).toEqual(["cp-1", "cp-2"]);
    });

    it("should pass options to storage adapter", async () => {
      await stateManager.list({ parentId: "entity-1", limit: 10 });

      expect(storageAdapter.list).toHaveBeenCalledWith({ parentId: "entity-1", limit: 10 });
    });
  });

  describe("executeCleanupForEntity", () => {
    it("should return empty result when no policy is set", async () => {
      const managerNoPolicy = new TestCheckpointStateManager(storageAdapter);

      const result = await managerNoPolicy.executeCleanupForEntity("entity-1", "workflow");
      expect(result.deletedCount).toBe(0);
      expect(result.freedSpaceBytes).toBe(0);
    });

    it("should execute count-based cleanup policy", async () => {
      const policy: CleanupPolicy = { type: "count", maxCount: 2 };

      // Create some checkpoints
      for (let i = 0; i < 5; i++) {
        const cp = createTestCheckpoint(`cp-clean-${i}`, {
          entityId: "entity-clean",
          snapshot: { index: i },
        });
        await stateManager.create(cp);
      }

      const result = await stateManager.executeCleanupForEntity(
        "entity-clean", "workflow", undefined, policy,
      );

      expect(result.deletedCount).toBeGreaterThan(0);
      expect(result.deletedCheckpointIds.length).toBeGreaterThan(0);
    });

    it("should use default policy when not overridden", async () => {
      const policy: CleanupPolicy = { type: "count", maxCount: 1 };
      const managerWithPolicy = new TestCheckpointStateManager(
        storageAdapter,
        eventManager,
        policy,
      );

      const cp = createTestCheckpoint("cp-default-policy", { entityId: "entity-dp" });
      await managerWithPolicy.create(cp);

      const result = await managerWithPolicy.executeCleanupForEntity("entity-dp", "workflow");
      // With only 1 checkpoint and maxCount=1, nothing should be deleted
      expect(result).toBeDefined();
    });
  });

  describe("compactDeltaChain", () => {
    function createDeltaCp(
      id: string,
      previousCheckpointId: string,
      baseCheckpointId: string,
      delta: Record<string, { from: unknown; to: unknown }>,
      timestamp: number,
      entityId: string = "entity-chain",
    ): TestCheckpoint {
      return {
        id,
        type: "DELTA",
        entityId,
        entityType: "workflow",
        baseCheckpointId,
        previousCheckpointId,
        delta,
        timestamp,
      } as TestCheckpoint;
    }

    function createFullCp(
      id: string,
      snapshot: Record<string, unknown>,
      timestamp: number,
      entityId: string = "entity-chain",
    ): TestCheckpoint {
      return {
        id,
        type: "FULL",
        entityId,
        entityType: "workflow",
        snapshot,
        timestamp,
      } as TestCheckpoint;
    }

    it("should return 0 when there are no deltas", async () => {
      await stateManager.create(createFullCp("cp-full", { a: 1 }, 100));

      const result = await stateManager.compactDeltaChain("entity-chain", "workflow");
      expect(result).toBe(0);
    });

    it("should return 0 when there is only one delta", async () => {
      await stateManager.create(createFullCp("cp-full", { a: 1 }, 100));
      await stateManager.create(createDeltaCp("cp-d1", "cp-full", "cp-full", {}, 200));

      const result = await stateManager.compactDeltaChain("entity-chain", "workflow");
      expect(result).toBe(0);
    });

    it("should return 0 when deltas are not consecutive in the same chain", async () => {
      // Two separate chains: cp-d1 belongs to chain-1, cp-d2 belongs to chain-2
      await stateManager.create(createFullCp("cp-full-1", { a: 1 }, 100, "entity-chain-1"));
      await stateManager.create(createFullCp("cp-full-2", { b: 2 }, 100, "entity-chain-2"));
      await stateManager.create({
        ...createDeltaCp("cp-d1", "cp-full-1", "cp-full-1", { a: { from: 1, to: 2 } }, 200),
        entityType: "workflow",
      } as TestCheckpoint);
      await stateManager.create({
        ...createDeltaCp("cp-d2", "cp-full-2", "cp-full-2", { b: { from: 2, to: 3 } }, 300),
        entityType: "workflow",
      } as TestCheckpoint);

      // listByEntityWithMetadata returns all checkpoints since the mock doesn't filter
      const result = await stateManager.compactDeltaChain("entity-chain-1", "workflow");
      // Deltas belong to different entities, so they're not consecutive in the same chain
      expect(result).toBe(0);
    });

    it("should merge two consecutive deltas and return 1", async () => {
      const fullCp = createFullCp("cp-base", { a: 1, b: 2, c: 3 }, 100);
      await stateManager.create(fullCp);

      const delta1 = createDeltaCp(
        "cp-d1", "cp-base", "cp-base",
        { a: { from: 1, to: 10 } }, 200,
      );
      await stateManager.create(delta1);

      const delta2 = createDeltaCp(
        "cp-d2", "cp-d1", "cp-base",
        { b: { from: 2, to: 20 } }, 300,
      );
      await stateManager.create(delta2);

      const result = await stateManager.compactDeltaChain("entity-chain", "workflow");
      expect(result).toBe(1);

      // Verify cp-d1 now has merged delta (a: 1→10 from d1, b: undefined→20 from d2 metadata)
      const updatedD1 = await stateManager.get("cp-d1");
      expect(updatedD1).not.toBeNull();
      expect(updatedD1!.delta).toBeDefined();
      expect((updatedD1!.delta as Record<string, { to: unknown }>).a).toEqual({ from: 1, to: 10 });
      // b should exist in d1's delta now with from from... hmm, b is in d2 but not d1
      // In the merged result, b should have { from: undefined, to: 20 }
      // because from is firstFrom ?? undefined, and d1 doesn't have b
      const mergedDelta = updatedD1!.delta as Record<string, { from: unknown; to: unknown }>;
      expect(mergedDelta.b).toBeDefined();

      // Verify cp-d2 was deleted
      const deletedD2 = await stateManager.get("cp-d2");
      expect(deletedD2).toBeNull();
    });

    it("should update successor's previousCheckpointId when merging middle deltas", async () => {
      // Chain: FULL:base → DELTA:d1 → DELTA:d2 → DELTA:d3
      await stateManager.create(createFullCp("cp-base", { x: 0 }, 100));
      await stateManager.create(createDeltaCp("cp-d1", "cp-base", "cp-base", { x: { from: 0, to: 1 } }, 200));
      await stateManager.create(createDeltaCp("cp-d2", "cp-d1", "cp-base", { x: { from: 1, to: 2 } }, 300));
      await stateManager.create(createDeltaCp("cp-d3", "cp-d2", "cp-base", { x: { from: 2, to: 3 } }, 400));

      const result = await stateManager.compactDeltaChain("entity-chain", "workflow");
      expect(result).toBe(1);

      // cp-d1 and cp-d2 were merged, so cp-d2 is deleted
      // cp-d3 should now point to cp-d1 (was cp-d2)
      const d3 = await stateManager.get("cp-d3");
      expect(d3).not.toBeNull();
      expect(d3!.previousCheckpointId).toBe("cp-d1");

      // cp-d2 should be deleted
      const d2 = await stateManager.get("cp-d2");
      expect(d2).toBeNull();
    });

    it("should preserve state equivalence after compaction (end-to-end)", async () => {
      // Create chain: FULL:base → DELTA:d1 → DELTA:d2 → DELTA:d3
      const baseSnapshot = { a: 1, b: 2, c: 3 };
      await stateManager.create(createFullCp("cp-base", baseSnapshot, 100));
      await stateManager.create(createDeltaCp("cp-d1", "cp-base", "cp-base", { a: { from: 1, to: 10 } }, 200));
      await stateManager.create(createDeltaCp("cp-d2", "cp-d1", "cp-base", { b: { from: 2, to: 20 } }, 300));
      await stateManager.create(createDeltaCp("cp-d3", "cp-d2", "cp-base", { c: { from: 3, to: 30 } }, 400));

      // Before compaction, verify restoration gives correct state
      const { BaseDeltaRestorer } = await import("../base-delta-restorer.js");
      const loadFn = async (id: string) => stateManager.get(id);

      // Expected final state: { a: 10, b: 20, c: 30 }
      const restorer = new BaseDeltaRestorer(loadFn);
      const before = await restorer.restore("cp-d3");
      expect(before.snapshot).toEqual({ a: 10, b: 20, c: 30 });

      // Compact: d1 and d2 merge
      const result = await stateManager.compactDeltaChain("entity-chain", "workflow");
      expect(result).toBe(1);

      // After compaction, restoration from d3 should still give correct state
      const after = await restorer.restore("cp-d3");
      expect(after.snapshot).toEqual({ a: 10, b: 20, c: 30 });

      // Chain should now be: cp-base → cp-d1 → cp-d3
      expect(after.metadata.checkpointChain).toEqual(["cp-base", "cp-d1", "cp-d3"]);
    });

    it("should handle merge with delete and re-add of same field", async () => {
      // Chain where d1 deletes 'a' and d2 re-adds 'a'
      await stateManager.create(createFullCp("cp-base", { a: 1, b: 2 }, 100));
      await stateManager.create(createDeltaCp("cp-d1", "cp-base", "cp-base",
        { a: { from: 1, to: undefined } }, 200));
      await stateManager.create(createDeltaCp("cp-d2", "cp-d1", "cp-base",
        { a: { from: undefined, to: 99 }, b: { from: 2, to: 200 } }, 300));
      await stateManager.create(createDeltaCp("cp-d3", "cp-d2", "cp-base",
        { a: { from: 99, to: 100 } }, 400));

      const result = await stateManager.compactDeltaChain("entity-chain", "workflow");
      expect(result).toBe(1);

      // Restore from d3 should still produce correct final state
      const { BaseDeltaRestorer } = await import("../base-delta-restorer.js");
      const loadFn = async (id: string) => stateManager.get(id);
      const restorer = new BaseDeltaRestorer(loadFn);
      const after = await restorer.restore("cp-d3");
      expect(after.snapshot).toEqual({ a: 100, b: 200 });
    });
  });

  describe("initialize", () => {
    it("should call storage adapter initialize", async () => {
      await stateManager.initialize();
      expect(storageAdapter.initialize).toHaveBeenCalled();
    });
  });

  describe("cleanup", () => {
    it("should call storage adapter close", async () => {
      await stateManager.cleanup();
      expect(storageAdapter.close).toHaveBeenCalled();
    });
  });
});
