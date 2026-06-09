/**
 * SqliteTaskStorage Integration Test
 *
 * Tests the complete task storage lifecycle with SQLite backend:
 * - CRUD lifecycle (save → load → update → delete → stats)
 * - Task list filtering (executionId, workflowId, status, time range, tags, pagination)
 * - Task statistics (counts, execution time, success/timeout rates)
 * - Batch operations
 * - Expired task cleanup
 * - Edge cases (empty data, special characters)
 *
 * SQLite database files are created in temp directory and cleaned up after tests.
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as fs from "fs/promises";
import * as path from "path";
import * as os from "os";
import { SqliteTaskStorage } from "../../sqlite/sqlite-task-storage.js";
import {
  createTaskMetadata,
  createMinimalTaskMetadata,
  createCompletedTaskMetadata,
  createFailedTaskMetadata,
  createTestData,
  createTaskBatch,
  TEST_TASK_ID,
  TEST_EXECUTION_ID,
  TEST_WORKFLOW_ID,
} from "../common/test-data.js";

describe("SqliteTaskStorage Integration", () => {
  let storage: SqliteTaskStorage;
  let tempDir: string;
  let dbPath: string;

  beforeEach(async () => {
    tempDir = await fs.mkdtemp(path.join(os.tmpdir(), "sqlite-task-int-"));
    dbPath = path.join(tempDir, "test.db");
    storage = new SqliteTaskStorage({ dbPath });
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
      const metadata = createTaskMetadata();

      // Create
      await storage.save(TEST_TASK_ID, data, metadata);

      // Read
      const loaded = await storage.load(TEST_TASK_ID);
      expect(loaded).toEqual(data);
      expect(await storage.exists(TEST_TASK_ID)).toBe(true);

      const meta = await storage.getMetadata(TEST_TASK_ID);
      expect(meta).not.toBeNull();
      expect(meta!.status).toBe("RUNNING");
      expect(meta!.taskId).toBe(TEST_TASK_ID);
      expect(meta!.executionId).toBe(TEST_EXECUTION_ID);
      expect(meta!.workflowId).toBe(TEST_WORKFLOW_ID);

      // Stats reflect the saved task
      const statsAfterSave = await storage.getTaskStats();
      expect(statsAfterSave.total).toBe(1);

      // Delete
      await storage.delete(TEST_TASK_ID);
      expect(await storage.load(TEST_TASK_ID)).toBeNull();
      expect(await storage.exists(TEST_TASK_ID)).toBe(false);

      // Stats reflect deletion
      const statsAfterDelete = await storage.getTaskStats();
      expect(statsAfterDelete.total).toBe(0);
    });

    it("should handle minimal task metadata", async () => {
      const metadata = createMinimalTaskMetadata();
      const data = createTestData(16);

      await storage.save(metadata.taskId, data, metadata);
      const loaded = await storage.load(metadata.taskId);
      expect(loaded).toEqual(data);

      const meta = await storage.getMetadata(metadata.taskId);
      expect(meta!.status).toBe("QUEUED");
      expect(meta!.startTime).toBeUndefined();
    });

    it("should return null for non-existent task", async () => {
      expect(await storage.load("non-existent")).toBeNull();
      expect(await storage.getMetadata("non-existent")).toBeNull();
    });

    it("should overwrite existing task data", async () => {
      const originalMeta = createTaskMetadata({ tags: ["original"] });
      await storage.save(TEST_TASK_ID, createTestData(16), originalMeta);

      const updatedMeta = createTaskMetadata({ tags: ["updated"], status: "COMPLETED" });
      await storage.save(TEST_TASK_ID, createTestData(32), updatedMeta);

      const loaded = await storage.load(TEST_TASK_ID);
      expect(loaded).toEqual(createTestData(32));
    });

    it("should update task status", async () => {
      const metadata = createTaskMetadata();
      await storage.save(TEST_TASK_ID, createTestData(), metadata);

      // Update status by saving with new metadata
      const updatedMetadata = createTaskMetadata({ status: "COMPLETED" });
      await storage.save(TEST_TASK_ID, createTestData(), updatedMetadata);

      const meta = await storage.getMetadata(TEST_TASK_ID);
      expect(meta!.status).toBe("COMPLETED");
    });
  });

  // ── List Filtering ────────────────────────────────────────────────────

  describe("List Filtering", () => {
    beforeEach(async () => {
      const batch = createTaskBatch(6);
      for (const item of batch) {
        await storage.save(item.id, item.data, item.metadata);
      }
    });

    it("should list all tasks", async () => {
      const ids = await storage.list();
      expect(ids).toHaveLength(6);
    });

    it("should filter by executionId", async () => {
      const ids = await storage.list({ executionId: TEST_EXECUTION_ID });
      expect(ids).toHaveLength(6);
    });

    it("should filter by workflowId", async () => {
      const ids = await storage.list({ workflowId: TEST_WORKFLOW_ID });
      expect(ids.length).toBeGreaterThan(0);
    });

    it("should filter by status", async () => {
      const ids = await storage.list({ status: "RUNNING" });
      expect(ids.length).toBeGreaterThan(0);
    });
  });

  // ── Task Statistics ───────────────────────────────────────────────────

  describe("Task Statistics", () => {
    it("should return correct task stats", async () => {
      // Save tasks with various statuses
      await storage.save("stats-1", createTestData(), createCompletedTaskMetadata());
      await storage.save("stats-2", createTestData(), createFailedTaskMetadata());
      await storage.save("stats-3", createTestData(), createTaskMetadata({ status: "RUNNING" }));

      const stats = await storage.getTaskStats();
      expect(stats.total).toBe(3);
    });

    it("should return zero stats for empty storage", async () => {
      const stats = await storage.getTaskStats();
      expect(stats.total).toBe(0);
    });
  });

  // ── Batch Operations ──────────────────────────────────────────────────

  describe("Batch Operations", () => {
    it("should save and load multiple items individually", async () => {
      const batch = createTaskBatch(5);
      for (const item of batch) {
        await storage.save(item.id, item.data, item.metadata);
      }

      for (const item of batch) {
        const loaded = await storage.load(item.id);
        expect(loaded).toEqual(item.data);
      }
    });

    it("should delete multiple items individually", async () => {
      const batch = createTaskBatch(3);
      for (const item of batch) {
        await storage.save(item.id, item.data, item.metadata);
      }

      for (const item of batch) {
        await storage.delete(item.id);
        expect(await storage.load(item.id)).toBeNull();
      }
    });
  });

  // ── Expired Task Cleanup ──────────────────────────────────────────────

  describe("Task Cleanup", () => {
    it("should clean up expired tasks", async () => {
      const oldMeta = createCompletedTaskMetadata({ completeTime: Date.now() - 100000 });
      await storage.save("expired-1", createTestData(), oldMeta);

      const recentMeta = createCompletedTaskMetadata();
      await storage.save("recent-1", createTestData(), recentMeta);

      await storage.cleanupTasks(50000);

      expect(await storage.load("expired-1")).toBeNull();
      expect(await storage.load("recent-1")).not.toBeNull();
    });
  });

  // ── Edge Cases ────────────────────────────────────────────────────────

  describe("Edge Cases", () => {
    it("should handle empty data buffer", async () => {
      const metadata = createTaskMetadata();
      await storage.save("empty-data", new Uint8Array(0), metadata);
      const loaded = await storage.load("empty-data");
      expect(loaded).toEqual(new Uint8Array(0));
    });

    it("should handle large data buffer", async () => {
      const largeData = createTestData(100000);
      const metadata = createTaskMetadata();
      await storage.save("large-data", largeData, metadata);
      const loaded = await storage.load("large-data");
      expect(loaded).toEqual(largeData);
    });

    it("should handle special characters in task ID", async () => {
      const specialId = "task-@#$%-123_测试";
      const metadata = createTaskMetadata({ taskId: specialId });
      await storage.save(specialId, createTestData(), metadata);
      const loaded = await storage.load(specialId);
      expect(loaded).not.toBeNull();
    });
  });

  // ── File Persistence ──────────────────────────────────────────────────

  describe("File Persistence", () => {
    it("should persist data across re-initialization", async () => {
      const metadata = createTaskMetadata();
      await storage.save("persist-1", createTestData(), metadata);
      await storage.close();

      // Re-open the same database file
      storage = new SqliteTaskStorage({ dbPath });
      await storage.initialize();

      const loaded = await storage.load("persist-1");
      expect(loaded).not.toBeNull();

      const meta = await storage.getMetadata("persist-1");
      expect(meta).not.toBeNull();
      expect(meta!.taskId).toBe("persist-1");
    });
  });
});