import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { MemoryCheckpointStorage } from "@wf-agent/storage";
import type { CheckpointStorageAdapter, CheckpointOptions } from "@wf-agent/storage";
import type { CheckpointStorageMetadata, CheckpointEntityType } from "@wf-agent/types";

const createMetadata = (overrides: Partial<CheckpointStorageMetadata> = {}): CheckpointStorageMetadata => ({
  entityType: "workflow",
  entityId: "wf-test",
  timestamp: Date.now(),
  ...overrides,
});

describe("Checkpoint Storage E2E", () => {
  let storage: CheckpointStorageAdapter;

  beforeEach(async () => {
    storage = new MemoryCheckpointStorage();
    await storage.initialize();
  });

  afterEach(async () => {
    await storage.clear();
    await storage.close();
  });

  describe("Basic CRUD Operations", () => {
    it("should save and load a checkpoint", async () => {
      const data = new Uint8Array([1, 2, 3, 4, 5]);
      const metadata = createMetadata({ entityId: "wf-1" });

      await storage.save("cp-1", data, metadata);
      const loaded = await storage.load("cp-1");

      expect(loaded).not.toBeNull();
      expect(Array.from(loaded!)).toEqual([1, 2, 3, 4, 5]);
    });

    it("should return null when loading non-existent checkpoint", async () => {
      const loaded = await storage.load("non-existent");
      expect(loaded).toBeNull();
    });

    it("should delete a checkpoint", async () => {
      const data = new Uint8Array([1, 2, 3]);
      const metadata = createMetadata({ entityId: "wf-1" });

      await storage.save("cp-1", data, metadata);
      expect(await storage.exists("cp-1")).toBe(true);

      await storage.delete("cp-1");
      expect(await storage.exists("cp-1")).toBe(false);
      expect(await storage.load("cp-1")).toBeNull();
    });

    it("should list all checkpoint IDs", async () => {
      await storage.save("cp-1", new Uint8Array([1]), createMetadata({ entityId: "wf-1" }));
      await storage.save("cp-2", new Uint8Array([2]), createMetadata({ entityId: "wf-1" }));
      await storage.save("cp-3", new Uint8Array([3]), createMetadata({ entityId: "wf-2" }));

      const ids = await storage.list();
      expect(ids.sort()).toEqual(["cp-1", "cp-2", "cp-3"]);
    });

    it("should return empty list when no checkpoints exist", async () => {
      const ids = await storage.list();
      expect(ids).toEqual([]);
    });
  });

  describe("Metadata Operations", () => {
    it("should retrieve metadata for a saved checkpoint", async () => {
      const metadata = createMetadata({
        entityId: "wf-1",
        entityType: "workflow",
        tags: ["critical", "test"],
      });

      await storage.save("cp-1", new Uint8Array([1, 2, 3]), metadata);
      const retrieved = await storage.getMetadata("cp-1");

      expect(retrieved).not.toBeNull();
      expect(retrieved!.entityId).toBe("wf-1");
      expect(retrieved!.entityType).toBe("workflow");
      expect(retrieved!.tags).toEqual(["critical", "test"]);
      expect(retrieved!.timestamp).toBeGreaterThan(0);
    });

    it("should return null for non-existent checkpoint metadata", async () => {
      const metadata = await storage.getMetadata("non-existent");
      expect(metadata).toBeNull();
    });
  });

  describe("Batch Operations", () => {
    it("should save multiple checkpoints in batch", async () => {
      const items = [
        { id: "cp-1", data: new Uint8Array([1]), metadata: createMetadata({ entityId: "wf-1" }) },
        { id: "cp-2", data: new Uint8Array([2]), metadata: createMetadata({ entityId: "wf-1" }) },
        { id: "cp-3", data: new Uint8Array([3]), metadata: createMetadata({ entityId: "wf-2" }) },
      ];

      await storage.saveBatch(items);

      const ids = await storage.list();
      expect(ids.sort()).toEqual(["cp-1", "cp-2", "cp-3"]);
    });

    it("should load multiple checkpoints in batch", async () => {
      await storage.save("cp-1", new Uint8Array([1]), createMetadata({ entityId: "wf-1" }));
      await storage.save("cp-2", new Uint8Array([2]), createMetadata({ entityId: "wf-1" }));
      await storage.save("cp-3", new Uint8Array([3]), createMetadata({ entityId: "wf-2" }));

      const results = await storage.loadBatch(["cp-1", "cp-3", "non-existent"]);

      expect(results).toHaveLength(3);
      expect(results[0].id).toBe("cp-1");
      expect(results[0].data).not.toBeNull();
      expect(results[1].id).toBe("cp-3");
      expect(results[1].data).not.toBeNull();
      expect(results[2].id).toBe("non-existent");
      expect(results[2].data).toBeNull();
    });

    it("should delete multiple checkpoints in batch", async () => {
      await storage.saveBatch([
        { id: "cp-1", data: new Uint8Array([1]), metadata: createMetadata({ entityId: "wf-1" }) },
        { id: "cp-2", data: new Uint8Array([2]), metadata: createMetadata({ entityId: "wf-1" }) },
        { id: "cp-3", data: new Uint8Array([3]), metadata: createMetadata({ entityId: "wf-2" }) },
      ]);

      await storage.deleteBatch(["cp-1", "cp-3"]);

      expect(await storage.exists("cp-1")).toBe(false);
      expect(await storage.exists("cp-2")).toBe(true);
      expect(await storage.exists("cp-3")).toBe(false);
    });
  });

  describe("Entity-Aware Operations (CheckpointSpecific)", () => {
    it("should list checkpoints with metadata by entity", async () => {
      await storage.saveBatch([
        { id: "cp-1", data: new Uint8Array([1]), metadata: createMetadata({ entityId: "wf-1", entityType: "workflow" }) },
        { id: "cp-2", data: new Uint8Array([2]), metadata: createMetadata({ entityId: "wf-1", entityType: "workflow" }) },
        { id: "cp-3", data: new Uint8Array([3]), metadata: createMetadata({ entityId: "wf-2", entityType: "workflow" }) },
        { id: "cp-4", data: new Uint8Array([4]), metadata: createMetadata({ entityId: "agent-1", entityType: "agent" }) },
      ]);

      const wf1Checkpoints = await storage.listByEntityWithMetadata("wf-1", "workflow");
      expect(wf1Checkpoints).toHaveLength(2);
      wf1Checkpoints.forEach(cp => {
        expect(cp.metadata.entityId).toBe("wf-1");
      });
    });

    it("should list all checkpoints with metadata", async () => {
      await storage.saveBatch([
        { id: "cp-1", data: new Uint8Array([1]), metadata: createMetadata({ entityId: "wf-1", tags: ["prod"] }) },
        { id: "cp-2", data: new Uint8Array([2]), metadata: createMetadata({ entityId: "wf-2", tags: ["dev"] }) },
      ]);

      const all = await storage.listWithMetadata();
      expect(all).toHaveLength(2);
      expect(all.find(cp => cp.id === "cp-1")?.metadata.tags).toEqual(["prod"]);
    });

    it("should listWithMetadata with entity filter", async () => {
      await storage.save("cp-1", new Uint8Array([1]), createMetadata({ entityId: "wf-1", entityType: "workflow" }));
      await storage.save("cp-2", new Uint8Array([2]), createMetadata({ entityId: "wf-1", entityType: "workflow", tags: ["backup"] }));
      await storage.save("cp-3", new Uint8Array([3]), createMetadata({ entityId: "wf-2", entityType: "workflow" }));

      const wf1Checkpoints = await storage.listWithMetadata({ entityType: "workflow", entityId: "wf-1" });
      expect(wf1Checkpoints).toHaveLength(2);
      wf1Checkpoints.forEach(cp => {
        expect(cp.metadata.entityId).toBe("wf-1");
      });
    });

    it("should get latest checkpoints by entity", async () => {
      const baseTime = Date.now();
      await storage.saveBatch([
        { id: "cp-1", data: new Uint8Array([1]), metadata: createMetadata({ entityId: "wf-1", entityType: "workflow", timestamp: baseTime + 100 }) },
        { id: "cp-2", data: new Uint8Array([2]), metadata: createMetadata({ entityId: "wf-1", entityType: "workflow", timestamp: baseTime + 200 }) },
        { id: "cp-3", data: new Uint8Array([3]), metadata: createMetadata({ entityId: "wf-1", entityType: "workflow", timestamp: baseTime + 300 }) },
      ]);

      const latest = await storage.getLatestByEntity("wf-1", "workflow", 2, true);
      expect(latest).toHaveLength(2);
      expect(latest[0].data).toBeDefined();
    });

    it("should delete checkpoints by entity", async () => {
      await storage.saveBatch([
        { id: "cp-1", data: new Uint8Array([1]), metadata: createMetadata({ entityId: "wf-1", entityType: "workflow" }) },
        { id: "cp-2", data: new Uint8Array([2]), metadata: createMetadata({ entityId: "wf-1", entityType: "workflow" }) },
        { id: "cp-3", data: new Uint8Array([3]), metadata: createMetadata({ entityId: "wf-2", entityType: "workflow" }) },
      ]);

      const deleted = await storage.deleteByEntity("wf-1", "workflow");
      expect(deleted).toBe(2);
      expect(await storage.exists("cp-1")).toBe(false);
      expect(await storage.exists("cp-2")).toBe(false);
      expect(await storage.exists("cp-3")).toBe(true);
    });

    it("should keep latest N checkpoints when deleting by entity", async () => {
      const baseTime = Date.now();
      await storage.saveBatch([
        { id: "cp-1", data: new Uint8Array([1]), metadata: createMetadata({ entityId: "wf-1", entityType: "workflow", timestamp: baseTime + 100 }) },
        { id: "cp-2", data: new Uint8Array([2]), metadata: createMetadata({ entityId: "wf-1", entityType: "workflow", timestamp: baseTime + 200 }) },
        { id: "cp-3", data: new Uint8Array([3]), metadata: createMetadata({ entityId: "wf-1", entityType: "workflow", timestamp: baseTime + 300 }) },
      ]);

      const deleted = await storage.deleteByEntity("wf-1", "workflow", { keepLatest: 1 });
      expect(deleted).toBe(2);
      expect(await storage.exists("cp-3")).toBe(true);
    });
  });

  describe("Checkpoint Options", () => {
    it("should save checkpoint with sync option", async () => {
      const data = new Uint8Array([1, 2, 3]);
      const metadata = createMetadata({ entityId: "wf-1" });
      const options: CheckpointOptions = { sync: true, syncTimeout: 5000 };

      await expect(
        storage.save("cp-sync", data, metadata, options),
      ).resolves.not.toThrow();
    });
  });

  describe("Metrics", () => {
    it("should track storage metrics after operations", async () => {
      await storage.save("cp-1", new Uint8Array([1]), createMetadata({ entityId: "wf-1" }));
      await storage.save("cp-2", new Uint8Array([2]), createMetadata({ entityId: "wf-1" }));
      await storage.load("cp-1");

      const metrics = await storage.getMetrics();
      expect(metrics.saveCount).toBe(2);
      expect(metrics.loadCount).toBe(1);
      expect(metrics.totalCount).toBe(2);
    });

    it("should reset metrics", async () => {
      await storage.save("cp-1", new Uint8Array([1]), createMetadata({ entityId: "wf-1" }));
      storage.resetMetrics();

      const metrics = await storage.getMetrics();
      expect(metrics.saveCount).toBe(0);
      expect(metrics.totalCount).toBe(1);
    });
  });

  describe("Edge Cases", () => {
    it("should handle empty data", async () => {
      const data = new Uint8Array([]);
      const metadata = createMetadata({ entityId: "wf-1" });

      await storage.save("cp-empty", data, metadata);
      const loaded = await storage.load("cp-empty");

      expect(loaded).not.toBeNull();
      expect(loaded!.length).toBe(0);
    });

    it("should handle large binary data", async () => {
      const largeData = new Uint8Array(1024 * 64);
      for (let i = 0; i < largeData.length; i++) {
        largeData[i] = i % 256;
      }

      const metadata = createMetadata({ entityId: "wf-1" });
      await storage.save("cp-large", largeData, metadata);

      const loaded = await storage.load("cp-large");
      expect(loaded).not.toBeNull();
      expect(loaded!.length).toBe(1024 * 64);
      expect(loaded![0]).toBe(0);
      expect(loaded![255]).toBe(255);
      expect(loaded![256]).toBe(0);
    });

    it("should handle save with tags filter", async () => {
      await storage.save("cp-1", new Uint8Array([1]), createMetadata({
        entityId: "wf-1",
        tags: ["alpha", "beta"],
      }));
      await storage.save("cp-2", new Uint8Array([2]), createMetadata({
        entityId: "wf-1",
        tags: ["beta", "gamma"],
      }));
      await storage.save("cp-3", new Uint8Array([3]), createMetadata({
        entityId: "wf-2",
        tags: ["gamma"],
      }));

      const results = await storage.listWithMetadata({ tags: ["alpha"] });
      expect(results).toHaveLength(1);
      expect(results[0].id).toBe("cp-1");
    });

    it("should return empty listByEntity for non-existent entity", async () => {
      const checkpoints = await storage.listByEntityWithMetadata("non-existent", "workflow");
      expect(checkpoints).toEqual([]);
    });

    it("should return 0 when deleting by non-existent entity", async () => {
      const deleted = await storage.deleteByEntity("non-existent", "workflow");
      expect(deleted).toBe(0);
    });
  });
});