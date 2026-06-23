/**
 * JsonTaskStorage Integration Test
 *
 * Tests the complete JSON task storage lifecycle:
 * - CRUD lifecycle with file I/O verification
 * - List filtering
 * - Task statistics (getTaskStats)
 * - Expired task cleanup (cleanupTasks)
 * - Batch operations
 * - File persistence across re-initialization
 * - Edge cases
 *
 * Test output directory: packages/storage/src/__tests__/json/output/task
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as fs from "fs/promises";
import * as path from "path";
import { fileURLToPath } from "url";
import { JsonTaskStorage } from "../../json/json-task-storage.js";
import {
  createTaskMetadata,
  createCompletedTaskMetadata,
  createFailedTaskMetadata,
  createTestData,
  createTaskBatch,
  TEST_TASK_ID,
  TEST_EXECUTION_ID,
} from "../common/test-data.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const TEST_OUTPUT_DIR = path.resolve(__dirname, "output", "task");

describe("JsonTaskStorage Integration", () => {
  let storage: JsonTaskStorage;
  let testDir: string;

  beforeEach(async () => {
    testDir = path.join(TEST_OUTPUT_DIR, `test-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`);
    storage = new JsonTaskStorage({ baseDir: testDir });
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
      const metadata = createTaskMetadata();

      // Create
      await storage.save(TEST_TASK_ID, data, metadata);

      // Read
      const loaded = await storage.load(TEST_TASK_ID);
      expect(loaded).toEqual(data);
      expect(await storage.exists(TEST_TASK_ID)).toBe(true);

      const meta = await storage.getMetadata(TEST_TASK_ID);
      expect(meta!.status).toBe("RUNNING");

      // Verify file structure
      const metaDir = path.join(testDir, "metadata", "task");
      const dataDir = path.join(testDir, "data", "task");
      expect(await fs.readdir(metaDir)).toContain(`${TEST_TASK_ID}.json`);
      expect(await fs.readdir(dataDir)).toContain(`${TEST_TASK_ID}.bin`);

      // Delete
      await storage.delete(TEST_TASK_ID);
      expect(await storage.load(TEST_TASK_ID)).toBeNull();
      expect(await storage.exists(TEST_TASK_ID)).toBe(false);
    });

    it("should handle task with error details", async () => {
      const metadata = createFailedTaskMetadata();
      await storage.save(metadata.taskId, createTestData(), metadata);

      const meta = await storage.getMetadata(metadata.taskId);
      expect(meta!.error).toBe("Task execution timeout");
      expect(meta!.errorStack).toContain("Error: timeout");
    });
  });

  // ── List Filtering ────────────────────────────────────────────────────

  describe("List Filtering", () => {
    beforeEach(async () => {
      const tasks = [
        createTaskMetadata({ status: "QUEUED" }),
        createTaskMetadata({ status: "RUNNING", taskId: "task-002" }),
        createCompletedTaskMetadata(),
        createFailedTaskMetadata(),
        createTaskMetadata({ status: "CANCELLED", taskId: "task-005" }),
      ];
      for (const t of tasks) {
        await storage.save(t.taskId, createTestData(8), t);
      }
    });

    it("should list all tasks", async () => {
      expect(await storage.list()).toHaveLength(5);
    });

    it("should filter by executionId", async () => {
      const ids = await storage.list({ executionId: TEST_EXECUTION_ID });
      expect(ids.length).toBeGreaterThan(0);
    });

    it("should filter by single status", async () => {
      const ids = await storage.list({ status: "COMPLETED" });
      expect(ids).toHaveLength(1);
    });

    it("should filter by multiple statuses", async () => {
      const ids = await storage.list({ status: ["COMPLETED", "FAILED"] });
      expect(ids).toHaveLength(2);
    });
  });

  // ── Task Statistics ───────────────────────────────────────────────────

  describe("Task Statistics", () => {
    it("should compute task stats", async () => {
      const batch = createTaskBatch(12);
      await storage.saveBatch(batch.map(({ id, data, metadata }) => ({ id, data, metadata })));

      const stats = await storage.getTaskStats();
      expect(stats.total).toBe(12);
      expect(stats.byStatus.QUEUED).toBe(2);
      expect(stats.byStatus.RUNNING).toBe(2);
    });

    it("should return zero stats when no tasks exist", async () => {
      const stats = await storage.getTaskStats();
      expect(stats.total).toBe(0);
    });
  });

  // ── Cleanup ───────────────────────────────────────────────────────────

  describe("Task Cleanup", () => {
    it("should cleanup expired tasks", async () => {
      const oldTime = Date.now() - 86400000;
      await storage.save("old-task", createTestData(8), createCompletedTaskMetadata({
        taskId: "old-task",
        completeTime: oldTime,
      }));
      await storage.save("recent-task", createTestData(8), createCompletedTaskMetadata({
        taskId: "recent-task",
        completeTime: Date.now(),
      }));

      const removed = await storage.cleanupTasks(3600000);
      expect(removed).toBe(1);

      expect(await storage.exists("old-task")).toBe(false);
      expect(await storage.exists("recent-task")).toBe(true);
    });

    it("should handle cleanup on empty storage", async () => {
      expect(await storage.cleanupTasks(3600000)).toBe(0);
    });
  });

  // ── Batch Operations ──────────────────────────────────────────────────

  describe("Batch Operations", () => {
    it("should save and load batch with persistence", async () => {
      const batch = createTaskBatch(5);
      await storage.saveBatch(batch.map(({ id, data, metadata }) => ({ id, data, metadata })));

      await storage.close();
      storage = new JsonTaskStorage({ baseDir: testDir });
      await storage.initialize();

      expect(await storage.list()).toHaveLength(5);
    });

    it("should delete batch", async () => {
      const batch = createTaskBatch(3);
      await storage.saveBatch(batch.map(({ id, data, metadata }) => ({ id, data, metadata })));

      await storage.deleteBatch(batch.map(({ id }) => id));
      expect(await storage.list()).toHaveLength(0);
    });
  });

  // ── Clear ─────────────────────────────────────────────────────────────

  describe("Clear", () => {
    it("should clear all tasks and files", async () => {
      await storage.save(TEST_TASK_ID, createTestData(), createTaskMetadata());
      await storage.clear();

      expect(await storage.list()).toHaveLength(0);

      const metaDir = path.join(testDir, "metadata", "task");
      expect(await fs.readdir(metaDir)).toHaveLength(0);
    });
  });
});