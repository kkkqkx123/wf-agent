/**
 * SqliteWorkflowStorage Integration Test
 *
 * Tests the complete workflow storage lifecycle with SQLite backend:
 * - CRUD lifecycle (save → load → update → delete)
 * - Workflow version management
 * - Batch operations
 * - Edge cases (not found, duplicate, empty data)
 *
 * SQLite database files are created in temp directory and cleaned up after tests.
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as fs from "fs/promises";
import * as path from "path";
import * as os from "os";
import { SqliteWorkflowStorage } from "../../sqlite/sqlite-workflow-storage.js";
import {
  createWorkflowMetadata,
  createMinimalWorkflowMetadata,
  createWorkflowVersionEntry,
  createWorkflowVersionListOptions,
  createTestData,
  createWorkflowBatch,
  TEST_WORKFLOW_ID,
} from "../common/test-data.js";

describe("SqliteWorkflowStorage Integration", () => {
  let storage: SqliteWorkflowStorage;
  let tempDir: string;
  let dbPath: string;

  beforeEach(async () => {
    tempDir = await fs.mkdtemp(path.join(os.tmpdir(), "sqlite-wf-int-"));
    dbPath = path.join(tempDir, "test.db");
    storage = new SqliteWorkflowStorage({ dbPath });
    await storage.initialize();
  });

  afterEach(async () => {
    await storage.close();
    await fs.rm(tempDir, { recursive: true, force: true });
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

      // Delete
      await storage.delete(TEST_WORKFLOW_ID);
      expect(await storage.load(TEST_WORKFLOW_ID)).toBeNull();
      expect(await storage.getMetadata(TEST_WORKFLOW_ID)).toBeNull();
    });

    it("should handle minimal workflow metadata", async () => {
      const metadata = createMinimalWorkflowMetadata();
      const data = createTestData();

      await storage.save(metadata.workflowId, data, metadata);
      const loaded = await storage.load(metadata.workflowId);
      expect(loaded).toEqual(data);
    });

    it("should return null for non-existent workflow", async () => {
      expect(await storage.load("non-existent")).toBeNull();
      expect(await storage.getMetadata("non-existent")).toBeNull();
      expect(await storage.exists("non-existent")).toBe(false);
    });

    it("should overwrite existing workflow", async () => {
      const metadata = createWorkflowMetadata();
      await storage.save(TEST_WORKFLOW_ID, createTestData(16), metadata);

      const newData = createTestData(32);
      await storage.save(TEST_WORKFLOW_ID, newData, metadata);

      const loaded = await storage.load(TEST_WORKFLOW_ID);
      expect(loaded).toEqual(newData);
    });
  });

  // ── Workflow Version Management ───────────────────────────────────────

  describe("Workflow Version Management", () => {
    it("should save and list workflow versions", async () => {
      const metadata = createWorkflowMetadata();
      await storage.save(TEST_WORKFLOW_ID, createTestData(), metadata);

      const versionEntry = createWorkflowVersionEntry();
      await storage.saveWorkflowVersion(TEST_WORKFLOW_ID, versionEntry.version, createTestData());

      const versions = await storage.listWorkflowVersions(
        TEST_WORKFLOW_ID,
        createWorkflowVersionListOptions(),
      );
      expect(versions.length).toBeGreaterThanOrEqual(1);
    });

    it("should get specific workflow version via list", async () => {
      const metadata = createWorkflowMetadata();
      await storage.save(TEST_WORKFLOW_ID, createTestData(), metadata);

      const versionEntry = createWorkflowVersionEntry();
      await storage.saveWorkflowVersion(TEST_WORKFLOW_ID, versionEntry.version, createTestData());

      const versions = await storage.listWorkflowVersions(
        TEST_WORKFLOW_ID,
        createWorkflowVersionListOptions(),
      );
      const version = versions.find((v) => v.version === versionEntry.version);
      expect(version).not.toBeUndefined();
      expect(version!.version).toBe(versionEntry.version);
    });
  });

  // ── Batch Operations ──────────────────────────────────────────────────

  describe("Batch Operations", () => {
    it("should save and load multiple items individually", async () => {
      const batch = createWorkflowBatch(5);
      for (const item of batch) {
        await storage.save(item.id, item.data, item.metadata);
      }

      for (const item of batch) {
        const loaded = await storage.load(item.id);
        expect(loaded).toEqual(item.data);
      }
    });

    it("should delete multiple items individually", async () => {
      const batch = createWorkflowBatch(3);
      for (const item of batch) {
        await storage.save(item.id, item.data, item.metadata);
      }

      for (const item of batch) {
        await storage.delete(item.id);
        expect(await storage.load(item.id)).toBeNull();
      }
    });
  });

  // ── Edge Cases ────────────────────────────────────────────────────────

  describe("Edge Cases", () => {
    it("should handle empty data buffer", async () => {
      const metadata = createWorkflowMetadata();
      await storage.save("empty-data", new Uint8Array(0), metadata);
      const loaded = await storage.load("empty-data");
      expect(loaded).toEqual(new Uint8Array(0));
    });
  });

  // ── File Persistence ──────────────────────────────────────────────────

  describe("File Persistence", () => {
    it("should persist data across re-initialization", async () => {
      const metadata = createWorkflowMetadata();
      await storage.save("persist-1", createTestData(), metadata);
      await storage.close();

      storage = new SqliteWorkflowStorage({ dbPath });
      await storage.initialize();

      const loaded = await storage.load("persist-1");
      expect(loaded).not.toBeNull();
    });
  });
});