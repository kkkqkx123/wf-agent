/**
 * JsonTaskStorage Tests
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as fs from "fs/promises";
import * as path from "path";
import * as os from "os";
import { JsonTaskStorage } from "../json-task-storage.js";
import type { TaskStorageMetadata, TaskStatus } from "@wf-agent/types";

describe("JsonTaskStorage", () => {
  let storage: JsonTaskStorage;
  let tempDir: string;

  const createMetadata = (overrides?: Partial<TaskStorageMetadata>): TaskStorageMetadata => ({
    taskId: "task-1",
    threadId: "thread-1",
    workflowId: "workflow-1",
    status: "QUEUED",
    submitTime: Date.now(),
    ...overrides,
  });

  beforeEach(async () => {
    tempDir = await fs.mkdtemp(path.join(os.tmpdir(), "json-task-test-"));
    storage = new JsonTaskStorage({ baseDir: tempDir });
    await storage.initialize();
  });

  afterEach(async () => {
    await storage.close();
    await fs.rm(tempDir, { recursive: true, force: true });
  });

  describe("save / load", () => {
    it("should save and load task", async () => {
      const taskId = "task-1";
      const data = new Uint8Array([1, 2, 3, 4, 5]);
      const metadata = createMetadata();

      await storage.save(taskId, data, metadata);

      const loaded = await storage.load(taskId);
      expect(loaded).not.toBeNull();
      expect(loaded).toEqual(data);
    });

    it("should return null for non-existent task", async () => {
      const loaded = await storage.load("non-existent");
      expect(loaded).toBeNull();
    });
  });

  describe("delete", () => {
    it("should delete task", async () => {
      const taskId = "task-1";
      await storage.save(taskId, new Uint8Array([1]), createMetadata());

      await storage.delete(taskId);

      const loaded = await storage.load(taskId);
      expect(loaded).toBeNull();
    });
  });

  describe("list", () => {
    beforeEach(async () => {
      await storage.save(
        "task-1",
        new Uint8Array([1]),
        createMetadata({
          taskId: "task-1",
          threadId: "thread-1",
          workflowId: "workflow-1",
          status: "QUEUED",
          submitTime: 1000,
        }),
      );
      await storage.save(
        "task-2",
        new Uint8Array([2]),
        createMetadata({
          taskId: "task-2",
          threadId: "thread-1",
          workflowId: "workflow-2",
          status: "RUNNING",
          submitTime: 2000,
          startTime: 2500,
        }),
      );
      await storage.save(
        "task-3",
        new Uint8Array([3]),
        createMetadata({
          taskId: "task-3",
          threadId: "thread-2",
          workflowId: "workflow-1",
          status: "COMPLETED",
          submitTime: 3000,
          startTime: 3500,
          completeTime: 4000,
        }),
      );
    });

    it("should list all tasks", async () => {
      const ids = await storage.list();
      expect(ids).toHaveLength(3);
    });

    it("should filter by threadId", async () => {
      const ids = await storage.list({ threadId: "thread-1" });
      expect(ids).toHaveLength(2);
    });

    it("should filter by workflowId", async () => {
      const ids = await storage.list({ workflowId: "workflow-1" });
      expect(ids).toHaveLength(2);
    });

    it("should filter by single status", async () => {
      const ids = await storage.list({ status: "QUEUED" });
      expect(ids).toHaveLength(1);
      expect(ids).toContain("task-1");
    });

    it("should filter by multiple statuses", async () => {
      const ids = await storage.list({ status: ["QUEUED", "RUNNING"] as TaskStatus[] });
      expect(ids).toHaveLength(2);
    });

    it("should filter by submitTime range", async () => {
      const ids = await storage.list({ submitTimeFrom: 1500, submitTimeTo: 2500 });
      expect(ids).toHaveLength(1);
      expect(ids).toContain("task-2");
    });

    it("should filter by startTime range", async () => {
      const ids = await storage.list({ startTimeFrom: 2000, startTimeTo: 3000 });
      expect(ids).toHaveLength(1);
      expect(ids).toContain("task-2");
    });

    it("should filter by completeTime range", async () => {
      const ids = await storage.list({ completeTimeFrom: 3500, completeTimeTo: 4500 });
      expect(ids).toHaveLength(1);
      expect(ids).toContain("task-3");
    });

    it("should sort by submitTime descending by default", async () => {
      const ids = await storage.list();
      expect(ids).toEqual(["task-3", "task-2", "task-1"]);
    });

    it("should sort by submitTime ascending", async () => {
      const ids = await storage.list({ sortBy: "submitTime", sortOrder: "asc" });
      expect(ids).toEqual(["task-1", "task-2", "task-3"]);
    });

    it("should support pagination", async () => {
      const ids = await storage.list({ limit: 2 });
      expect(ids).toHaveLength(2);

      const ids2 = await storage.list({ limit: 2, offset: 2 });
      expect(ids2).toHaveLength(1);
    });
  });

  describe("exists", () => {
    it("should return true for existing task", async () => {
      await storage.save("task-1", new Uint8Array([1]), createMetadata());
      expect(await storage.exists("task-1")).toBe(true);
    });

    it("should return false for non-existent task", async () => {
      expect(await storage.exists("non-existent")).toBe(false);
    });
  });

  describe("getMetadata", () => {
    it("should return metadata for existing task", async () => {
      const metadata = createMetadata({ taskId: "task-1" });
      await storage.save("task-1", new Uint8Array([1]), metadata);

      const loaded = await storage.getMetadata("task-1");
      expect(loaded).toEqual(metadata);
    });

    it("should return null for non-existent task", async () => {
      const loaded = await storage.getMetadata("non-existent");
      expect(loaded).toBeNull();
    });
  });

  describe("getTaskStats", () => {
    beforeEach(async () => {
      // Create tasks with different statuses
      await storage.save(
        "task-1",
        new Uint8Array([1]),
        createMetadata({
          workflowId: "workflow-1",
          status: "COMPLETED",
          submitTime: 1000,
          startTime: 1100,
          completeTime: 1200,
        }),
      );
      await storage.save(
        "task-2",
        new Uint8Array([2]),
        createMetadata({
          workflowId: "workflow-1",
          status: "COMPLETED",
          submitTime: 2000,
          startTime: 2100,
          completeTime: 2300,
        }),
      );
      await storage.save(
        "task-3",
        new Uint8Array([3]),
        createMetadata({
          workflowId: "workflow-2",
          status: "FAILED",
          submitTime: 3000,
        }),
      );
      await storage.save(
        "task-4",
        new Uint8Array([4]),
        createMetadata({
          workflowId: "workflow-1",
          status: "QUEUED",
          submitTime: 4000,
        }),
      );
      await storage.save(
        "task-5",
        new Uint8Array([5]),
        createMetadata({
          workflowId: "workflow-1",
          status: "TIMEOUT",
          submitTime: 5000,
        }),
      );
    });

    it("should return total count", async () => {
      const stats = await storage.getTaskStats();
      expect(stats.total).toBe(5);
    });

    it("should return status breakdown", async () => {
      const stats = await storage.getTaskStats();
      expect(stats.byStatus.COMPLETED).toBe(2);
      expect(stats.byStatus.FAILED).toBe(1);
      expect(stats.byStatus.QUEUED).toBe(1);
      expect(stats.byStatus.TIMEOUT).toBe(1);
    });

    it("should return workflow breakdown", async () => {
      const stats = await storage.getTaskStats();
      expect(stats.byWorkflow["workflow-1"]).toBe(4);
      expect(stats.byWorkflow["workflow-2"]).toBe(1);
    });

    it("should calculate execution time stats", async () => {
      const stats = await storage.getTaskStats();
      // task-1: 1200 - 1100 = 100
      // task-2: 2300 - 2100 = 200
      expect(stats.avgExecutionTime).toBe(150);
      expect(stats.minExecutionTime).toBe(100);
      expect(stats.maxExecutionTime).toBe(200);
    });

    it("should calculate success rate", async () => {
      const stats = await storage.getTaskStats();
      expect(stats.successRate).toBe(2 / 5);
    });

    it("should calculate timeout rate", async () => {
      const stats = await storage.getTaskStats();
      expect(stats.timeoutRate).toBe(1 / 5);
    });

    it("should filter by workflowId", async () => {
      const stats = await storage.getTaskStats({ workflowId: "workflow-1" });
      expect(stats.total).toBe(4);
    });

    it("should filter by time range", async () => {
      const stats = await storage.getTaskStats({ timeFrom: 2500, timeTo: 4500 });
      expect(stats.total).toBe(2);
    });
  });

  describe("cleanupTasks", () => {
    beforeEach(async () => {
      const now = Date.now();
      // Old completed task
      await storage.save(
        "task-old",
        new Uint8Array([1]),
        createMetadata({
          status: "COMPLETED",
          completeTime: now - 10000,
        }),
      );
      // Recent completed task
      await storage.save(
        "task-recent",
        new Uint8Array([2]),
        createMetadata({
          status: "COMPLETED",
          completeTime: now - 100,
        }),
      );
      // Running task (should not be cleaned)
      await storage.save(
        "task-running",
        new Uint8Array([3]),
        createMetadata({
          status: "RUNNING",
        }),
      );
      // Old failed task
      await storage.save(
        "task-failed",
        new Uint8Array([4]),
        createMetadata({
          status: "FAILED",
          completeTime: now - 10000,
        }),
      );
    });

    it("should clean up old completed tasks", async () => {
      const cleaned = await storage.cleanupTasks(5000);
      expect(cleaned).toBe(2);

      const ids = await storage.list();
      expect(ids).toHaveLength(2);
      expect(ids).toContain("task-recent");
      expect(ids).toContain("task-running");
    });

    it("should not clean up recent tasks", async () => {
      const cleaned = await storage.cleanupTasks(50);
      // task-old and task-failed have completeTime = now - 10000, which is older than 50ms
      // task-recent has completeTime = now - 100, which is also older than 50ms
      // So all 3 completed/failed tasks will be cleaned
      expect(cleaned).toBe(3);
    });
  });

  describe("tags filtering", () => {
    beforeEach(async () => {
      await storage.save(
        "task-1",
        new Uint8Array([1]),
        createMetadata({
          tags: ["important", "production"],
        }),
      );
      await storage.save(
        "task-2",
        new Uint8Array([2]),
        createMetadata({
          tags: ["test"],
        }),
      );
    });

    it("should filter by tags", async () => {
      const ids = await storage.list({ tags: ["important"] });
      expect(ids).toHaveLength(1);
      expect(ids).toContain("task-1");
    });
  });

  describe("clear", () => {
    it("should clear all tasks", async () => {
      await storage.save("task-1", new Uint8Array([1]), createMetadata());
      await storage.save("task-2", new Uint8Array([2]), createMetadata());

      await storage.clear();

      const ids = await storage.list();
      expect(ids).toHaveLength(0);
    });
  });
});
