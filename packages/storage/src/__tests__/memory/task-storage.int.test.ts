/**
 * MemoryTaskStorage Integration Test
 *
 * Tests the complete task storage lifecycle:
 * - CRUD lifecycle (save → load → update → delete → stats)
 * - Task list filtering (executionId, workflowId, status, time range, tags, pagination)
 * - Task statistics (counts, execution time, success/timeout rates)
 * - Batch operations
 * - Expired task cleanup
 * - Edge cases (empty data, special characters)
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { MemoryTaskStorage } from "../../memory/memory-task-storage.js";
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

describe("MemoryTaskStorage Integration", () => {
  let storage: MemoryTaskStorage;

  beforeEach(async () => {
    storage = new MemoryTaskStorage();
    await storage.initialize();
  });

  afterEach(async () => {
    await storage.close();
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
      expect(statsAfterSave.byStatus.RUNNING).toBe(1);

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

    it("should overwrite existing task data and preserve metadata merge", async () => {
      const originalMeta = createTaskMetadata({ tags: ["original"] });
      await storage.save(TEST_TASK_ID, createTestData(16), originalMeta);

      // Overwrite with new data and updated metadata
      const updatedMeta = createTaskMetadata({ tags: ["updated"], status: "COMPLETED" });
      await storage.save(TEST_TASK_ID, createTestData(32), updatedMeta);

      const loaded = await storage.load(TEST_TASK_ID);
      expect(loaded).toEqual(createTestData(32));

      const meta = await storage.getMetadata(TEST_TASK_ID);
      expect(meta!.tags).toEqual(["updated"]);
      expect(meta!.status).toBe("COMPLETED");
    });

    it("should handle empty data buffer", async () => {
      const data = new Uint8Array(0);
      await storage.save(TEST_TASK_ID, data, createTaskMetadata());
      const loaded = await storage.load(TEST_TASK_ID);
      expect(loaded).toEqual(data);
    });
  });

  // ── List Filtering ────────────────────────────────────────────────────

  describe("List Filtering", () => {
    beforeEach(async () => {
      const now = Date.now();
      const tasks = [
        // task-000: QUEUED, execution-1, wf-1, no startTime, no completeTime
        createTaskMetadata({
          taskId: "task-000",
          status: "QUEUED",
          executionId: "execution-1",
          workflowId: "wf-1",
          submitTime: now - 60000,
          startTime: undefined,
          completeTime: undefined,
          tags: ["urgent"],
        }),
        // task-001: RUNNING, execution-1, wf-1
        createTaskMetadata({
          taskId: "task-001",
          status: "RUNNING",
          executionId: "execution-1",
          workflowId: "wf-1",
          submitTime: now - 50000,
          startTime: now - 45000,
          completeTime: undefined,
          tags: ["urgent", "processing"],
        }),
        // task-002: COMPLETED, execution-1, wf-1
        createTaskMetadata({
          taskId: "task-002",
          status: "COMPLETED",
          executionId: "execution-1",
          workflowId: "wf-1",
          submitTime: now - 40000,
          startTime: now - 35000,
          completeTime: now - 5000,
          tags: ["completed"],
        }),
        // task-003: FAILED, execution-2, wf-2
        createTaskMetadata({
          taskId: "task-003",
          status: "FAILED",
          executionId: "execution-2",
          workflowId: "wf-2",
          submitTime: now - 30000,
          startTime: now - 25000,
          completeTime: now - 10000,
          error: "timeout",
          tags: ["error"],
        }),
        // task-004: CANCELLED, execution-2, wf-2, no startTime
        createTaskMetadata({
          taskId: "task-004",
          status: "CANCELLED",
          executionId: "execution-2",
          workflowId: "wf-2",
          submitTime: now - 20000,
          startTime: undefined,
          completeTime: undefined,
          tags: [],
        }),
        // task-005: TIMEOUT, execution-3, wf-3
        createTaskMetadata({
          taskId: "task-005",
          status: "TIMEOUT",
          executionId: "execution-3",
          workflowId: "wf-3",
          submitTime: now - 10000,
          startTime: now - 8000,
          completeTime: now - 1000,
          tags: ["timeout", "urgent"],
        }),
      ];

      for (const t of tasks) {
        await storage.save(t.taskId, createTestData(8), t);
      }
    });

    it("should list all tasks", async () => {
      const ids = await storage.list();
      expect(ids).toHaveLength(6);
    });

    it("should filter by executionId", async () => {
      const ids = await storage.list({ executionId: "execution-1" });
      expect(ids).toHaveLength(3);
      expect(ids).toEqual(expect.arrayContaining(["task-000", "task-001", "task-002"]));
    });

    it("should filter by workflowId", async () => {
      const ids = await storage.list({ workflowId: "wf-2" });
      expect(ids).toHaveLength(2);
      expect(ids).toEqual(expect.arrayContaining(["task-003", "task-004"]));
    });

    it("should filter by single status", async () => {
      const ids = await storage.list({ status: "COMPLETED" });
      expect(ids).toHaveLength(1);
      expect(ids[0]).toBe("task-002");
    });

    it("should filter by multiple statuses", async () => {
      const ids = await storage.list({ status: ["COMPLETED", "FAILED"] });
      expect(ids).toHaveLength(2);
      expect(ids).toEqual(expect.arrayContaining(["task-002", "task-003"]));
    });

    it("should filter by submitTime range", async () => {
      const now = Date.now();
      const ids = await storage.list({
        submitTimeFrom: now - 55000,
        submitTimeTo: now - 15000,
      });
      expect(ids).toHaveLength(4);
      expect(ids).toEqual(expect.arrayContaining(["task-001", "task-002", "task-003", "task-004"]));
    });

    it("should filter by startTime range", async () => {
      const now = Date.now();
      const ids = await storage.list({
        startTimeFrom: now - 40000,
        startTimeTo: now - 5000,
      });
      expect(ids).toHaveLength(3);
      expect(ids).toEqual(expect.arrayContaining(["task-002", "task-003", "task-005"]));
    });

    it("should filter by completeTime range", async () => {
      const now = Date.now();
      const ids = await storage.list({
        completeTimeFrom: now - 12000,
        completeTimeTo: now,
      });
      expect(ids).toHaveLength(3);
      expect(ids).toEqual(expect.arrayContaining(["task-002", "task-003", "task-005"]));
    });

    it("should filter by tags (match any)", async () => {
      const ids = await storage.list({ tags: ["urgent"] });
      expect(ids).toHaveLength(3);
      expect(ids).toEqual(expect.arrayContaining(["task-000", "task-001", "task-005"]));
    });

    it("should support pagination with offset and limit", async () => {
      const allIds = await storage.list();

      const page1 = await storage.list({ limit: 2, offset: 0 });
      expect(page1).toHaveLength(2);

      const page2 = await storage.list({ limit: 2, offset: 2 });
      expect(page2).toHaveLength(2);

      // Pages should not overlap
      const overlap = page1.filter((id) => page2.includes(id));
      expect(overlap).toHaveLength(0);

      // Together they should be a subset of all IDs
      expect([...page1, ...page2].length).toBeLessThanOrEqual(allIds.length);
    });

    it("should combine multiple filters", async () => {
      const ids = await storage.list({
        executionId: "execution-1",
        status: ["QUEUED", "RUNNING"],
      });
      expect(ids).toHaveLength(2);
      expect(ids).toEqual(expect.arrayContaining(["task-000", "task-001"]));
    });

    it("should return empty list when no matches", async () => {
      const ids = await storage.list({ executionId: "non-existent-exec" });
      expect(ids).toEqual([]);
    });
  });

  // ── Task Statistics ───────────────────────────────────────────────────

  describe("Task Statistics", () => {
    it("should compute task stats correctly", async () => {
      const batch = createTaskBatch(12);
      await storage.saveBatch(batch.map(({ id, data, metadata }) => ({ id, data, metadata })));

      const stats = await storage.getTaskStats();
      expect(stats.total).toBe(12);
      expect(stats.byStatus.QUEUED).toBe(2);
      expect(stats.byStatus.RUNNING).toBe(2);
      expect(stats.byStatus.COMPLETED).toBe(2);
      expect(stats.byStatus.FAILED).toBe(2);
      expect(stats.byStatus.CANCELLED).toBe(2);
      expect(stats.byStatus.TIMEOUT).toBe(2);
    });

    it("should compute byWorkflow breakdown", async () => {
      const batch = createTaskBatch(6);
      await storage.saveBatch(batch.map(({ id, data, metadata }) => ({ id, data, metadata })));

      const stats = await storage.getTaskStats({ workflowId: TEST_WORKFLOW_ID });
      expect(stats.total).toBe(6);
      expect(Object.keys(stats.byWorkflow)).toContain(TEST_WORKFLOW_ID);
      expect(stats.byWorkflow[TEST_WORKFLOW_ID]).toBe(6);
    });

    it("should filter stats by workflowId", async () => {
      const batch = createTaskBatch(6);
      // Override one to a different workflow
      batch[0]!.metadata.workflowId = "other-wf";
      await storage.saveBatch(batch.map(({ id, data, metadata }) => ({ id, data, metadata })));

      const stats = await storage.getTaskStats({ workflowId: TEST_WORKFLOW_ID });
      expect(stats.total).toBe(5);
    });

    it("should filter stats by time range", async () => {
      const now = Date.now();
      const tasks = [
        createTaskMetadata({
          taskId: "old-task",
          submitTime: now - 86400000,
        }),
        createTaskMetadata({
          taskId: "recent-task",
          submitTime: now - 60000,
        }),
      ];
      for (const t of tasks) {
        await storage.save(t.taskId, createTestData(8), t);
      }

      const stats = await storage.getTaskStats({
        timeFrom: now - 3600000,
        timeTo: now,
      });
      expect(stats.total).toBe(1);
      expect(Object.keys(stats.byWorkflow)).toContain(TEST_WORKFLOW_ID);
    });

    it("should compute successRate and timeoutRate", async () => {
      // 3 COMPLETED + 1 TIMEOUT out of 6 total
      const tasks = [
        createTaskMetadata({ taskId: "t-01", status: "COMPLETED" }),
        createTaskMetadata({ taskId: "t-02", status: "COMPLETED" }),
        createTaskMetadata({ taskId: "t-03", status: "COMPLETED" }),
        createTaskMetadata({ taskId: "t-04", status: "TIMEOUT" }),
        createTaskMetadata({ taskId: "t-05", status: "FAILED" }),
        createTaskMetadata({ taskId: "t-06", status: "CANCELLED" }),
      ];
      for (const t of tasks) {
        await storage.save(t.taskId, createTestData(8), t);
      }

      const stats = await storage.getTaskStats();
      expect(stats.successRate).toBeCloseTo(0.5, 2);
      expect(stats.timeoutRate).toBeCloseTo(1 / 6, 2);
    });

    it("should compute execution time statistics", async () => {
      const now = Date.now();
      const tasks = [
        createTaskMetadata({
          taskId: "t-fast",
          status: "COMPLETED",
          startTime: now - 2000,
          completeTime: now - 1000,
        }),
        createTaskMetadata({
          taskId: "t-slow",
          status: "COMPLETED",
          startTime: now - 10000,
          completeTime: now - 1000,
        }),
        createTaskMetadata({
          taskId: "t-noexec",
          status: "QUEUED",
          startTime: undefined,
          completeTime: undefined,
        }),
      ];
      for (const t of tasks) {
        await storage.save(t.taskId, createTestData(8), t);
      }

      const stats = await storage.getTaskStats();
      expect(stats.avgExecutionTime).toBeCloseTo((1000 + 9000) / 2, -1);
      expect(stats.maxExecutionTime).toBe(9000);
      expect(stats.minExecutionTime).toBe(1000);
    });

    it("should return zero stats when no tasks exist", async () => {
      const stats = await storage.getTaskStats();
      expect(stats.total).toBe(0);
      expect(stats.byStatus).toBeDefined();
      expect(Object.keys(stats.byStatus)).toHaveLength(0);
      expect(Object.keys(stats.byWorkflow)).toHaveLength(0);
      expect(stats.successRate).toBeUndefined();
      expect(stats.timeoutRate).toBeUndefined();
    });
  });

  // ── Batch Operations ──────────────────────────────────────────────────

  describe("Batch Operations", () => {
    it("should save and load batch", async () => {
      const batch = createTaskBatch(5);
      await storage.saveBatch(batch.map(({ id, data, metadata }) => ({ id, data, metadata })));

      const loaded = await storage.loadBatch(batch.map(({ id }) => id));
      expect(loaded).toHaveLength(5);
      expect(loaded.every((item) => item.data !== null)).toBe(true);

      // Verify IDs match
      for (let i = 0; i < batch.length; i++) {
        expect(loaded[i]!.id).toBe(batch[i]!.id);
      }
    });

    it("should return null for non-existent IDs in loadBatch", async () => {
      const batch = createTaskBatch(3);
      await storage.saveBatch(batch.map(({ id, data, metadata }) => ({ id, data, metadata })));

      const ids = [batch[0]!.id, "non-existent", batch[1]!.id];
      const loaded = await storage.loadBatch(ids);
      expect(loaded).toHaveLength(3);
      expect(loaded[0]!.data).not.toBeNull();
      expect(loaded[1]!.data).toBeNull();
      expect(loaded[2]!.data).not.toBeNull();
    });

    it("should delete batch", async () => {
      const batch = createTaskBatch(3);
      await storage.saveBatch(batch.map(({ id, data, metadata }) => ({ id, data, metadata })));

      await storage.deleteBatch(batch.map(({ id }) => id));
      expect(await storage.list()).toHaveLength(0);
    });
  });

  // ── Cleanup ───────────────────────────────────────────────────────────

  describe("Task Cleanup", () => {
    it("should cleanup expired completed tasks", async () => {
      const oldTime = Date.now() - 86400000;
      await storage.save(
        "old-completed-task",
        createTestData(8),
        createCompletedTaskMetadata({
          taskId: "old-completed-task",
          completeTime: oldTime,
        }),
      );
      await storage.save(
        "recent-completed-task",
        createTestData(8),
        createCompletedTaskMetadata({
          taskId: "recent-completed-task",
          completeTime: Date.now(),
        }),
      );

      const removed = await storage.cleanupTasks(3600000);
      expect(removed).toBe(1);

      expect(await storage.exists("recent-completed-task")).toBe(true);
      expect(await storage.exists("old-completed-task")).toBe(false);
    });

    it("should cleanup expired failed and cancelled tasks", async () => {
      const oldTime = Date.now() - 86400000;
      await storage.save(
        "old-failed",
        createTestData(8),
        createFailedTaskMetadata({
          taskId: "old-failed",
          completeTime: oldTime,
        }),
      );
      await storage.save(
        "old-cancelled",
        createTestData(8),
        createTaskMetadata({
          taskId: "old-cancelled",
          status: "CANCELLED",
          completeTime: oldTime,
        }),
      );

      const removed = await storage.cleanupTasks(3600000);
      expect(removed).toBe(2);
    });

    it("should not remove non-terminal tasks during cleanup", async () => {
      await storage.save(
        "running-task",
        createTestData(8),
        createTaskMetadata({
          taskId: "running-task",
          status: "RUNNING",
          submitTime: Date.now() - 86400000,
        }),
      );

      const removed = await storage.cleanupTasks(3600000);
      expect(removed).toBe(0);
      expect(await storage.exists("running-task")).toBe(true);
    });

    it("should handle cleanup on empty storage", async () => {
      const removed = await storage.cleanupTasks(3600000);
      expect(removed).toBe(0);
    });
  });

  // ── Edge Cases ────────────────────────────────────────────────────────

  describe("Edge Cases", () => {
    it("should handle tasks with error details", async () => {
      const metadata = createFailedTaskMetadata();
      await storage.save(metadata.taskId, createTestData(), metadata);

      const loaded = await storage.load(metadata.taskId);
      expect(loaded).not.toBeNull();

      const meta = await storage.getMetadata(metadata.taskId);
      expect(meta!.error).toBe("Task execution timeout");
      expect(meta!.errorStack).toContain("Error: timeout");
    });

    it("should handle many tasks", async () => {
      const count = 50;
      const batch = createTaskBatch(count);
      await storage.saveBatch(batch.map(({ id, data, metadata }) => ({ id, data, metadata })));

      expect(await storage.list()).toHaveLength(count);
      const stats = await storage.getTaskStats();
      expect(stats.total).toBe(count);
    });

    it("should handle task IDs with special characters", async () => {
      const ids = ["task-123", "task_456", "task.789", "task/001"];
      for (const id of ids) {
        await storage.save(id, createTestData(8), createTaskMetadata({ taskId: id }));
      }

      const allIds = await storage.list();
      for (const id of ids) {
        expect(allIds).toContain(id);
      }
    });

    it("should handle tags as empty array", async () => {
      await storage.save(
        TEST_TASK_ID,
        createTestData(),
        createTaskMetadata({ tags: [] }),
      );

      const loaded = await storage.load(TEST_TASK_ID);
      expect(loaded).not.toBeNull();

      const meta = await storage.getMetadata(TEST_TASK_ID);
      expect(meta!.tags).toEqual([]);
    });
  });

  // ── Clear ─────────────────────────────────────────────────────────────

  describe("Clear", () => {
    it("should clear all tasks", async () => {
      const batch = createTaskBatch(5);
      await storage.saveBatch(batch.map(({ id, data, metadata }) => ({ id, data, metadata })));
      expect(await storage.list()).toHaveLength(5);

      await storage.clear();

      expect(await storage.list()).toHaveLength(0);
      expect(await storage.load(TEST_TASK_ID)).toBeNull();
      expect(await storage.getTaskStats()).toEqual({
        total: 0,
        byStatus: {},
        byWorkflow: {},
        avgExecutionTime: undefined,
        maxExecutionTime: undefined,
        minExecutionTime: undefined,
        successRate: undefined,
        timeoutRate: undefined,
      });
    });
  });
});