/**
 * SqliteThreadStorage Tests
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as fs from "fs/promises";
import * as path from "path";
import * as os from "os";
import { SqliteThreadStorage } from "../sqlite-thread-storage.js";
import type { ThreadStorageMetadata, ThreadStatus } from "@wf-agent/types";

describe("SqliteThreadStorage", () => {
  let storage: SqliteThreadStorage;
  let tempDir: string;
  let dbPath: string;

  const createMetadata = (overrides?: Partial<ThreadStorageMetadata>): ThreadStorageMetadata => ({
    threadId: "thread-1",
    workflowId: "workflow-1",
    workflowVersion: "1.0.0.0",
    status: "RUNNING",
    startTime: Date.now(),
    ...overrides,
  });

  beforeEach(async () => {
    tempDir = await fs.mkdtemp(path.join(os.tmpdir(), "sqlite-thread-test-"));
    dbPath = path.join(tempDir, "test.db");
    storage = new SqliteThreadStorage({ dbPath });
    await storage.initialize();
  });

  afterEach(async () => {
    await storage.close();
    await fs.rm(tempDir, { recursive: true, force: true });
  });

  describe("save / load", () => {
    it("should save and load thread", async () => {
      const threadId = "thread-1";
      const data = new Uint8Array([1, 2, 3, 4, 5]);
      const metadata = createMetadata();

      await storage.save(threadId, data, metadata);

      const loaded = await storage.load(threadId);
      expect(loaded).not.toBeNull();
      expect(loaded).toEqual(data);
    });

    it("should return null for non-existent thread", async () => {
      const loaded = await storage.load("non-existent");
      expect(loaded).toBeNull();
    });

    it("should overwrite existing thread", async () => {
      const threadId = "thread-1";
      const data1 = new Uint8Array([1, 2, 3]);
      const data2 = new Uint8Array([4, 5, 6]);

      await storage.save(threadId, data1, createMetadata());
      await storage.save(threadId, data2, createMetadata());

      const loaded = await storage.load(threadId);
      expect(loaded).toEqual(data2);
    });
  });

  describe("delete", () => {
    it("should delete thread", async () => {
      const threadId = "thread-1";
      await storage.save(threadId, new Uint8Array([1]), createMetadata());

      await storage.delete(threadId);

      const loaded = await storage.load(threadId);
      expect(loaded).toBeNull();
    });
  });

  describe("list", () => {
    beforeEach(async () => {
      await storage.save(
        "thread-1",
        new Uint8Array([1]),
        createMetadata({
          threadId: "thread-1",
          workflowId: "workflow-1",
          status: "RUNNING",
          startTime: 1000,
        }),
      );
      await storage.save(
        "thread-2",
        new Uint8Array([2]),
        createMetadata({
          threadId: "thread-2",
          workflowId: "workflow-1",
          status: "COMPLETED",
          startTime: 2000,
          endTime: 3000,
        }),
      );
      await storage.save(
        "thread-3",
        new Uint8Array([3]),
        createMetadata({
          threadId: "thread-3",
          workflowId: "workflow-2",
          status: "FAILED",
          startTime: 3000,
          endTime: 4000,
        }),
      );
    });

    it("should list all threads", async () => {
      const ids = await storage.list();
      expect(ids).toHaveLength(3);
    });

    it("should filter by workflowId", async () => {
      const ids = await storage.list({ workflowId: "workflow-1" });
      expect(ids).toHaveLength(2);
    });

    it("should filter by single status", async () => {
      const ids = await storage.list({ status: "RUNNING" });
      expect(ids).toHaveLength(1);
      expect(ids).toContain("thread-1");
    });

    it("should filter by multiple statuses", async () => {
      const ids = await storage.list({ status: ["RUNNING", "COMPLETED"] as ThreadStatus[] });
      expect(ids).toHaveLength(2);
    });

    it("should filter by startTime range", async () => {
      const ids = await storage.list({ startTimeFrom: 1500, startTimeTo: 2500 });
      expect(ids).toHaveLength(1);
      expect(ids).toContain("thread-2");
    });

    it("should filter by endTime range", async () => {
      const ids = await storage.list({ endTimeFrom: 2500, endTimeTo: 3500 });
      expect(ids).toHaveLength(1);
      expect(ids).toContain("thread-2");
    });

    it("should sort by startTime descending by default", async () => {
      const ids = await storage.list();
      expect(ids).toEqual(["thread-3", "thread-2", "thread-1"]);
    });

    it("should sort by startTime ascending", async () => {
      const ids = await storage.list({ sortBy: "startTime", sortOrder: "asc" });
      expect(ids).toEqual(["thread-1", "thread-2", "thread-3"]);
    });

    it("should support pagination", async () => {
      const ids = await storage.list({ limit: 2 });
      expect(ids).toHaveLength(2);

      const ids2 = await storage.list({ limit: 2, offset: 2 });
      expect(ids2).toHaveLength(1);
    });
  });

  describe("exists", () => {
    it("should return true for existing thread", async () => {
      await storage.save("thread-1", new Uint8Array([1]), createMetadata());
      expect(await storage.exists("thread-1")).toBe(true);
    });

    it("should return false for non-existent thread", async () => {
      expect(await storage.exists("non-existent")).toBe(false);
    });
  });

  describe("getMetadata", () => {
    it("should return metadata for existing thread", async () => {
      const metadata = createMetadata({ threadId: "thread-1" });
      await storage.save("thread-1", new Uint8Array([1]), metadata);

      const loaded = await storage.getMetadata("thread-1");
      expect(loaded).not.toBeNull();
      expect(loaded?.threadId).toBe("thread-1");
      expect(loaded?.workflowId).toBe("workflow-1");
      expect(loaded?.status).toBe("RUNNING");
    });

    it("should return null for non-existent thread", async () => {
      const loaded = await storage.getMetadata("non-existent");
      expect(loaded).toBeNull();
    });
  });

  describe("updateThreadStatus", () => {
    it("should update thread status", async () => {
      await storage.save(
        "thread-1",
        new Uint8Array([1, 2, 3]),
        createMetadata({ status: "RUNNING" }),
      );

      await storage.updateThreadStatus("thread-1", "COMPLETED");

      const metadata = await storage.getMetadata("thread-1");
      expect(metadata?.status).toBe("COMPLETED");
    });

    it("should preserve data when updating status", async () => {
      const data = new Uint8Array([1, 2, 3, 4, 5]);
      await storage.save("thread-1", data, createMetadata({ status: "RUNNING" }));

      await storage.updateThreadStatus("thread-1", "COMPLETED");

      const loaded = await storage.load("thread-1");
      expect(loaded).toEqual(data);
    });
  });

  describe("threadType filtering", () => {
    beforeEach(async () => {
      await storage.save(
        "thread-1",
        new Uint8Array([1]),
        createMetadata({
          threadType: "MAIN",
        }),
      );
      await storage.save(
        "thread-2",
        new Uint8Array([2]),
        createMetadata({
          threadType: "FORK_JOIN",
        }),
      );
    });

    it("should filter by threadType", async () => {
      const ids = await storage.list({ threadType: "MAIN" });
      expect(ids).toHaveLength(1);
      expect(ids).toContain("thread-1");
    });
  });

  describe("parentThreadId filtering", () => {
    beforeEach(async () => {
      await storage.save(
        "thread-1",
        new Uint8Array([1]),
        createMetadata({
          threadId: "thread-1",
        }),
      );
      await storage.save(
        "thread-2",
        new Uint8Array([2]),
        createMetadata({
          threadId: "thread-2",
          parentThreadId: "thread-1",
        }),
      );
    });

    it("should filter by parentThreadId", async () => {
      const ids = await storage.list({ parentThreadId: "thread-1" });
      expect(ids).toHaveLength(1);
      expect(ids).toContain("thread-2");
    });
  });

  describe("tags filtering", () => {
    beforeEach(async () => {
      await storage.save(
        "thread-1",
        new Uint8Array([1]),
        createMetadata({
          tags: ["important", "production"],
        }),
      );
      await storage.save(
        "thread-2",
        new Uint8Array([2]),
        createMetadata({
          tags: ["test"],
        }),
      );
    });

    it("should filter by tags", async () => {
      const ids = await storage.list({ tags: ["important"] });
      expect(ids).toHaveLength(1);
      expect(ids).toContain("thread-1");
    });
  });

  describe("clear", () => {
    it("should clear all threads", async () => {
      await storage.save("thread-1", new Uint8Array([1]), createMetadata());
      await storage.save("thread-2", new Uint8Array([2]), createMetadata());

      await storage.clear();

      const ids = await storage.list();
      expect(ids).toHaveLength(0);
    });
  });
});
