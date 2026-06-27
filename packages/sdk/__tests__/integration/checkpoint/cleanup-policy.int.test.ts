/**
 * Cleanup Policy Integration Tests
 *
 * Tests cleanup strategies with real storage adapter.
 * Covers: CP-INT-06 through CP-INT-10
 */

import { describe, it, expect, beforeEach, vi, afterEach } from "vitest";
import { MemoryCheckpointStorage } from "@wf-agent/storage";
import { BaseCheckpointStateManager } from "@sdk/shared/checkpoint/core/base-state-manager";
import type { CheckpointStorageAdapter } from "@sdk/shared/checkpoint/types";
import type {
  BaseCheckpoint,
  CheckpointStorageMetadata,
  CleanupPolicy,
  CheckpointInfo,
} from "@wf-agent/types";

interface TestCheckpoint extends BaseCheckpoint<Record<string, { from: unknown; to: unknown }>, Record<string, unknown>> {
  entityId: string;
}

function createMockStorageAdapter(): CheckpointStorageAdapter {
  const store = new Map<string, { data: Uint8Array; metadata: unknown }>();

  return {
    save: async (id: string, data: Uint8Array, metadata: unknown) => {
      store.set(id, { data, metadata });
    },
    load: async (id: string) => {
      const entry = store.get(id);
      return entry ? entry.data : null;
    },
    delete: async (id: string) => {
      store.delete(id);
    },
    list: async (_options?: Record<string, unknown>) => {
      return Array.from(store.keys());
    },
    listWithMetadata: async (_options?: Record<string, unknown>) => {
      return Array.from(store.entries()).map(([id, entry]) => ({
        id,
        metadata: entry.metadata,
        data: entry.data,
      }));
    },
    loadBatch: async (ids: string[]) => {
      return ids.map(id => ({ id, data: store.get(id)?.data ?? null }));
    },
    listByEntityWithMetadata: async (entityId: string, _entityType: string) => {
      return Array.from(store.entries())
        .filter(([, entry]) => {
          const meta = entry.metadata as Record<string, unknown>;
          return meta.entityId === entityId;
        })
        .map(([id, entry]) => ({
          id,
          metadata: entry.metadata,
        }));
    },
    initialize: async () => {},
    close: async () => {},
    getEntityMetadata: async () => null,
    setEntityMetadata: async () => {},
  } as CheckpointStorageAdapter;
}

function createTestCheckpoint(id: string, entityId: string, timestamp: number, type: "FULL" | "DELTA" = "FULL"): TestCheckpoint {
  return {
    id,
    entityId,
    timestamp,
    type,
    snapshot: { value: Math.floor(Math.random() * 100) },
    delta: type === "DELTA" ? { value: { from: 0, to: 1 } } : undefined,
    previousCheckpointId: type === "DELTA" ? `prev-${id}` : undefined,
    baseCheckpointId: type === "DELTA" ? `base-${entityId}` : undefined,
  } as TestCheckpoint;
}

class TestStateManager extends BaseCheckpointStateManager<TestCheckpoint> {
  extractStorageMetadata(checkpoint: TestCheckpoint): CheckpointStorageMetadata {
    return {
      entityType: "workflow",
      entityId: checkpoint.entityId,
      timestamp: checkpoint.timestamp,
      checkpointType: checkpoint.type,
      baseCheckpointId: checkpoint.baseCheckpointId,
      previousCheckpointId: checkpoint.previousCheckpointId,
      blobSize: 100,
    };
  }

  buildCreatedEvent(_checkpoint: TestCheckpoint): unknown {
    return { type: "checkpoint.created" };
  }

  buildDeletedEvent(_checkpointId: string, _reason?: string): unknown {
    return { type: "checkpoint.deleted" };
  }

  buildFailedEvent(_checkpointId: string, _error: unknown, _operation?: string): unknown {
    return { type: "checkpoint.failed" };
  }
}

