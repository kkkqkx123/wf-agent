import { describe, it, expect, beforeEach, vi } from "vitest";
import { BaseCheckpointStateManager } from "../base-state-manager.js";
import type { BaseCheckpoint, CheckpointStorageMetadata, CleanupPolicy } from "@wf-agent/types";
import type { EventRegistry } from "../../registry/event-registry.js";
import type { CheckpointStorageAdapter } from "../../types.js";

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
        data: entry.data,
      }));
    }),
    loadBatch: vi.fn(async (ids: string[]) => {
      return ids.map(id => ({ id, data: store.get(id)?.data ?? null }));
    }),
    listByEntityWithMetadata: vi.fn(async (_entityId: string, _entityType: string) => {
      return Array.from(store.entries())
        .filter(([id]) => id.startsWith("cp-"))
        .map(([id, entry]) => ({
          id,
          metadata: entry.metadata,
        }));
    }),
    initialize: vi.fn().mockResolvedValue(undefined),
    close: vi.fn().mockResolvedValue(undefined),
    getEntityMetadata: vi.fn().mockResolvedValue(undefined),
    setEntityMetadata: vi.fn().mockResolvedValue(undefined),
  } as CheckpointStorageAdapter;
}

interface TestState {
  name: string;
  value: number;
}

interface TestCheckpoint extends BaseCheckpoint<Record<string, { from: unknown; to: unknown }>, TestState> {
  entityId: string;
}

function createTestCheckpoint(id: string, overrides?: Partial<TestCheckpoint>): TestCheckpoint {
  return {
    id,
    type: "FULL",
    entityId: "entity-1",
    timestamp: Date.now(),
    snapshot: { name: "test", value: 42 },
    ...overrides,
  } as TestCheckpoint;
}

function createTestDeltaCheckpoint(id: string, prevId: string, deltaMap?: Record<string, { from: unknown; to: unknown }>): TestCheckpoint {
  return {
    id,
    type: "DELTA",
    entityId: "entity-chained",
    timestamp: Date.now(),
    baseCheckpointId: "cp-base",
    previousCheckpointId: prevId,
    delta: deltaMap ?? { value: { from: 0, to: 1 } },
  } as TestCheckpoint;
}

class TestStateManager extends BaseCheckpointStateManager<TestCheckpoint> {
  extractStorageMetadata(checkpoint: TestCheckpoint): CheckpointStorageMetadata {
    const base: CheckpointStorageMetadata = {
      entityType: "workflow",
      entityId: checkpoint.entityId,
      timestamp: checkpoint.timestamp,
      checkpointType: checkpoint.type,
      baseCheckpointId: checkpoint.baseCheckpointId,
      previousCheckpointId: checkpoint.previousCheckpointId,
      blobSize: 0,
    };
    return base;
  }

  buildCreatedEvent(checkpoint: TestCheckpoint): unknown {
    return { type: "checkpoint.created", payload: { checkpointId: checkpoint.id } };
  }

  buildDeletedEvent(checkpointId: string, reason?: string): unknown {
    return { type: "checkpoint.deleted", payload: { checkpointId, reason } };
  }

  buildFailedEvent(checkpointId: string, error: unknown, operation?: string): unknown {
    return { type: "checkpoint.failed", payload: { checkpointId, error, operation } };
  }
}

