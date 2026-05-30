/**
 * Persistence E2E Test: Checkpoint Storage
 *
 * Tests cross-lifecycle persistence and serialization integrity using SQLite.
 * - Cross-lifecycle: save → close → reopen → verify data survives
 * - Serialization integrity: complex metadata, large binary, Unicode, deep objects
 */
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as fs from "fs/promises";
import * as path from "path";
import * as os from "os";
import { SqliteCheckpointStorage } from "@wf-agent/storage";
import type { CheckpointStorageAdapter } from "@wf-agent/storage";
import type { CheckpointStorageMetadata } from "@wf-agent/types";

let tempDir: string;
let dbPath: string;

const createMetadata = (
  overrides: Partial<CheckpointStorageMetadata> = {},
): CheckpointStorageMetadata => ({
  entityType: "workflow",
  entityId: "wf-test",
  timestamp: Date.now(),
  ...overrides,
});

async function createStorage(verifyIntegrity = false): Promise<CheckpointStorageAdapter> {
  const storage = new SqliteCheckpointStorage({
    dbPath,
    verifyIntegrity,
    useConnectionPool: false,
  }) as unknown as CheckpointStorageAdapter;
  await storage.initialize();
  return storage;
}

describe("Checkpoint Storage Persistence (SQLite)", () => {
  beforeEach(async () => {
    tempDir = await fs.mkdtemp(path.join(os.tmpdir(), "cp-persist-"));
    dbPath = path.join(tempDir, "checkpoints.db");
  });

  afterEach(async () => {
    try {
      await fs.rm(tempDir, { recursive: true, force: true });
    } catch {
      // ignore cleanup errors
    }
  });

  describe("Cross-Lifecycle Persistence", () => {
    it("should persist data across close/reopen lifecycle", async () => {
      // Phase 1: save data
      let storage = await createStorage();
      const data = new Uint8Array([1, 2, 3, 4, 5]);
      const metadata = createMetadata({ entityId: "wf-1", tags: ["prod"] });
      await storage.save("cp-1", data, metadata);
      await storage.save("cp-2", new Uint8Array([10, 20]), createMetadata({ entityId: "wf-1", timestamp: metadata.timestamp + 100 }));
      await storage.close();

      // Phase 2: reopen and verify
      storage = await createStorage();
      const loaded = await storage.load("cp-1");
      expect(loaded).not.toBeNull();
      expect(Array.from(loaded!)).toEqual([1, 2, 3, 4, 5]);

      const metadataLoaded = await storage.getMetadata("cp-1");
      expect(metadataLoaded).not.toBeNull();
      expect(metadataLoaded!.entityId).toBe("wf-1");
      expect(metadataLoaded!.tags).toEqual(["prod"]);

      const allIds = await storage.list();
      expect(allIds.sort()).toEqual(["cp-1", "cp-2"]);

      await storage.close();
    });

    it("should persist data after multiple write cycles", async () => {
      let storage = await createStorage();

      // Write cycle 1
      await storage.save("cp-cycle", new Uint8Array([100]), createMetadata({ entityId: "wf-1" }));
      await storage.close();

      // Write cycle 2: reopen and write more
      storage = await createStorage();
      await storage.save("cp-cycle-2", new Uint8Array([200]), createMetadata({ entityId: "wf-2" }));
      await storage.close();

      // Verify both survive
      storage = await createStorage();
      expect(await storage.exists("cp-cycle")).toBe(true);
      expect(await storage.exists("cp-cycle-2")).toBe(true);
      const list = await storage.list();
      expect(list.length).toBe(2);
      await storage.close();
    });

    it("should persist and retrieve after delete + close", async () => {
      let storage = await createStorage();
      await storage.save("cp-del", new Uint8Array([99]), createMetadata({ entityId: "wf-1" }));
      await storage.save("cp-keep", new Uint8Array([88]), createMetadata({ entityId: "wf-2" }));
      await storage.delete("cp-del");
      await storage.close();

      // Reopen and verify deletion persisted
      storage = await createStorage();
      expect(await storage.exists("cp-del")).toBe(false);
      expect(await storage.exists("cp-keep")).toBe(true);
      await storage.close();
    });
  });

  describe("Serialization Integrity", () => {
    it("should preserve complex customFields through persistence", async () => {
      let storage = await createStorage();
      const complexFields: Record<string, unknown> = {
        nested: { a: 1, b: [2, 3, { c: "deep" }] },
        boolean: true,
        nullValue: null,
        arrayOfObjects: [{ x: 1 }, { y: 2 }],
        numberPrecision: 0.123456789,
      };
      const metadata = createMetadata({
        entityId: "wf-complex",
        customFields: complexFields,
        tags: ["a", "b", "c"],
      });
      await storage.save("cp-complex", new Uint8Array([1, 2]), metadata);
      await storage.close();

      // Reopen and verify
      storage = await createStorage();
      const loadedMeta = await storage.getMetadata("cp-complex");
      expect(loadedMeta).not.toBeNull();
      expect(loadedMeta!.customFields).toEqual(complexFields);
      expect(loadedMeta!.tags).toEqual(["a", "b", "c"]);
      await storage.close();
    });

    it("should handle large binary data (1MB) correctly", async () => {
      let storage = await createStorage();
      const largeData = new Uint8Array(1024 * 1024);
      for (let i = 0; i < largeData.length; i++) {
        largeData[i] = i & 0xff;
      }
      const metadata = createMetadata({ entityId: "wf-large" });
      await storage.save("cp-large", largeData, metadata);
      await storage.close();

      // Reopen and verify
      storage = await createStorage();
      const loaded = await storage.load("cp-large");
      expect(loaded).not.toBeNull();
      expect(loaded!.length).toBe(1024 * 1024);
      // Spot-check a few positions
      expect(loaded![0]).toBe(0);
      expect(loaded![255]).toBe(255);
      expect(loaded![256]).toBe(0);
      expect(loaded![loaded!.length - 1]).toBe((loaded!.length - 1) & 0xff);
      await storage.close();
    });

    it("should handle binary data with all byte values (0x00-0xFF)", async () => {
      let storage = await createStorage();
      const allBytes = new Uint8Array(256);
      for (let i = 0; i < 256; i++) {
        allBytes[i] = i;
      }
      await storage.save("cp-bytes", allBytes, createMetadata({ entityId: "wf-bytes" }));
      await storage.close();

      storage = await createStorage();
      const loaded = await storage.load("cp-bytes");
      expect(loaded).not.toBeNull();
      // Verify every single byte
      for (let i = 0; i < 256; i++) {
        expect(loaded![i]).toBe(i);
      }
      await storage.close();
    });

    it("should handle Unicode metadata (Chinese, emoji, special chars)", async () => {
      let storage = await createStorage();
      const unicodeFields: Record<string, unknown> = {
        chinese: "你好世界",
        japanese: "こんにちは",
        emoji: "🚀🎉✅",
        special: "~!@#$%^&*()_+{}|:\"<>?`-=[];',./",
        mixed: "Hello 你好 🌍",
      };
      const metadata = createMetadata({
        entityId: "wf-unicode",
        customFields: unicodeFields,
        tags: ["标签1", "标签2", "🚀"],
      });
      await storage.save("cp-unicode", new Uint8Array([1]), metadata);
      await storage.close();

      storage = await createStorage();
      const loadedMeta = await storage.getMetadata("cp-unicode");
      expect(loadedMeta).not.toBeNull();
      expect(loadedMeta!.customFields).toEqual(unicodeFields);
      expect(loadedMeta!.tags).toEqual(["标签1", "标签2", "🚀"]);
      await storage.close();
    });

    it("should preserve timestamp precision through persistence", async () => {
      let storage = await createStorage();
      const preciseTime = 1712345678901;
      const metadata = createMetadata({
        entityId: "wf-time",
        timestamp: preciseTime,
      });
      await storage.save("cp-time", new Uint8Array([1]), metadata);
      await storage.close();

      storage = await createStorage();
      const loadedMeta = await storage.getMetadata("cp-time");
      expect(loadedMeta).not.toBeNull();
      expect(loadedMeta!.timestamp).toBe(preciseTime);
      await storage.close();
    });

    it("should handle multiple entities and listWithMetadata filters", async () => {
      let storage = await createStorage();
      const baseTime = Date.now();
      const entities = [
        { id: "cp-e1", entityId: "wf-1", entityType: "workflow" as const, timestamp: baseTime, tags: ["prod"] },
        { id: "cp-e2", entityId: "wf-1", entityType: "workflow" as const, timestamp: baseTime + 100, tags: ["prod", "backup"] },
        { id: "cp-e3", entityId: "wf-2", entityType: "workflow" as const, timestamp: baseTime + 200, tags: ["dev"] },
        { id: "cp-e4", entityId: "agent-1", entityType: "agent" as const, timestamp: baseTime + 300 },
      ];
      for (const e of entities) {
        await storage.save(e.id, new Uint8Array([1]), createMetadata(e));
      }
      await storage.close();

      storage = await createStorage();

      // Filter by entity
      const wf1Checkpoints = await storage.listWithMetadata({ entityType: "workflow", entityId: "wf-1" });
      expect(wf1Checkpoints.length).toBe(2);
      wf1Checkpoints.forEach(cp => {
        expect(cp.metadata.entityId).toBe("wf-1");
      });

      // Filter by tags (AND logic)
      const prodBackup = await storage.listWithMetadata({ tags: ["prod", "backup"] });
      expect(prodBackup.length).toBe(1);
      expect(prodBackup[0]!.id).toBe("cp-e2");

      // Sort by timestamp and pagination
      const sorted = await storage.listWithMetadata({ entityType: "workflow", sortBy: "timestamp", sortOrder: "asc" });
      expect(sorted[0]!.id).toBe("cp-e1");
      expect(sorted[sorted.length - 1]!.id).toBe("cp-e3");

      // Pagination
      const paged = await storage.listWithMetadata({ entityType: "workflow", limit: 1, offset: 0 });
      expect(paged.length).toBe(1);

      await storage.close();
    });
  });

  describe("Integrity Verification", () => {
    it("should pass integrity check on verified storage", async () => {
      let storage = await createStorage(true);
      const data = new Uint8Array([1, 2, 3, 4, 5]);
      await storage.save("cp-verify", data, createMetadata({ entityId: "wf-verify" }));
      await storage.close();

      storage = await createStorage(true);
      const loaded = await storage.load("cp-verify");
      expect(loaded).not.toBeNull();
      expect(Array.from(loaded!)).toEqual([1, 2, 3, 4, 5]);
      await storage.close();
    });
  });
});