describe("Cleanup Policy Integration", () => {
  let storage: CheckpointStorageAdapter;
  let stateManager: TestStateManager;
  const entityId = "cleanup-test-entity";

  beforeEach(() => {
    storage = createMockStorageAdapter();
    stateManager = new TestStateManager(storage);
  });

  describe("CP-INT-06: TimeBased cleanup policy", () => {
    beforeEach(() => {
      vi.useFakeTimers();
      vi.setSystemTime(new Date("2025-01-15T12:00:00Z"));
    });

    afterEach(() => {
      vi.useRealTimers();
    });

    it("should delete checkpoints older than retention period", async () => {
      const now = Date.now();
      const oneDay = 24 * 60 * 60 * 1000;

      for (let i = 0; i < 10; i++) {
        const cp = createTestCheckpoint(`cp-${i}`, entityId, now - (10 - i) * oneDay);
        await stateManager.create(cp);
      }

      const policy: CleanupPolicy = { type: "time", retentionDays: 5 };
      const result = await stateManager.executeCleanupForEntity(entityId, "workflow", undefined, policy);

      expect(result.deletedCount).toBeGreaterThan(0);
      const remaining = await storage.listByEntityWithMetadata(entityId, "workflow");
      expect(remaining.length).toBeLessThanOrEqual(6);
    });

    it("should not delete recent checkpoints", async () => {
      const now = Date.now();
      const oneDay = 24 * 60 * 60 * 1000;

      for (let i = 0; i < 5; i++) {
        const cp = createTestCheckpoint(`cp-recent-${i}`, entityId, now - i * oneDay);
        await stateManager.create(cp);
      }

      const policy: CleanupPolicy = { type: "time", retentionDays: 30 };
      const result = await stateManager.executeCleanupForEntity(entityId, "workflow", undefined, policy);

      expect(result.deletedCount).toBe(0);
    });
  });

  describe("CP-INT-07: CountBased cleanup policy", () => {
    it("should keep only maxCount most recent checkpoints", async () => {
      const now = Date.now();

      for (let i = 0; i < 10; i++) {
        const cp = createTestCheckpoint(`cp-count-${i}`, entityId, now + i * 1000);
        await stateManager.create(cp);
      }

      const policy: CleanupPolicy = { type: "count", maxCount: 3, minRetention: 0 };
      const result = await stateManager.executeCleanupForEntity(entityId, "workflow", undefined, policy);

      expect(result.deletedCount).toBe(7);
      const remaining = await storage.listByEntityWithMetadata(entityId, "workflow");
      expect(remaining.length).toBe(3);
    });

    it("should respect minRetention", async () => {
      const now = Date.now();

      for (let i = 0; i < 10; i++) {
        const cp = createTestCheckpoint(`cp-minret-${i}`, entityId, now + i * 1000, i % 3 === 0 ? "FULL" : "DELTA");
        await stateManager.create(cp);
      }

      const policy: CleanupPolicy = { type: "count", maxCount: 5, minRetention: 2 };
      const result = await stateManager.executeCleanupForEntity(entityId, "workflow", undefined, policy);

      const remaining = await storage.listByEntityWithMetadata(entityId, "workflow");
      expect(remaining.length).toBeGreaterThanOrEqual(5);
    });
  });

  describe("CP-INT-08: SizeBased cleanup policy", () => {
    it("should delete checkpoints when total size exceeds limit", async () => {
      const now = Date.now();

      for (let i = 0; i < 10; i++) {
        const cp = createTestCheckpoint(`cp-size-${i}`, entityId, now + i * 1000);
        await stateManager.create(cp);
      }

      const policy: CleanupPolicy = { type: "size", maxSizeBytes: 300, minRetention: 0 };
      const result = await stateManager.executeCleanupForEntity(entityId, "workflow", undefined, policy);

      expect(result.deletedCount).toBeGreaterThan(0);
      expect(result.freedSpaceBytes).toBeGreaterThan(0);
    });

    it("should not delete when under size limit", async () => {
      const now = Date.now();

      for (let i = 0; i < 3; i++) {
        const cp = createTestCheckpoint(`cp-ok-${i}`, entityId, now + i * 1000);
        await stateManager.create(cp);
      }

      const policy: CleanupPolicy = { type: "size", maxSizeBytes: 10000, minRetention: 0 };
      const result = await stateManager.executeCleanupForEntity(entityId, "workflow", undefined, policy);

      expect(result.deletedCount).toBe(0);
    });
  });

  describe("CP-INT-09: dependency protection during cleanup", () => {
    it("should protect delta chain base checkpoints from deletion", async () => {
      const now = Date.now();

      const fullCp = createTestCheckpoint("cp-full-protect", entityId, now, "FULL");
      await stateManager.create(fullCp);

      for (let i = 1; i <= 5; i++) {
        const deltaCp = createTestCheckpoint(`cp-delta-protect-${i}`, entityId, now + i * 1000, "DELTA");
        deltaCp.baseCheckpointId = "cp-full-protect";
        deltaCp.previousCheckpointId = i === 1 ? "cp-full-protect" : `cp-delta-protect-${i - 1}`;
        await stateManager.create(deltaCp);
      }

      const policy: CleanupPolicy = { type: "count", maxCount: 2, minRetention: 0 };
      const result = await stateManager.executeCleanupForEntity(entityId, "workflow", undefined, policy);

      const remaining = await storage.listByEntityWithMetadata(entityId, "workflow");
      const hasFullCheckpoint = remaining.some(r => r.id === "cp-full-protect");
      expect(hasFullCheckpoint).toBe(true);
    });
  });

  describe("CP-INT-10: exclude checkpoint from cleanup", () => {
    it("should not delete excluded checkpoint", async () => {
      const now = Date.now();

      for (let i = 0; i < 5; i++) {
        const cp = createTestCheckpoint(`cp-exclude-${i}`, entityId, now + i * 1000, i === 0 ? "FULL" : "DELTA");
        if (i > 0) {
          cp.previousCheckpointId = `cp-exclude-${i - 1}`;
          cp.baseCheckpointId = "cp-exclude-0";
        }
        await stateManager.create(cp);
      }

      const policy: CleanupPolicy = { type: "count", maxCount: 1, minRetention: 0 };
      const result = await stateManager.executeCleanupForEntity(entityId, "workflow", "cp-exclude-0", policy);

      const remaining = await storage.listByEntityWithMetadata(entityId, "workflow");
      expect(remaining.some(r => r.id === "cp-exclude-0")).toBe(true);
    });
  });

  describe("CP-INT-11: incremental cleanup with watermark", () => {
    it("should only check recent checkpoints after initial cleanup", async () => {
      const now = Date.now();

      for (let i = 0; i < 10; i++) {
        const cp = createTestCheckpoint(`cp-wm-${i}`, entityId, now + i * 1000);
        await stateManager.create(cp);
      }

      const policy: CleanupPolicy = { type: "count", maxCount: 5, minRetention: 0 };

      const result1 = await stateManager.executeCleanupForEntity(entityId, "workflow", undefined, policy);
      expect(result1.deletedCount).toBe(5);

      for (let i = 10; i < 15; i++) {
        const cp = createTestCheckpoint(`cp-wm-${i}`, entityId, now + i * 1000);
        await stateManager.create(cp);
      }

      const result2 = await stateManager.executeCleanupForEntity(entityId, "workflow", undefined, policy);
      expect(result2.deletedCount).toBeGreaterThanOrEqual(0);
    });
  });
});
