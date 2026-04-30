/**
 * SqliteCheckpointStorage 测试
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as fs from "fs/promises";
import * as path from "path";
import * as os from "os";
import { SqliteCheckpointStorage } from "../sqlite-checkpoint-storage.js";
import type { CheckpointStorageMetadata } from "@wf-agent/types";

describe("SqliteCheckpointStorage", () => {
  let storage: SqliteCheckpointStorage;
  let tempDir: string;
  let dbPath: string;

  const createMetadata = (
    overrides?: Partial<CheckpointStorageMetadata>,
  ): CheckpointStorageMetadata => ({
    executionId: "execution-1",
    workflowId: "workflow-1",
    timestamp: Date.now(),
    ...overrides,
  });

  beforeEach(async () => {
    tempDir = await fs.mkdtemp(path.join(os.tmpdir(), "sqlite-test-"));
    dbPath = path.join(tempDir, "test.db");
    storage = new SqliteCheckpointStorage({ dbPath });
    await storage.initialize();
  });

  afterEach(async () => {
    await storage.close();
    await fs.rm(tempDir, { recursive: true, force: true });
  });

  describe("initialize", () => {
    it("should create database file", async () => {
      const stat = await fs.stat(dbPath);
      expect(stat.isFile()).toBe(true);
    });
  });

  describe("save / load", () => {
    it("should save and load checkpoint", async () => {
      const checkpointId = "checkpoint-1";
      const data = new Uint8Array([1, 2, 3, 4, 5]);
      const metadata = createMetadata();

      await storage.save(checkpointId, data, metadata);

      const loaded = await storage.load(checkpointId);
      expect(loaded).not.toBeNull();
      expect(loaded).toEqual(data);
    });

    it("should return null for non-existent checkpoint", async () => {
      const loaded = await storage.load("non-existent");
      expect(loaded).toBeNull();
    });

    it("should overwrite existing checkpoint", async () => {
      const checkpointId = "checkpoint-1";
      const data1 = new Uint8Array([1, 2, 3]);
      const data2 = new Uint8Array([4, 5, 6]);

      await storage.save(checkpointId, data1, createMetadata());
      await storage.save(checkpointId, data2, createMetadata());

      const loaded = await storage.load(checkpointId);
      expect(loaded).toEqual(data2);
    });
  });

  describe("delete", () => {
    it("should delete checkpoint", async () => {
      const checkpointId = "checkpoint-1";
      await storage.save(checkpointId, new Uint8Array([1]), createMetadata());

      await storage.delete(checkpointId);

      const loaded = await storage.load(checkpointId);
      expect(loaded).toBeNull();
    });
  });

  describe("list", () => {
    beforeEach(async () => {
      await storage.save(
        "cp-1",
        new Uint8Array([1]),
        createMetadata({ executionId: "execution-1", timestamp: 1000 }),
      );
      await storage.save(
        "cp-2",
        new Uint8Array([2]),
        createMetadata({ executionId: "execution-1", timestamp: 2000 }),
      );
      await storage.save(
        "cp-3",
        new Uint8Array([3]),
        createMetadata({ executionId: "execution-2", timestamp: 3000 }),
      );
    });

    it("should list all checkpoints", async () => {
      const ids = await storage.list();
      expect(ids).toHaveLength(3);
    });

    it("should filter by executionId", async () => {
      const ids = await storage.list({ executionId: "execution-1" });
      expect(ids).toHaveLength(2);
    });

    it("should filter by workflowId", async () => {
      const ids = await storage.list({ workflowId: "workflow-1" });
      expect(ids).toHaveLength(3);
    });

    it("should sort by timestamp descending", async () => {
      const ids = await storage.list();
      expect(ids).toEqual(["cp-3", "cp-2", "cp-1"]);
    });

    it("should support pagination", async () => {
      const ids = await storage.list({ limit: 2 });
      expect(ids).toHaveLength(2);
      expect(ids).toEqual(["cp-3", "cp-2"]);

      const ids2 = await storage.list({ limit: 2, offset: 2 });
      expect(ids2).toHaveLength(1);
      expect(ids2).toEqual(["cp-1"]);
    });
  });

  describe("exists", () => {
    it("should return true for existing checkpoint", async () => {
      await storage.save("cp-1", new Uint8Array([1]), createMetadata());
      expect(await storage.exists("cp-1")).toBe(true);
    });

    it("should return false for non-existent checkpoint", async () => {
      expect(await storage.exists("non-existent")).toBe(false);
    });
  });

  describe("clear", () => {
    it("should clear all checkpoints", async () => {
      await storage.save("cp-1", new Uint8Array([1]), createMetadata());
      await storage.save("cp-2", new Uint8Array([2]), createMetadata());

      await storage.clear();

      const ids = await storage.list();
      expect(ids).toHaveLength(0);
    });
  });
});
