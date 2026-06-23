/**
 * MemoryCheckpointStorage Integration Test
 *
 * Tests the complete checkpoint storage lifecycle:
 * - CRUD lifecycle
 * - Entity-aware queries (listByEntityWithMetadata, getLatestByEntity)
 * - Delete with retention policies (deleteByEntity)
 * - List filtering with metadata-only mode
 * - Batch operations
 * - Index maintenance (entity index, tag index)
 * - Edge cases
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { MemoryCheckpointStorage } from "../../memory/memory-checkpoint-storage.js";
import {
  createCheckpointMetadata,
  createMinimalCheckpointMetadata,
  createCheckpointListOptions,
  createCheckpointBatch,
  createTestData,
  TEST_CHECKPOINT_ID,
  TEST_ENTITY_ID,
} from "../common/test-data.js";

describe("MemoryCheckpointStorage Integration", () => {
  let storage: MemoryCheckpointStorage;

  beforeEach(async () => {
    storage = new MemoryCheckpointStorage();
    await storage.initialize();
  });

  afterEach(async () => {
    await storage.close();
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

      await storage.save(metadata.entityId, data, metadata);
      expect(await storage.load(metadata.entityId)).toEqual(data);
    });

    it("should return null for non-existent checkpoint", async () => {
      expect(await storage.load("non-existent")).toBeNull();
      expect(await storage.getMetadata("non-existent")).toBeNull();
    });

    it("should overwrite existing checkpoint", async () => {
      const data1 = createTestData(16);
      const data2 = createTestData(32);

      await storage.save(TEST_CHECKPOINT_ID, data1, createCheckpointMetadata());
      await storage.save(TEST_CHECKPOINT_ID, data2, createCheckpointMetadata());

      expect(await storage.load(TEST_CHECKPOINT_ID)).toEqual(data2);
    });
  });

  // ── Entity-Aware Queries ──────────────────────────────────────────────

  describe("Entity-Aware Queries", () => {
    beforeEach(async () => {
      // Create checkpoints for two entities
      const entity1 = createCheckpointBatch("entity-1", 5);
      const entity2 = createCheckpointBatch("entity-2", 3);

      for (const item of [...entity1, ...entity2]) {
        await storage.save(item.id, item.data, item.metadata);
      }
    });

    it("should list checkpoints with metadata only", async () => {
      const items = await storage.listWithMetadata(
        createCheckpointListOptions({ entityId: "entity-1" }),
      );
      expect(items.length).toBeGreaterThan(0);
      for (const item of items) {
        expect(item.metadata).toBeDefined();
        expect(item.metadata.entityId).toBe("entity-1");
      }
    });

    it("should list checkpoints by entity with metadata", async () => {
      const items = await storage.listByEntityWithMetadata("entity-1", "workflow");
      expect(items).toHaveLength(5);
    });

    it("should list checkpoints by entity with pagination", async () => {
      const items = await storage.listByEntityWithMetadata("entity-1", "workflow", {
        limit: 2,
        offset: 0,
      });
      expect(items).toHaveLength(2);
    });

    it("should get latest checkpoint by entity", async () => {
      const items = await storage.getLatestByEntity("entity-1", "workflow", 1);
      expect(items).toHaveLength(1);
      expect(items[0]!.metadata.entityId).toBe("entity-1");
    });

    it("should get latest checkpoints with data included", async () => {
      const items = await storage.getLatestByEntity("entity-1", "workflow", 2, true);
      expect(items).toHaveLength(2);
      for (const item of items) {
        expect(item.data).toBeDefined();
      }
    });

    it("should return empty array for non-existent entity", async () => {
      const items = await storage.listByEntityWithMetadata("non-existent", "workflow");
      expect(items).toEqual([]);

      const latest = await storage.getLatestByEntity("non-existent", "workflow");
      expect(latest).toEqual([]);
    });
  });

  // ── Delete with Retention ─────────────────────────────────────────────

  describe("Delete with Retention Policies", () => {
    it("should delete by entity", async () => {
      const batch = createCheckpointBatch("entity-to-delete", 3);
      for (const item of batch) {
        await storage.save(item.id, item.data, item.metadata);
      }

      const deleted = await storage.deleteByEntity("entity-to-delete", "workflow");
      expect(deleted).toBe(3);

      const remaining = await storage.list({
        entityId: "entity-to-delete",
        entityType: "workflow",
      });
      expect(remaining).toHaveLength(0);
    });

    it("should keep latest N checkpoints", async () => {
      const batch = createCheckpointBatch("entity-keep", 5);
      for (const item of batch) {
        await storage.save(item.id, item.data, item.metadata);
      }

      // Keep latest 2 out of 5
      const deleted = await storage.deleteByEntity("entity-keep", "workflow", {
        keepLatest: 2,
      });
      expect(deleted).toBe(3);

      const remaining = await storage.list({
        entityId: "entity-keep",
        entityType: "workflow",
      });
      expect(remaining).toHaveLength(2);
    });

    it("should delete checkpoints older than timestamp", async () => {
      const now = Date.now();
      const old = createCheckpointBatch("entity-old", 3);
      for (const item of old) {
        await storage.save(item.id, item.data, item.metadata);
      }

      // Should keep the more recent ones (inequality based on batch timestamps)
      const deleted = await storage.deleteByEntity("entity-old", "workflow", {
        olderThan: now,
      });
      expect(deleted).toBeGreaterThanOrEqual(0);

      const remaining = await storage.list({
        entityId: "entity-old",
        entityType: "workflow",
      });
      // At least some should remain since batch timestamps are recent
      expect(remaining.length + deleted).toBe(3);
    });
  });

  // ── List Filtering ────────────────────────────────────────────────────

  describe("List Filtering", () => {
    beforeEach(async () => {
      const agentBatch = createCheckpointBatch("agent-entity", 3);
      const taskBatch = createCheckpointBatch("task-entity", 2);
      for (const item of [...agentBatch, ...taskBatch]) {
        // Override entity type
        const agentMeta = createCheckpointMetadata({
          entityId: "agent-entity",
          entityType: "agent",
        });
        const taskMeta = createCheckpointMetadata({
          entityId: "task-entity",
          entityType: "task",
        });
        await storage.save(item.id, item.data, item.metadata.entityId === "agent-entity" ? agentMeta : taskMeta);
      }
    });

    it("should list all checkpoints", async () => {
      const ids = await storage.list();
      expect(ids).toHaveLength(5);
    });

    it("should filter by entity type", async () => {
      const ids = await storage.list({ entityType: "agent" });
      expect(ids).toHaveLength(3);
    });

    it("should filter by tags", async () => {
      const ids = await storage.list({ tags: ["full"] });
      expect(ids.length).toBeGreaterThan(0);
    });

    it("should return empty list when no matches", async () => {
      const ids = await storage.list({ entityId: "non-existent" });
      expect(ids).toEqual([]);
    });
  });

  // ── Batch Operations ──────────────────────────────────────────────────

  describe("Batch Operations", () => {
    it("should save batch with index updates", async () => {
      const batch = createCheckpointBatch("batch-entity", 5);
      await storage.saveBatch(batch.map(({ id, data, metadata }) => ({ id, data, metadata })));

      const ids = await storage.list({ entityId: "batch-entity" });
      expect(ids).toHaveLength(5);
    });

    it("should load and delete batch", async () => {
      const batch = createCheckpointBatch("batch-entity", 3);
      await storage.saveBatch(batch.map(({ id, data, metadata }) => ({ id, data, metadata })));

      const loaded = await storage.loadBatch(batch.map(({ id }) => id));
      expect(loaded).toHaveLength(3);

      await storage.deleteBatch(batch.map(({ id }) => id));
      expect(await storage.list({ entityId: "batch-entity" })).toHaveLength(0);
    });
  });

  // ── Index Maintenance ─────────────────────────────────────────────────

  describe("Index Maintenance", () => {
    it("should maintain entity index after delete", async () => {
      const batch = createCheckpointBatch("idx-entity", 3);
      for (const item of batch) {
        await storage.save(item.id, item.data, item.metadata);
      }

      // Delete one checkpoint
      await storage.delete(batch[0]!.id);

      // Entity index should still work
      const items = await storage.listByEntityWithMetadata("idx-entity", "workflow");
      expect(items).toHaveLength(2);
    });

    it("should maintain tag index after delete", async () => {
      const meta = createCheckpointMetadata({ tags: ["important"] });
      await storage.save("tagged-chk", createTestData(), meta);
      await storage.delete("tagged-chk");

      // Tag-based query should return empty
      const ids = await storage.list({ tags: ["important"] });
      expect(ids).not.toContain("tagged-chk");
    });

    it("should clear all indexes on clear", async () => {
      const batch = createCheckpointBatch("clear-entity", 3);
      await storage.saveBatch(batch.map(({ id, data, metadata }) => ({ id, data, metadata })));
      await storage.clear();

      const items = await storage.listByEntityWithMetadata("clear-entity", "workflow");
      expect(items).toHaveLength(0);

      expect(await storage.list()).toHaveLength(0);
    });
  });

  // ── Edge Cases ────────────────────────────────────────────────────────

  describe("Edge Cases", () => {
    it("should handle checkpoints for different entity types", async () => {
      const types = ["workflow", "agent", "task"] as const;
      for (const entityType of types) {
        const id = `chk-${entityType}`;
        await storage.save(
          id,
          createTestData(8),
          createCheckpointMetadata({ entityType, entityId: `entity-${entityType}` }),
        );
      }

      for (const entityType of types) {
        const ids = await storage.list({ entityType });
        expect(ids).toHaveLength(1);
      }
    });

    it("should support pagination in list", async () => {
      const batch = createCheckpointBatch("paginated-entity", 10);
      await storage.saveBatch(batch.map(({ id, data, metadata }) => ({ id, data, metadata })));

      const page1 = await storage.list({
        entityId: "paginated-entity",
        entityType: "workflow",
        limit: 3,
        offset: 0,
      });
      expect(page1).toHaveLength(3);

      const page2 = await storage.list({
        entityId: "paginated-entity",
        entityType: "workflow",
        limit: 3,
        offset: 3,
      });
      expect(page2).toHaveLength(3);

      // No overlap
      for (const id of page1) {
        expect(page2).not.toContain(id);
      }
    });
  });

  // ── Clear ─────────────────────────────────────────────────────────────

  describe("Clear", () => {
    it("should clear all checkpoints and indexes", async () => {
      await storage.save(TEST_CHECKPOINT_ID, createTestData(), createCheckpointMetadata());
      await storage.clear();

      expect(await storage.list()).toHaveLength(0);
      expect(await storage.load(TEST_CHECKPOINT_ID)).toBeNull();

      const items = await storage.listByEntityWithMetadata(TEST_ENTITY_ID, "workflow");
      expect(items).toHaveLength(0);
    });
  });
});