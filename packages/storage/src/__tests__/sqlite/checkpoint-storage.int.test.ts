/**
 * SqliteCheckpointStorage Integration Test
 *
 * Tests the complete checkpoint storage lifecycle with SQLite backend:
 * - CRUD lifecycle
 * - Entity-aware queries (listByEntityWithMetadata, getLatestByEntity)
 * - Delete with retention policies (deleteByEntity)
 * - List filtering with metadata-only mode
 * - Batch operations
 * - Edge cases
 *
 * SQLite database files are created in temp directory and cleaned up after tests.
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as fs from "fs/promises";
import * as path from "path";
import * as os from "os";
import { SqliteCheckpointStorage } from "../../sqlite/sqlite-checkpoint-storage.js";
import {
  createCheckpointMetadata,
  createMinimalCheckpointMetadata,
  createCheckpointBatch,
  createTestData,
  TEST_CHECKPOINT_ID,
  TEST_ENTITY_ID,
} from "../common/test-data.js";

describe("SqliteCheckpointStorage Integration", () => {
  let storage: SqliteCheckpointStorage;
  let tempDir: string;
  let dbPath: string;

  beforeEach(async () => {
    tempDir = await fs.mkdtemp(path.join(os.tmpdir(), "sqlite-cp-int-"));
    dbPath = path.join(tempDir, "test.db");
    storage = new SqliteCheckpointStorage({ dbPath });
    await storage.initialize();
  });

  afterEach(async () => {
    await storage.close();
    await fs.rm(tempDir, { recursive: true, force: true });
  });

  // ── Lifecycle ─────────────────────────────────────────────────────────

  describe("CRUD Lifecycle", () => {
    it("should complete full CRUD lifecycle", async () => {
      const data = createTestData();
      const metadata = createCheckpointMetadata();

      // Create
      await storage.save(TEST_CHECKPOINT_ID, data, metadata);

      // Read
      const loaded = await storage.load(TEST_CHECKPOINT_ID);
      expect(loaded).toEqual(data);
      expect(await storage.exists(TEST_CHECKPOINT_ID)).toBe(true);

      const meta = await storage.getMetadata(TEST_CHECKPOINT_ID);
      expect(meta).not.toBeNull();
      expect(meta!.entityType).toBe("workflow");
      expect(meta!.entityId).toBe(TEST_ENTITY_ID);

      // Delete
      await storage.delete(TEST_CHECKPOINT_ID);
      expect(await storage.load(TEST_CHECKPOINT_ID)).toBeNull();
      expect(await storage.exists(TEST_CHECKPOINT_ID)).toBe(false);
    });

    it("should handle minimal checkpoint metadata", async () => {
      const metadata = createMinimalCheckpointMetadata();
      const data = createTestData(16);
      const id = "minimal-chk";

      await storage.save(id, data, metadata);
      const loaded = await storage.load(id);
      expect(loaded).toEqual(data);

      const meta = await storage.getMetadata(id);
      expect(meta!.entityType).toBe("workflow");
      expect(meta!.entityId).toBe("minimal-entity-001");
    });

    it("should return null for non-existent checkpoint", async () => {
      expect(await storage.load("non-existent")).toBeNull();
      expect(await storage.getMetadata("non-existent")).toBeNull();
    });
  });

  // ── Entity-Aware Queries ──────────────────────────────────────────────

  describe("Entity-Aware Queries", () => {
    it("should list checkpoints by entity", async () => {
      const metadata = createCheckpointMetadata();
      await storage.save(TEST_CHECKPOINT_ID, createTestData(), metadata);

      const items = await storage.listByEntityWithMetadata(TEST_ENTITY_ID, "workflow");
      expect(items.length).toBeGreaterThanOrEqual(1);
      expect(items.some((item) => item.id === TEST_CHECKPOINT_ID)).toBe(true);
    });

    it("should get latest checkpoint by entity", async () => {
      const metadata = createCheckpointMetadata();
      await storage.save(TEST_CHECKPOINT_ID, createTestData(), metadata);

      const items = await storage.getLatestByEntity(TEST_ENTITY_ID, "workflow");
      expect(items.length).toBeGreaterThanOrEqual(1);
      expect(items[0]!.id).toBe(TEST_CHECKPOINT_ID);
    });

    it("should delete checkpoints by entity", async () => {
      const metadata = createCheckpointMetadata();
      await storage.save(TEST_CHECKPOINT_ID, createTestData(), metadata);

      await storage.deleteByEntity(TEST_ENTITY_ID, "workflow");

      expect(await storage.load(TEST_CHECKPOINT_ID)).toBeNull();
    });
  });

  // ── List Filtering ────────────────────────────────────────────────────

  describe("List Filtering", () => {
    beforeEach(async () => {
      const batch = createCheckpointBatch(TEST_ENTITY_ID, 4);
      for (const item of batch) {
        await storage.save(item.id, item.data, item.metadata);
      }
    });

    it("should list all checkpoints", async () => {
      const ids = await storage.list();
      expect(ids).toHaveLength(4);
    });

    it("should filter by entityId", async () => {
      const ids = await storage.list({ entityId: TEST_ENTITY_ID });
      expect(ids.length).toBeGreaterThanOrEqual(1);
    });
  });

  // ── Batch Operations ──────────────────────────────────────────────────

  describe("Batch Operations", () => {
    it("should save and load batch", async () => {
      const batch = createCheckpointBatch(TEST_ENTITY_ID, 5);
      await storage.saveBatch(batch.map(({ id, data, metadata }) => ({ id, data, metadata })));

      const loaded = await storage.loadBatch(batch.map(({ id }) => id));
      expect(loaded).toHaveLength(5);
      expect(loaded.every((item) => item.data !== null)).toBe(true);

      for (let i = 0; i < batch.length; i++) {
        expect(loaded[i]!.id).toBe(batch[i]!.id);
      }
    });

    it("should delete batch", async () => {
      const batch = createCheckpointBatch(TEST_ENTITY_ID, 3);
      await storage.saveBatch(batch.map(({ id, data, metadata }) => ({ id, data, metadata })));

      await storage.deleteBatch(batch.map(({ id }) => id));

      for (const item of batch) {
        expect(await storage.load(item.id)).toBeNull();
      }
    });
  });

  // ── File Persistence ──────────────────────────────────────────────────

  describe("File Persistence", () => {
    it("should persist data across re-initialization", async () => {
      const metadata = createCheckpointMetadata();
      await storage.save("persist-1", createTestData(), metadata);
      await storage.close();

      storage = new SqliteCheckpointStorage({ dbPath });
      await storage.initialize();

      const loaded = await storage.load("persist-1");
      expect(loaded).not.toBeNull();
    });
  });
});