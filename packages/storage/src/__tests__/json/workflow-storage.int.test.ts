/**
 * JsonWorkflowStorage Integration Test
 *
 * Tests the complete JSON workflow storage lifecycle:
 * - CRUD lifecycle with file I/O verification
 * - Workflow version management
 * - Batch operations
 * - File persistence (metadata/data separation, directory structure)
 * - Lazy loading mode
 * - Edge cases
 *
 * Test output directory: packages/storage/src/__tests__/json/output/workflow
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as fs from "fs/promises";
import * as path from "path";
import { fileURLToPath } from "url";
import { JsonWorkflowStorage } from "../../json/json-workflow-storage.js";
import {
  createWorkflowMetadata,
  createMinimalWorkflowMetadata,
  createWorkflowVersionEntry,
  createTestData,
  createWorkflowBatch,
  TEST_WORKFLOW_ID,
} from "../common/test-data.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const TEST_OUTPUT_DIR = path.resolve(__dirname, "output", "workflow");

describe("JsonWorkflowStorage Integration", () => {
  let storage: JsonWorkflowStorage;
  let testDir: string;

  beforeEach(async () => {
    testDir = path.join(TEST_OUTPUT_DIR, `test-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`);
    storage = new JsonWorkflowStorage({ baseDir: testDir });
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
      const metadata = createWorkflowMetadata();

      // Create
      await storage.save(TEST_WORKFLOW_ID, data, metadata);

      // Read
      const loaded = await storage.load(TEST_WORKFLOW_ID);
      expect(loaded).not.toBeNull();
      expect(loaded).toEqual(data);

      // Verify file structure
      const metaDir = path.join(testDir, "metadata", "workflow");
      const dataDir = path.join(testDir, "data", "workflow");
      const metaFiles = await fs.readdir(metaDir);
      const dataFiles = await fs.readdir(dataDir);

      expect(metaFiles).toContain(`${TEST_WORKFLOW_ID}.json`);
      expect(dataFiles).toContain(`${TEST_WORKFLOW_ID}.bin`);

      // Update metadata
      await storage.updateWorkflowMetadata(TEST_WORKFLOW_ID, {
        name: "Updated Workflow",
        nodeCount: 10,
      });
      const updatedMeta = await storage.getMetadata(TEST_WORKFLOW_ID);
      expect(updatedMeta!.name).toBe("Updated Workflow");

      // Delete
      await storage.delete(TEST_WORKFLOW_ID);
      expect(await storage.load(TEST_WORKFLOW_ID)).toBeNull();
      expect(await storage.exists(TEST_WORKFLOW_ID)).toBe(false);
    });

    it("should initialize and create directory structure", async () => {
      const dirs = [
        path.join(testDir, "metadata", "workflow"),
        path.join(testDir, "data", "workflow"),
        path.join(testDir, "metadata", "versions"),
        path.join(testDir, "data", "versions"),
      ];

      for (const dir of dirs) {
        const stat = await fs.stat(dir);
        expect(stat.isDirectory()).toBe(true);
      }
    });

    it("should handle minimal workflow metadata", async () => {
      const metadata = createMinimalWorkflowMetadata();
      const data = createTestData(16);

      await storage.save(metadata.workflowId, data, metadata);
      const loaded = await storage.load(metadata.workflowId);
      expect(loaded).toEqual(data);
    });

    it("should return null for non-existent workflow", async () => {
      expect(await storage.load("non-existent")).toBeNull();
      expect(await storage.getMetadata("non-existent")).toBeNull();
    });

    it("should overwrite existing workflow data and files", async () => {
      const data1 = createTestData(16);
      const data2 = createTestData(32);

      await storage.save(TEST_WORKFLOW_ID, data1, createWorkflowMetadata());
      await storage.save(TEST_WORKFLOW_ID, data2, createWorkflowMetadata());

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
    it("should list all workflow IDs after re-initialization", async () => {
      const batch = createWorkflowBatch(3);
      for (const item of batch) {
        await storage.save(item.id, item.data, item.metadata);
      }

      // Close and re-initialize to test persistence
      await storage.close();
      storage = new JsonWorkflowStorage({ baseDir: testDir });
      await storage.initialize();

      const ids = await storage.list();
      expect(ids).toHaveLength(3);
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
      await storage.save(TEST_WORKFLOW_ID, createTestData(), createWorkflowMetadata());

      const v1 = createWorkflowVersionEntry(TEST_WORKFLOW_ID, "1.0.0");
      const v2 = createWorkflowVersionEntry(TEST_WORKFLOW_ID, "2.0.0");

      await storage.saveWorkflowVersion(TEST_WORKFLOW_ID, v1.version, v1.data, v1.changeNote);
      await storage.saveWorkflowVersion(TEST_WORKFLOW_ID, v2.version, v2.data, v2.changeNote);

      const loadedV1 = await storage.loadWorkflowVersion(TEST_WORKFLOW_ID, "1.0.0");
      expect(loadedV1).toEqual(v1.data);

      const loadedV2 = await storage.loadWorkflowVersion(TEST_WORKFLOW_ID, "2.0.0");
      expect(loadedV2).toEqual(v2.data);
    });

    it("should persist versions across re-initialization", async () => {
      await storage.save(TEST_WORKFLOW_ID, createTestData(), createWorkflowMetadata());
      await storage.saveWorkflowVersion(TEST_WORKFLOW_ID, "1.0.0", createTestData(16), "Initial");
      await storage.saveWorkflowVersion(TEST_WORKFLOW_ID, "2.0.0", createTestData(32), "Update");

      await storage.close();
      storage = new JsonWorkflowStorage({ baseDir: testDir });
      await storage.initialize();

      const versions = await storage.listWorkflowVersions(TEST_WORKFLOW_ID);
      expect(versions).toHaveLength(2);
      expect(versions[0]!.version).toBe("2.0.0"); // Newest first
      expect(versions[0]!.changeNote).toBe("Update");
    });

    it("should delete versions and clean up files", async () => {
      await storage.save(TEST_WORKFLOW_ID, createTestData(), createWorkflowMetadata());
      await storage.saveWorkflowVersion(TEST_WORKFLOW_ID, "1.0.0", createTestData(16));
      await storage.saveWorkflowVersion(TEST_WORKFLOW_ID, "2.0.0", createTestData(32));

      await storage.deleteWorkflowVersion(TEST_WORKFLOW_ID, "1.0.0");

      const versions = await storage.listWorkflowVersions(TEST_WORKFLOW_ID);
      expect(versions).toHaveLength(1);
      expect(versions[0]!.version).toBe("2.0.0");
    });

    it("should return null for non-existent version", async () => {
      expect(await storage.loadWorkflowVersion(TEST_WORKFLOW_ID, "99.0.0")).toBeNull();
    });
  });

  // ── Lazy Loading Mode ─────────────────────────────────────────────────

  describe("Lazy Loading Mode", () => {
    it("should work in lazy loading mode", async () => {
      const lazyStorage = new JsonWorkflowStorage({
        baseDir: testDir,
        lazyLoading: true,
        metadataCacheSize: 50,
      });
      await lazyStorage.initialize();

      const batch = createWorkflowBatch(5);
      for (const item of batch) {
        await lazyStorage.save(item.id, item.data, item.metadata);
      }

      await lazyStorage.close();

      // Re-open in lazy mode
      const lazyStorage2 = new JsonWorkflowStorage({
        baseDir: testDir,
        lazyLoading: true,
        metadataCacheSize: 50,
      });
      await lazyStorage2.initialize();

      const ids = await lazyStorage2.list();
      expect(ids).toHaveLength(5);

      for (const item of batch) {
        const loaded = await lazyStorage2.load(item.id);
        expect(loaded).toEqual(item.data);

        const meta = await lazyStorage2.getMetadata(item.id);
        expect(meta).not.toBeNull();
      }

      await lazyStorage2.close();
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
      expect(loaded.every((item) => item.data !== null)).toBe(true);
    });

    it("should delete multiple workflows in batch", async () => {
      const batch = createWorkflowBatch(3);
      await storage.saveBatch(batch.map(({ id, data, metadata }) => ({ id, data, metadata })));

      await storage.deleteBatch(batch.map(({ id }) => id));

      const ids = await storage.list();
      expect(ids).toHaveLength(0);
    });
  });

  // ── Edge Cases ────────────────────────────────────────────────────────

  describe("Edge Cases", () => {
    it("should handle empty data", async () => {
      const data = new Uint8Array(0);
      await storage.save(TEST_WORKFLOW_ID, data, createWorkflowMetadata());
      const loaded = await storage.load(TEST_WORKFLOW_ID);
      expect(loaded).toEqual(data);
    });

    it("should handle workflow IDs with special characters", async () => {
      const ids = ["workflow-123", "workflow_456", "workflow.789"];
      for (const id of ids) {
        await storage.save(id, createTestData(8), createWorkflowMetadata({ workflowId: id }));
        const loaded = await storage.load(id);
        expect(loaded).not.toBeNull();
      }
    });
  });

  // ── Storage Metrics ───────────────────────────────────────────────────

  describe("Storage Metrics", () => {
    it("should track operations", async () => {
      await storage.save(TEST_WORKFLOW_ID, createTestData(64), createWorkflowMetadata());
      await storage.load(TEST_WORKFLOW_ID);

      const metrics = await storage.getMetrics();
      expect(metrics.saveCount).toBeGreaterThanOrEqual(1);
      expect(metrics.loadCount).toBeGreaterThanOrEqual(1);
      expect(metrics.totalBlobSize).toBeGreaterThan(0);
    });
  });

  // ── Clear ─────────────────────────────────────────────────────────────

  describe("Clear", () => {
    it("should clear all data and clean up files", async () => {
      await storage.save(TEST_WORKFLOW_ID, createTestData(), createWorkflowMetadata());
      await storage.clear();

      const ids = await storage.list();
      expect(ids).toHaveLength(0);

      // Verify files are cleaned up
      const metaDir = path.join(testDir, "metadata", "workflow");
      const files = await fs.readdir(metaDir);
      expect(files).toHaveLength(0);
    });
  });
});