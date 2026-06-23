/**
 * JsonCheckpointStorage Integration Test
 *
 * Tests the complete JSON checkpoint storage lifecycle:
 * - CRUD lifecycle with file I/O verification
 * - Entity-aware queries (listByEntityWithMetadata, getLatestByEntity)
 * - Delete with retention policies
 * - List filtering
 * - Batch operations
 * - File persistence across re-initialization
 * - Edge cases
 *
 * Test output directory: packages/storage/src/__tests__/json/output/checkpoint
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as fs from "fs/promises";
import * as path from "path";
import { fileURLToPath } from "url";
import { JsonCheckpointStorage } from "../../json/json-checkpoint-storage.js";
import {
  createCheckpointMetadata,
  createCheckpointOptions,
  createCheckpointBatch,
  createTestData,
  TEST_CHECKPOINT_ID,
} from "../common/test-data.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const TEST_OUTPUT_DIR = path.resolve(__dirname, "output", "checkpoint");

describe("JsonCheckpointStorage Integration", () => {
  let storage: JsonCheckpointStorage;
  let testDir: string;

  beforeEach(async () => {
    testDir = path.join(TEST_OUTPUT_DIR, `test-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`);
    storage = new JsonCheckpointStorage({ baseDir: testDir });
    await storage.initialize();
  });

  afterEach(async () => {
    await storage.close();
    await fs.rm(testDir, { recursive: true, force: true }).catch(() => {});
  });

  // ── Lifecycle ─────────────────────────────────────────────────────────

  describe("CRUD Lifecycle", () => {
    it("should complete full CRUD lifecycle", async () => {
      const data = createTestData();
      const metadata = createCheckpointMetadata();
      const options = createCheckpointOptions();

      // Create
      await storage.save(TEST_CHECKPOINT_ID, data, metadata, options);

      // Read
      const loaded = await storage.load(TEST_CHECKPOINT_ID);
      expect(loaded).toEqual(data);
      expect(await storage.exists(TEST_CHECKPOINT_ID)).toBe(true);

      // Verify file structure
      const metaDir = path.join(testDir, "metadata", "checkpoint");
      const dataDir = path.join(testDir, "data", "checkpoint");
      expect(await fs.readdir(metaDir)).toContain(`${TEST_CHECKPOINT_ID}.json`);
      expect(await fs.readdir(dataDir)).toContain(`${TEST_CHECKPOINT_ID}.bin`);

      // Delete
      await storage.delete(TEST_CHECKPOINT_ID);
      expect(await storage.load(TEST_CHECKPOINT_ID)).toBeNull();
      expect(await storage.exists(TEST_CHECKPOINT_ID)).toBe(false);
    });
  });

  // ── Entity-Aware Queries ──────────────────────────────────────────────

  describe("Entity-Aware Queries", () => {
    beforeEach(async () => {
      const entity1 = createCheckpointBatch("entity-1", 5);
      const entity2 = createCheckpointBatch("entity-2", 3);
      for (const item of [...entity1, ...entity2]) {
        await storage.save(item.id, item.data, item.metadata);
      }
    });

    it("should listWithMetadata filtered by entity", async () => {
      const items = await storage.listWithMetadata({
        entityId: "entity-1",
        entityType: "workflow",
      });
      expect(items).toHaveLength(5);
      for (const item of items) {
        expect(item.metadata.entityId).toBe("entity-1");
      }
    });

    it("should listByEntityWithMetadata", async () => {
      const items = await storage.listByEntityWithMetadata("entity-1", "workflow");
      expect(items).toHaveLength(5);
    });

    it("should listByEntityWithMetadata with pagination", async () => {
      const items = await storage.listByEntityWithMetadata("entity-1", "workflow", {
        limit: 2,
        offset: 0,
      });
      expect(items).toHaveLength(2);
    });

    it("should getLatestByEntity", async () => {
      const items = await storage.getLatestByEntity("entity-1", "workflow", 2, true);
      expect(items).toHaveLength(2);
      for (const item of items) {
        expect(item.data).toBeDefined();
      }
    });

    it("should return empty array for non-existent entity", async () => {
      expect(await storage.listByEntityWithMetadata("non-existent", "workflow")).toEqual([]);
      expect(await storage.getLatestByEntity("non-existent", "workflow")).toEqual([]);
    });
  });

  // ── Delete with Retention ─────────────────────────────────────────────

  describe("Delete with Retention Policies", () => {
    it("should deleteByEntity", async () => {
      const batch = createCheckpointBatch("delete-entity", 3);
      for (const item of batch) {
        await storage.save(item.id, item.data, item.metadata);
      }

      const deleted = await storage.deleteByEntity("delete-entity", "workflow");
      expect(deleted).toBe(3);

      expect(await storage.list({ entityId: "delete-entity" })).toHaveLength(0);
    });

    it("should keepLatest N checkpoints", async () => {
      const batch = createCheckpointBatch("keep-entity", 5);
      for (const item of batch) {
        await storage.save(item.id, item.data, item.metadata);
      }

      const deleted = await storage.deleteByEntity("keep-entity", "workflow", {
        keepLatest: 2,
      });
      expect(deleted).toBe(3);

      const remaining = await storage.list({ entityId: "keep-entity" });
      expect(remaining).toHaveLength(2);
    });
  });

  // ── List Filtering ────────────────────────────────────────────────────

  describe("List Filtering", () => {
    beforeEach(async () => {
      const batch = createCheckpointBatch("list-entity", 5);
      for (const item of batch) {
        await storage.save(item.id, item.data, item.metadata);
      }
    });

    it("should list with entity type filter", async () => {
      const ids = await storage.list({ entityType: "workflow" });
      expect(ids.length).toBeGreaterThan(0);
    });

    it("should list with tag filter", async () => {
      const ids = await storage.list({ tags: ["full"] });
      expect(ids.length).toBeGreaterThan(0);
    });

    it("should support pagination", async () => {
      const page1 = await storage.list({
        entityId: "list-entity",
        limit: 2,
        offset: 0,
      });
      expect(page1).toHaveLength(2);
    });
  });

  // ── Batch Operations ──────────────────────────────────────────────────

  describe("Batch Operations", () => {
    it("should persist batch across re-initialization", async () => {
      const batch = createCheckpointBatch("batch-entity", 5);
      await storage.saveBatch(batch.map(({ id, data, metadata }) => ({ id, data, metadata })));

      await storage.close();
      storage = new JsonCheckpointStorage({ baseDir: testDir });
      await storage.initialize();

      expect(await storage.list({ entityId: "batch-entity" })).toHaveLength(5);
    });

    it("should delete batch", async () => {
      const batch = createCheckpointBatch("batch-entity", 3);
      await storage.saveBatch(batch.map(({ id, data, metadata }) => ({ id, data, metadata })));

      await storage.deleteBatch(batch.map(({ id }) => id));
      expect(await storage.list({ entityId: "batch-entity" })).toHaveLength(0);
    });
  });

  // ── Edge Cases ────────────────────────────────────────────────────────

  describe("Edge Cases", () => {
    it("should handle checkpoints for different entity types", async () => {
      const types = ["workflow", "agent", "task"] as const;
      for (const entityType of types) {
        await storage.save(
          `chk-${entityType}`,
          createTestData(8),
          createCheckpointMetadata({ entityType, entityId: `entity-${entityType}` }),
        );
      }

      for (const entityType of types) {
        expect(await storage.list({ entityType })).toHaveLength(1);
      }
    });
  });

  // ── Clear ─────────────────────────────────────────────────────────────

  describe("Clear", () => {
    it("should clear all data and files", async () => {
      await storage.save(TEST_CHECKPOINT_ID, createTestData(), createCheckpointMetadata());
      await storage.clear();

      expect(await storage.list()).toHaveLength(0);

      const metaDir = path.join(testDir, "metadata", "checkpoint");
      expect(await fs.readdir(metaDir)).toHaveLength(0);
    });
  });
});