describe("BaseCheckpointStateManager", () => {
  let stateManager: TestStateManager;
  let storageAdapter: CheckpointStorageAdapter;
  let eventManager: EventRegistry;

  beforeEach(() => {
    storageAdapter = createMockStorageAdapter();
    eventManager = {
      emit: vi.fn().mockResolvedValue(undefined),
      getEmitter: vi.fn(() => ({
        beginBatch: vi.fn(),
        endBatch: vi.fn().mockResolvedValue(undefined),
      })),
      on: vi.fn(),
      off: vi.fn(),
      removeAllListeners: vi.fn(),
      destroy: vi.fn().mockResolvedValue(undefined),
    } as unknown as EventRegistry;

    stateManager = new TestStateManager(storageAdapter, eventManager);
  });

  describe("create", () => {
    it("should save checkpoint via storage adapter and return ID", async () => {
      const checkpoint = createTestCheckpoint("cp-1");
      const id = await stateManager.create(checkpoint);
      expect(id).toBe("cp-1");
      expect(storageAdapter.save).toHaveBeenCalledWith("cp-1", expect.any(Uint8Array), expect.any(Object));
    });

    it("should emit event when eventManager is set", async () => {
      const checkpoint = createTestCheckpoint("cp-event");
      await stateManager.create(checkpoint);
      expect(eventManager.emit).toHaveBeenCalled();
    });

    it("should not emit event when eventManager is not set", async () => {
      const stateManagerNoEvent = new TestStateManager(storageAdapter);
      const checkpoint = createTestCheckpoint("cp-no-event");
      await stateManagerNoEvent.create(checkpoint);
    });

    it("should propagate error when storage fails", async () => {
      const mockSave = vi.fn().mockRejectedValue(new Error("Storage write failed"));
      const badAdapter = { ...storageAdapter, save: mockSave };
      const sm = new TestStateManager(badAdapter as CheckpointStorageAdapter);
      const checkpoint = createTestCheckpoint("cp-4");
      await expect(sm.create(checkpoint)).rejects.toThrow("Storage write failed");
    });

    it("should emit failed event when storage fails", async () => {
      const mockSave = vi.fn().mockRejectedValue(new Error("fail"));
      const badAdapter = { ...storageAdapter, save: mockSave };
      const sm = new TestStateManager(badAdapter as CheckpointStorageAdapter, eventManager);
      const checkpoint = createTestCheckpoint("cp-5");
      await expect(sm.create(checkpoint)).rejects.toThrow();
      expect(eventManager.emit).toHaveBeenCalled();
    });

    it("should track checkpoint size in memory", async () => {
      const checkpoint = createTestCheckpoint("cp-size");
      await stateManager.create(checkpoint);
    });
  });

  describe("get", () => {
    it("should return checkpoint when it exists", async () => {
      const checkpoint = createTestCheckpoint("cp-get");
      await stateManager.create(checkpoint);
      const loaded = await stateManager.get("cp-get");
      expect(loaded).not.toBeNull();
      expect(loaded!.id).toBe("cp-get");
    });

    it("should return null when checkpoint does not exist", async () => {
      const loaded = await stateManager.get("non-existent");
      expect(loaded).toBeNull();
    });

    it("should throw error on deserialization failure", async () => {
      const mockAdapter = createMockStorageAdapter();
      mockAdapter.load = vi.fn().mockResolvedValue(new Uint8Array([1, 2, 3]));
      const sm = new TestStateManager(mockAdapter as CheckpointStorageAdapter);
      await expect(sm.get("corrupted")).rejects.toThrow("Checkpoint data corrupted");
    });

    it("should track size on successful load", async () => {
      const checkpoint = createTestCheckpoint("cp-size-track");
      await stateManager.create(checkpoint);
      const loaded = await stateManager.get("cp-size-track");
      expect(loaded).not.toBeNull();
    });
  });

  describe("delete", () => {
    it("should delete checkpoint from storage adapter", async () => {
      await stateManager.delete("cp-del-1");
      expect(storageAdapter.delete).toHaveBeenCalledWith("cp-del-1");
    });

    it("should emit deleted event with reason", async () => {
      await stateManager.delete("cp-del-2", "cleanup");
      expect(eventManager.emit).toHaveBeenCalled();
    });

    it("should propagate deletion error", async () => {
      const badAdapter = createMockStorageAdapter();
      badAdapter.delete = vi.fn().mockRejectedValue(new Error("Delete failed"));
      const sm = new TestStateManager(badAdapter as CheckpointStorageAdapter);
      await expect(sm.delete("missing")).rejects.toThrow("Delete failed");
    });

    it("should remove size tracking on delete", async () => {
      const checkpoint = createTestCheckpoint("cp-size-del");
      await stateManager.create(checkpoint);
      await stateManager.delete("cp-size-del");
    });
  });

  describe("list", () => {
    it("should return checkpoint IDs from storage adapter", async () => {
      (storageAdapter.list as ReturnType<typeof vi.fn>).mockResolvedValue(["cp-1", "cp-2"]);
      const ids = await stateManager.list();
      expect(ids).toEqual(["cp-1", "cp-2"]);
    });

    it("should pass options to storage adapter", async () => {
      await stateManager.list({ parentId: "entity-1", limit: 5 });
      expect(storageAdapter.list).toHaveBeenCalledWith({ parentId: "entity-1", limit: 5 });
    });
  });

  describe("executeCleanupForEntity", () => {
    it("should return empty result when no policy is set", async () => {
      const sm = new TestStateManager(storageAdapter);
      const result = await sm.executeCleanupForEntity("entity-empty", "workflow");
      expect(result.deletedCount).toBe(0);
      expect(result.freedSpaceBytes).toBe(0);
    });

    it("should execute count-based cleanup policy", async () => {
      const policy: CleanupPolicy = { type: "count", maxCount: 2 };
      for (let i = 0; i < 5; i++) {
        const cp = createTestCheckpoint(`cp-clean-${i}`, { entityId: "entity-clean" });
        await stateManager.create(cp);
      }

      const result = await stateManager.executeCleanupForEntity("entity-clean", "workflow", undefined, policy);

      expect(result.deletedCount).toBeGreaterThan(0);
    });

    it("should use default policy when not overridden", async () => {
      const policy: CleanupPolicy = { type: "count", maxCount: 10 };
      const sm = new TestStateManager(storageAdapter, eventManager, policy);

      const cp = createTestCheckpoint("cp-default-policy", { entityId: "entity-dp" });
      await sm.create(cp);

      const result = await sm.executeCleanupForEntity("entity-dp", "workflow");
      expect(result.deletedCount).toBe(0);
    });
  });

  describe("compactDeltaChain", () => {
    it("should return 0 when there are no deltas", async () => {
      const checkpoint = createTestCheckpoint("cp-full");
      await stateManager.create(checkpoint);
      const result = await stateManager.compactDeltaChain("entity-chained", "workflow");
      expect(result).toBe(0);
    });

    it("should return 0 when there is only one delta", async () => {
      await stateManager.create(createTestCheckpoint("cp-full", { entityId: "entity-chained" }));
      await stateManager.create(createTestDeltaCheckpoint("cp-d1", "cp-full"));
      const result = await stateManager.compactDeltaChain("entity-chained", "workflow");
      expect(result).toBe(0);
    });

    it("should return 0 when deltas are not consecutive in the same chain", async () => {
      await stateManager.create(createTestCheckpoint("cp-full-1", { entityId: "entity-chained" }));
      await stateManager.create(createTestCheckpoint("cp-full-2", { entityId: "entity-chained" }));
      await stateManager.create(createTestDeltaCheckpoint("cp-d1", "cp-full-1"));
      await stateManager.create(createTestDeltaCheckpoint("cp-d2", "cp-full-2"));
      const result = await stateManager.compactDeltaChain("entity-chained", "workflow");
      expect(result).toBe(0);
    });

    it("should merge two consecutive deltas and return 1", async () => {
      await stateManager.create(createTestCheckpoint("cp-base", { entityId: "entity-chained" }));
      await stateManager.create(createTestDeltaCheckpoint("cp-d1", "cp-base"));
      await stateManager.create(createTestDeltaCheckpoint("cp-d2", "cp-d1"));

      const result = await stateManager.compactDeltaChain("entity-chained", "workflow");
      expect(result).toBe(1);
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
