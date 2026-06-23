/**
 * MemoryWorkflowStorage Integration Test
 *
 * Tests the complete workflow storage lifecycle and adapter-specific operations:
 * - CRUD lifecycle (save → load → update → delete)
 * - Workflow version management
 * - Batch operations
 * - Edge cases (not found, duplicate, empty data)
 * - Storage metrics
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { MemoryWorkflowStorage } from "../../memory/memory-workflow-storage.js";
import {
  createWorkflowMetadata,
  createMinimalWorkflowMetadata,
  createWorkflowVersionEntry,
  createWorkflowVersionListOptions,
  createTestData,
  createWorkflowBatch,
  TEST_WORKFLOW_ID,
} from "../common/test-data.js";

describe("MemoryWorkflowStorage Integration", () => {
  let storage: MemoryWorkflowStorage;

  beforeEach(async () => {
    storage = new MemoryWorkflowStorage();
    await storage.initialize();
  });

  afterEach(async () => {
    await storage.close();
  });

  // ── Lifecycle ─────────────────────────────────────────────────────────

  describe("CRUD Lifecycle", () => {
    it("should complete full CRUD lifecycle (save → load → update → delete)", async () => {
      const data = createTestData();
      const metadata = createWorkflowMetadata();

      // Create
      await storage.save(TEST_WORKFLOW_ID, data, metadata);

      // Read
      const loaded = await storage.load(TEST_WORKFLOW_ID);
      expect(loaded).not.toBeNull();
      expect(loaded).toEqual(data);

      // Update metadata
      await storage.updateWorkflowMetadata(TEST_WORKFLOW_ID, {
        name: "Updated Workflow",
        nodeCount: 10,
      });

      const updatedMeta = await storage.getMetadata(TEST_WORKFLOW_ID);
      expect(updatedMeta).not.toBeNull();
      expect(updatedMeta!.name).toBe("Updated Workflow");
      expect(updatedMeta!.nodeCount).toBe(10);
      expect(updatedMeta!.edgeCount).toBe(4); // Unchanged

      // Delete
      await storage.delete(TEST_WORKFLOW_ID);
      const afterDelete = await storage.load(TEST_WORKFLOW_ID);
      expect(afterDelete).toBeNull();
      const exists = await storage.exists(TEST_WORKFLOW_ID);
      expect(exists).toBe(false);
    });

    it("should return null for non-existent workflow", async () => {
      const loaded = await storage.load("non-existent");
      expect(loaded).toBeNull();

      const metadata = await storage.getMetadata("non-existent");
      expect(metadata).toBeNull();
    });

    it("should handle minimal workflow metadata", async () => {
      const data = createTestData(16);
      const metadata = createMinimalWorkflowMetadata();

      await storage.save(metadata.workflowId, data, metadata);

      const loaded = await storage.load(metadata.workflowId);
      expect(loaded).toEqual(data);

      const exists = await storage.exists(metadata.workflowId);
      expect(exists).toBe(true);
    });

    it("should handle empty data", async () => {
      const data = new Uint8Array(0);
      const metadata = createWorkflowMetadata();

      await storage.save(TEST_WORKFLOW_ID, data, metadata);
      const loaded = await storage.load(TEST_WORKFLOW_ID);
      expect(loaded).toEqual(data);
    });

    it("should handle duplicate save (overwrite)", async () => {
      const data1 = createTestData(16);
      const data2 = createTestData(32);
      const metadata = createWorkflowMetadata();

      await storage.save(TEST_WORKFLOW_ID, data1, metadata);
      await storage.save(TEST_WORKFLOW_ID, data2, metadata);

      const loaded = await storage.load(TEST_WORKFLOW_ID);
      expect(loaded).toEqual(data2);
    });

    it("should throw when updating metadata for non-existent workflow", async () => {
      await expect(
        storage.updateWorkflowMetadata("non-existent", { name: "Nope" }),
      ).rejects.toThrow();
    });
  });

  // ── List / Exists ─────────────────────────────────────────────────────

  describe("List and Exists", () => {
    it("should list all workflow IDs", async () => {
      const batch = createWorkflowBatch(3);
      for (const item of batch) {
        await storage.save(item.id, item.data, item.metadata);
      }

      const ids = await storage.list();
      expect(ids).toHaveLength(3);
      for (const item of batch) {
        expect(ids).toContain(item.id);
      }
    });

    it("should return empty list when no workflows exist", async () => {
      const ids = await storage.list();
      expect(ids).toEqual([]);
    });

    it("should return correct exists status", async () => {
      await storage.save(TEST_WORKFLOW_ID, createTestData(), createWorkflowMetadata());
      expect(await storage.exists(TEST_WORKFLOW_ID)).toBe(true);
      expect(await storage.exists("non-existent")).toBe(false);
    });
  });

  // ── Versioning ────────────────────────────────────────────────────────

  describe("Workflow Versioning", () => {
    it("should save and load workflow versions", async () => {
      // First save the workflow
      await storage.save(TEST_WORKFLOW_ID, createTestData(), createWorkflowMetadata());

      const v1 = createWorkflowVersionEntry(TEST_WORKFLOW_ID, "1.0.0");
      const v2 = createWorkflowVersionEntry(TEST_WORKFLOW_ID, "2.0.0");

      await storage.saveWorkflowVersion(TEST_WORKFLOW_ID, v1.version, v1.data, v1.changeNote);
      await storage.saveWorkflowVersion(TEST_WORKFLOW_ID, v2.version, v2.data, v2.changeNote);

      // Load specific versions
      const loadedV1 = await storage.loadWorkflowVersion(TEST_WORKFLOW_ID, "1.0.0");
      expect(loadedV1).toEqual(v1.data);

      const loadedV2 = await storage.loadWorkflowVersion(TEST_WORKFLOW_ID, "2.0.0");
      expect(loadedV2).toEqual(v2.data);
    });

    it("should list workflow versions", async () => {
      await storage.save(TEST_WORKFLOW_ID, createTestData(), createWorkflowMetadata());
      await storage.saveWorkflowVersion(TEST_WORKFLOW_ID, "1.0.0", createTestData(16), "Initial");
      await storage.saveWorkflowVersion(TEST_WORKFLOW_ID, "2.0.0", createTestData(32), "Update");
      await storage.saveWorkflowVersion(TEST_WORKFLOW_ID, "3.0.0", createTestData(64), "Major");

      const versions = await storage.listWorkflowVersions(
        TEST_WORKFLOW_ID,
        createWorkflowVersionListOptions(),
      );
      expect(versions).toHaveLength(3);
    });

    it("should delete workflow versions", async () => {
      await storage.save(TEST_WORKFLOW_ID, createTestData(), createWorkflowMetadata());
      await storage.saveWorkflowVersion(TEST_WORKFLOW_ID, "1.0.0", createTestData(16));
      await storage.saveWorkflowVersion(TEST_WORKFLOW_ID, "2.0.0", createTestData(32));

      await storage.deleteWorkflowVersion(TEST_WORKFLOW_ID, "1.0.0");

      const versions = await storage.listWorkflowVersions(TEST_WORKFLOW_ID);
      expect(versions).toHaveLength(1);
      expect(versions[0]!.version).toBe("2.0.0");
    });

    it("should return null for non-existent version", async () => {
      const loaded = await storage.loadWorkflowVersion(TEST_WORKFLOW_ID, "99.0.0");
      expect(loaded).toBeNull();
    });
  });

  // ── Batch Operations ──────────────────────────────────────────────────

  describe("Batch Operations", () => {
    it("should save and load multiple workflows in batch", async () => {
      const batch = createWorkflowBatch(5);
      await storage.saveBatch(batch.map(({ id, data, metadata }) => ({ id, data, metadata })));

      const ids = await storage.list();
      expect(ids).toHaveLength(5);

      const loaded = await storage.loadBatch(batch.map(({ id }) => id));
      expect(loaded).toHaveLength(5);
      for (const item of loaded) {
        expect(item.data).not.toBeNull();
      }
    });

    it("should delete multiple workflows in batch", async () => {
      const batch = createWorkflowBatch(3);
      await storage.saveBatch(batch.map(({ id, data, metadata }) => ({ id, data, metadata })));

      await storage.deleteBatch(batch.map(({ id }) => id));

      const ids = await storage.list();
      expect(ids).toHaveLength(0);
    });
  });

  // ── Storage Metrics ───────────────────────────────────────────────────

  describe("Storage Metrics", () => {
    it("should track save and load operations", async () => {
      await storage.save(TEST_WORKFLOW_ID, createTestData(64), createWorkflowMetadata());
      await storage.load(TEST_WORKFLOW_ID);

      const metrics = await storage.getMetrics();
      expect(metrics.saveCount).toBeGreaterThanOrEqual(1);
      expect(metrics.loadCount).toBeGreaterThanOrEqual(1);
      expect(metrics.totalBlobSize).toBeGreaterThan(0);
    });

    it("should reset metrics", async () => {
      await storage.save(TEST_WORKFLOW_ID, createTestData(64), createWorkflowMetadata());
      storage.resetMetrics();

      const metrics = await storage.getMetrics();
      expect(metrics.saveCount).toBe(0);
    });
  });

  // ── Clear / Re-initialize ─────────────────────────────────────────────

  describe("Clear and Re-initialize", () => {
    it("should clear all data and allow reuse", async () => {
      await storage.save(TEST_WORKFLOW_ID, createTestData(), createWorkflowMetadata());
      await storage.clear();

      const ids = await storage.list();
      expect(ids).toHaveLength(0);

      // Reuse after clear
      await storage.save(TEST_WORKFLOW_ID, createTestData(), createWorkflowMetadata());
      const loaded = await storage.load(TEST_WORKFLOW_ID);
      expect(loaded).not.toBeNull();
    });
  });
});