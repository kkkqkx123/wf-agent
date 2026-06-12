import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { MemoryTaskStorage } from "@wf-agent/storage";
import type { TaskStorageAdapter } from "@wf-agent/storage";
import type { TaskStorageMetadata, TaskStatus } from "@wf-agent/types";

const createMetadata = (
  overrides: Partial<TaskStorageMetadata> & { status?: TaskStatus } = {},
): TaskStorageMetadata => ({
  taskId: "task-test",
  executionId: "exec-test",
  workflowId: "wf-test",
  status: "QUEUED",
  submitTime: Date.now(),
  ...overrides,
});

describe("Task Storage E2E", () => {
  let storage: TaskStorageAdapter;

  beforeEach(async () => {
    storage = new MemoryTaskStorage();
    await storage.initialize();
  });

  afterEach(async () => {
    await storage.clear();
    await storage.close();
  });

  describe("Basic CRUD Operations", () => {
    it("should save and load a task", async () => {
      const data = new Uint8Array([1, 2, 3]);
      const metadata = createMetadata({ taskId: "task-1" });

      await storage.save("task-1", data, metadata);
      const loaded = await storage.load("task-1");

      expect(loaded).not.toBeNull();
      expect(Array.from(loaded!)).toEqual([1, 2, 3]);
    });

    it("should return null when loading non-existent task", async () => {
      const loaded = await storage.load("non-existent");
      expect(loaded).toBeNull();
    });

    it("should delete a task", async () => {
      await storage.save("task-1", new Uint8Array([1]), createMetadata({ taskId: "task-1" }));
      expect(await storage.exists("task-1")).toBe(true);

      await storage.delete("task-1");
      expect(await storage.exists("task-1")).toBe(false);
    });

    it("should list all task IDs", async () => {
      await storage.save("task-1", new Uint8Array([1]), createMetadata({ taskId: "task-1" }));
      await storage.save("task-2", new Uint8Array([2]), createMetadata({ taskId: "task-2" }));

      const ids = await storage.list();
      expect(ids.sort()).toEqual(["task-1", "task-2"]);
    });
  });

  describe("Metadata Operations", () => {
    it("should retrieve metadata for a saved task", async () => {
      const metadata = createMetadata({
        taskId: "task-1",
        executionId: "exec-1",
        workflowId: "wf-1",
        status: "RUNNING",
        tags: ["urgent"],
      });

      await storage.save("task-1", new Uint8Array([1]), metadata);
      const retrieved = await storage.getMetadata("task-1");

      expect(retrieved).not.toBeNull();
      expect(retrieved!.taskId).toBe("task-1");
      expect(retrieved!.executionId).toBe("exec-1");
      expect(retrieved!.workflowId).toBe("wf-1");
      expect(retrieved!.status).toBe("RUNNING");
      expect(retrieved!.tags).toEqual(["urgent"]);
    });
  });

  describe("Filtered Listing", () => {
    const now = Date.now();

    beforeEach(async () => {
      const tasks = [
        {
          id: "task-1",
          executionId: "exec-1",
          workflowId: "wf-1",
          status: "QUEUED" as TaskStatus,
          submitTime: now + 100,
        },
        {
          id: "task-2",
          executionId: "exec-1",
          workflowId: "wf-1",
          status: "RUNNING" as TaskStatus,
          submitTime: now + 200,
          startTime: now + 300,
        },
        {
          id: "task-3",
          executionId: "exec-1",
          workflowId: "wf-1",
          status: "COMPLETED" as TaskStatus,
          submitTime: now + 300,
          startTime: now + 400,
          completeTime: now + 500,
        },
        {
          id: "task-4",
          executionId: "exec-2",
          workflowId: "wf-2",
          status: "FAILED" as TaskStatus,
          submitTime: now + 400,
          tags: ["critical"],
        },
        {
          id: "task-5",
          executionId: "exec-2",
          workflowId: "wf-2",
          status: "CANCELLED" as TaskStatus,
          submitTime: now + 500,
          tags: ["test"],
        },
      ];

      for (const task of tasks) {
        const { id, ...metaFields } = task;
        await storage.save(id, new Uint8Array([1]), createMetadata(metaFields));
      }
    });

    it("should filter by execution ID", async () => {
      const ids = await storage.list({ executionId: "exec-1" });
      expect(ids.sort()).toEqual(["task-1", "task-2", "task-3"]);
    });

    it("should filter by workflow ID", async () => {
      const ids = await storage.list({ workflowId: "wf-2" });
      expect(ids.sort()).toEqual(["task-4", "task-5"]);
    });

    it("should filter by single status", async () => {
      const ids = await storage.list({ status: "RUNNING" });
      expect(ids).toEqual(["task-2"]);
    });

    it("should filter by multiple statuses", async () => {
      const ids = await storage.list({ status: ["QUEUED", "RUNNING"] });
      expect(ids.sort()).toEqual(["task-1", "task-2"]);
    });

    it("should filter by tags", async () => {
      const ids = await storage.list({ tags: ["critical"] });
      expect(ids).toEqual(["task-4"]);
    });

    it("should filter by submit time range", async () => {
      const ids = await storage.list({ submitTimeFrom: now + 250, submitTimeTo: now + 450 });
      expect(ids.sort()).toEqual(["task-3", "task-4"]);
    });

    it("should apply pagination", async () => {
      const paged = await storage.list({ limit: 2, offset: 1 });
      expect(paged).toHaveLength(2);
    });

    it("should filter by start time range", async () => {
      const ids = await storage.list({ startTimeFrom: now + 250, startTimeTo: now + 500 });
      expect(ids.sort()).toEqual(["task-2", "task-3"]);
    });

    it("should filter by complete time range", async () => {
      const ids = await storage.list({ completeTimeFrom: now + 250, completeTimeTo: now + 600 });
      expect(ids).toEqual(["task-3"]);
    });

    it("should return empty when no tasks match all filters", async () => {
      const ids = await storage.list({ executionId: "exec-1", status: "FAILED" });
      expect(ids).toEqual([]);
    });
  });

  describe("Task Statistics", () => {
    const now = Date.now();

    beforeEach(async () => {
      const tasks = [
        {
          id: "task-1",
          workflowId: "wf-1",
          status: "COMPLETED" as TaskStatus,
          submitTime: now + 100,
          startTime: now + 200,
          completeTime: now + 500,
        },
        {
          id: "task-2",
          workflowId: "wf-1",
          status: "COMPLETED" as TaskStatus,
          submitTime: now + 200,
          startTime: now + 300,
          completeTime: now + 400,
        },
        {
          id: "task-3",
          workflowId: "wf-1",
          status: "RUNNING" as TaskStatus,
          submitTime: now + 300,
        },
        { id: "task-4", workflowId: "wf-2", status: "FAILED" as TaskStatus, submitTime: now + 400 },
        { id: "task-5", workflowId: "wf-2", status: "QUEUED" as TaskStatus, submitTime: now + 500 },
      ];

      for (const task of tasks) {
        const { id, ...metaFields } = task;
        await storage.save(id, new Uint8Array([1]), createMetadata(metaFields));
      }
    });

    it("should return overall task statistics", async () => {
      const stats = await storage.getTaskStats();
      expect(stats.total).toBe(5);
      expect(stats.byStatus["COMPLETED"]).toBe(2);
      expect(stats.byStatus["RUNNING"]).toBe(1);
      expect(stats.byStatus["FAILED"]).toBe(1);
      expect(stats.byStatus["QUEUED"]).toBe(1);
    });

    it("should filter task statistics by workflow", async () => {
      const stats = await storage.getTaskStats({ workflowId: "wf-1" });
      expect(stats.total).toBe(3);
      expect(stats.byStatus["COMPLETED"]).toBe(2);
      expect(stats.byStatus["RUNNING"]).toBe(1);
    });

    it("should calculate average execution time", async () => {
      const stats = await storage.getTaskStats({ workflowId: "wf-1" });
      expect(stats.avgExecutionTime).toBeDefined();
      expect(stats.maxExecutionTime).toBeDefined();
    });

    it("should filter statistics by time range", async () => {
      const now2 = Date.now();
      const tasks = [
        {
          id: "ts-1",
          workflowId: "wf-1",
          status: "COMPLETED" as TaskStatus,
          submitTime: now2 - 5000,
          startTime: now2 - 4000,
          completeTime: now2 - 3000,
        },
        {
          id: "ts-2",
          workflowId: "wf-1",
          status: "COMPLETED" as TaskStatus,
          submitTime: now2 - 2000,
          startTime: now2 - 1500,
          completeTime: now2 - 1000,
        },
      ];
      for (const task of tasks) {
        const { id, ...metaFields } = task;
        await storage.save(id, new Uint8Array([1]), createMetadata(metaFields));
      }

      // Only first task in range
      const stats = await storage.getTaskStats({ timeFrom: now2 - 6000, timeTo: now2 - 3000 });
      expect(stats.total).toBe(1);
      expect(stats.byStatus["COMPLETED"]).toBe(1);
    });

    it("should return empty stats when no tasks match time filter", async () => {
      const stats = await storage.getTaskStats({ timeFrom: Date.now() + 86400000 });
      expect(stats.total).toBe(0);
    });
  });

  describe("Task Cleanup", () => {
    const now = Date.now();

    beforeEach(async () => {
      const tasks = [
        {
          id: "task-old-completed",
          status: "COMPLETED" as TaskStatus,
          submitTime: now - 200000,
          startTime: now - 190000,
          completeTime: now - 180000,
        },
        {
          id: "task-old-failed",
          status: "FAILED" as TaskStatus,
          submitTime: now - 200000,
          completeTime: now - 180000,
        },
        {
          id: "task-old-cancelled",
          status: "CANCELLED" as TaskStatus,
          submitTime: now - 200000,
          completeTime: now - 180000,
        },
        {
          id: "task-recent-completed",
          status: "COMPLETED" as TaskStatus,
          submitTime: now - 50000,
          completeTime: now - 40000,
        },
        { id: "task-running", status: "RUNNING" as TaskStatus, submitTime: now - 10000 },
        { id: "task-queued", status: "QUEUED" as TaskStatus, submitTime: now },
      ];

      for (const task of tasks) {
        const { id, ...metaFields } = task;
        await storage.save(id, new Uint8Array([1]), createMetadata(metaFields));
      }
    });

    it("should cleanup expired terminal tasks", async () => {
      const deleted = await storage.cleanupTasks(100000);
      expect(deleted).toBe(3);
      expect(await storage.exists("task-old-completed")).toBe(false);
      expect(await storage.exists("task-old-failed")).toBe(false);
      expect(await storage.exists("task-old-cancelled")).toBe(false);
    });

    it("should keep non-terminal and recent tasks after cleanup", async () => {
      await storage.cleanupTasks(100000);

      expect(await storage.exists("task-recent-completed")).toBe(true);
      expect(await storage.exists("task-running")).toBe(true);
      expect(await storage.exists("task-queued")).toBe(true);
    });

    it("should return 0 when retention time does not expire any tasks", async () => {
      const deleted = await storage.cleanupTasks(86400000);
      expect(deleted).toBe(0);
    });
  });

  describe("Batch Operations", () => {
    it("should save multiple tasks in batch", async () => {
      const items = [
        { id: "task-1", data: new Uint8Array([1]), metadata: createMetadata({ taskId: "task-1" }) },
        { id: "task-2", data: new Uint8Array([2]), metadata: createMetadata({ taskId: "task-2" }) },
      ];

      await storage.saveBatch(items);
      expect((await storage.list()).sort()).toEqual(["task-1", "task-2"]);
    });

    it("should load multiple tasks in batch", async () => {
      await storage.save("task-1", new Uint8Array([10]), createMetadata({ taskId: "task-1" }));
      await storage.save("task-2", new Uint8Array([20]), createMetadata({ taskId: "task-2" }));

      const results = await storage.loadBatch(["task-1", "non-existent"]);
      expect(results).toHaveLength(2);
      expect(Array.from(results[0]!.data!)).toEqual([10]);
      expect(results[1]!.data).toBeNull();
    });

    it("should delete multiple tasks in batch", async () => {
      await storage.saveBatch([
        { id: "task-1", data: new Uint8Array([1]), metadata: createMetadata({ taskId: "task-1" }) },
        { id: "task-2", data: new Uint8Array([2]), metadata: createMetadata({ taskId: "task-2" }) },
        { id: "task-3", data: new Uint8Array([3]), metadata: createMetadata({ taskId: "task-3" }) },
      ]);

      await storage.deleteBatch(["task-1", "task-3"]);
      expect((await storage.list()).sort()).toEqual(["task-2"]);
    });
  });

  describe("Metrics", () => {
    it("should track storage metrics", async () => {
      await storage.save("task-1", new Uint8Array([1]), createMetadata({ taskId: "task-1" }));
      await storage.load("task-1");

      const metrics = await storage.getMetrics();
      expect(metrics.saveCount).toBe(1);
      expect(metrics.loadCount).toBe(1);
    });
  });

  describe("Edge Cases", () => {
    it("should handle task lifecycle status transitions", async () => {
      const metadata = createMetadata({
        taskId: "task-lifecycle",
        executionId: "exec-1",
        workflowId: "wf-1",
        status: "QUEUED",
      });

      await storage.save("task-lifecycle", new Uint8Array([1]), metadata);

      metadata.status = "RUNNING";
      metadata.startTime = Date.now();
      await storage.save("task-lifecycle", new Uint8Array([2]), metadata);

      metadata.status = "COMPLETED";
      metadata.completeTime = Date.now();
      await storage.save("task-lifecycle", new Uint8Array([3]), metadata);

      const loaded = await storage.load("task-lifecycle");
      expect(Array.from(loaded!)).toEqual([3]);

      const finalMetadata = await storage.getMetadata("task-lifecycle");
      expect(finalMetadata!.status).toBe("COMPLETED");
      expect(finalMetadata!.startTime).toBeGreaterThan(0);
      expect(finalMetadata!.completeTime).toBeGreaterThan(0);
    });

    it("should handle tasks with error information", async () => {
      const metadata = createMetadata({
        taskId: "task-error",
        status: "FAILED",
        error: "Something went wrong",
        errorStack: "Error: Something went wrong\n    at Object.<anonymous> (test.ts:1:1)",
      });

      await storage.save("task-error", new Uint8Array([1]), metadata);

      const retrieved = await storage.getMetadata("task-error");
      expect(retrieved!.error).toBe("Something went wrong");
      expect(retrieved!.errorStack).toContain("Error: Something went wrong");
    });

    it("should handle task timeout metadata", async () => {
      const metadata = createMetadata({
        taskId: "task-timeout",
        status: "TIMEOUT",
        timeout: 30000,
        startTime: Date.now() - 60000,
      });

      await storage.save("task-timeout", new Uint8Array([1]), metadata);

      const retrieved = await storage.getMetadata("task-timeout");
      expect(retrieved!.status).toBe("TIMEOUT");
      expect(retrieved!.timeout).toBe(30000);
    });

    it("should return empty list for non-matching filters", async () => {
      await storage.save(
        "task-1",
        new Uint8Array([1]),
        createMetadata({ taskId: "task-1", executionId: "exec-1" }),
      );
      const ids = await storage.list({ executionId: "non-existent" });
      expect(ids).toEqual([]);
    });
  });
});
