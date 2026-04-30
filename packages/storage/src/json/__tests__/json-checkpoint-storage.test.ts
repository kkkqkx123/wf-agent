/**
 * JsonCheckpointStorage Tests
 * Tests for metadata-data separation
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as fs from "fs/promises";
import * as path from "path";
import * as os from "os";
import { JsonCheckpointStorage } from "../json-checkpoint-storage.js";
import type { CheckpointStorageMetadata } from "@wf-agent/types";

describe("JsonCheckpointStorage", () => {
  let storage: JsonCheckpointStorage;
  let tempDir: string;

  const createMetadata = (
    overrides?: Partial<CheckpointStorageMetadata>,
  ): CheckpointStorageMetadata => ({
    executionId: "execution-1",
    workflowId: "workflow-1",
    timestamp: Date.now(),
    ...overrides,
  });

  beforeEach(async () => {
    tempDir = await fs.mkdtemp(path.join(os.tmpdir(), "json-checkpoint-test-"));
    storage = new JsonCheckpointStorage({ baseDir: tempDir });
    await storage.initialize();
  });

  afterEach(async () => {
    await storage.close();
    await fs.rm(tempDir, { recursive: true, force: true });
  });

  describe("initialize", () => {
    it("should create base directory and subdirectories", async () => {
      const newDir = path.join(os.tmpdir(), "json-checkpoint-test-new-" + Date.now());
      const newStorage = new JsonCheckpointStorage({ baseDir: newDir });

      await newStorage.initialize();

      const stat = await fs.stat(newDir);
      expect(stat.isDirectory()).toBe(true);

      // Check metadata and data subdirectories are created
      const metadataStat = await fs.stat(path.join(newDir, "metadata", "checkpoint"));
      const dataStat = await fs.stat(path.join(newDir, "data", "checkpoint"));
      expect(metadataStat.isDirectory()).toBe(true);
      expect(dataStat.isDirectory()).toBe(true);

      await newStorage.close();
      await fs.rm(newDir, { recursive: true, force: true });
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

    it("should persist metadata and data to separate files", async () => {
      const checkpointId = "checkpoint-1";
      const data = new Uint8Array([1, 2, 3]);

      await storage.save(checkpointId, data, createMetadata());

      // Check metadata file exists
      const metadataFiles = await fs.readdir(path.join(tempDir, "metadata", "checkpoint"));
      expect(metadataFiles).toContain("checkpoint-1.json");

      // Check data file exists
      const dataFiles = await fs.readdir(path.join(tempDir, "data", "checkpoint"));
      expect(dataFiles).toContain("checkpoint-1.bin");
    });
  });

  describe("delete", () => {
    it("should delete checkpoint metadata and data", async () => {
      const checkpointId = "checkpoint-1";
      await storage.save(checkpointId, new Uint8Array([1]), createMetadata());

      await storage.delete(checkpointId);

      const loaded = await storage.load(checkpointId);
      expect(loaded).toBeNull();

      // Verify both files are deleted
      const metadataFiles = await fs.readdir(path.join(tempDir, "metadata", "checkpoint"));
      const dataFiles = await fs.readdir(path.join(tempDir, "data", "checkpoint"));
      expect(metadataFiles).not.toContain("checkpoint-1.json");
      expect(dataFiles).not.toContain("checkpoint-1.bin");
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

    it("should sort by timestamp descending", async () => {
      const ids = await storage.list();
      expect(ids).toEqual(["cp-3", "cp-2", "cp-1"]);
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
