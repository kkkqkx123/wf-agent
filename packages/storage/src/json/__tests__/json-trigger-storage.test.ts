/**
 * JsonTriggerStorage Tests
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as fs from "fs/promises";
import * as path from "path";
import * as os from "os";
import { JsonTriggerStorage } from "../json-trigger-storage.js";
import type { TriggerStorageMetadata } from "@wf-agent/types";

describe("JsonTriggerStorage", () => {
  let storage: JsonTriggerStorage;
  let tempDir: string;

  const createMetadata = (
    overrides?: Partial<TriggerStorageMetadata>,
  ): TriggerStorageMetadata => ({
    name: "Test Trigger",
    description: "Test trigger description",
    category: "test",
    enabled: true,
    createdAt: Date.now(),
    updatedAt: Date.now(),
    ...overrides,
  });

  beforeEach(async () => {
    tempDir = await fs.mkdtemp(path.join(os.tmpdir(), "json-trigger-test-"));
    storage = new JsonTriggerStorage({ baseDir: tempDir });
    await storage.initialize();
  });

  afterEach(async () => {
    await storage.close();
    await fs.rm(tempDir, { recursive: true, force: true });
  });

  describe("initialize", () => {
    it("should create metadata and data directories", async () => {
      const metadataDir = path.join(tempDir, "metadata", "trigger");
      const dataDir = path.join(tempDir, "data", "trigger");

      const metadataStat = await fs.stat(metadataDir);
      const dataStat = await fs.stat(dataDir);

      expect(metadataStat.isDirectory()).toBe(true);
      expect(dataStat.isDirectory()).toBe(true);
    });
  });

  describe("save / load", () => {
    it("should save and load trigger", async () => {
      const triggerId = "trigger-1";
      const data = new Uint8Array([1, 2, 3, 4, 5]);
      const metadata = createMetadata();

      await storage.save(triggerId, data, metadata);

      const loaded = await storage.load(triggerId);
      expect(loaded).not.toBeNull();
      expect(loaded).toEqual(data);
    });

    it("should return null for non-existent trigger", async () => {
      const loaded = await storage.load("non-existent");
      expect(loaded).toBeNull();
    });

    it("should persist metadata and data to separate files", async () => {
      const triggerId = "trigger-1";
      const data = new Uint8Array([1, 2, 3]);

      await storage.save(triggerId, data, createMetadata());

      const metadataFiles = await fs.readdir(path.join(tempDir, "metadata", "trigger"));
      expect(metadataFiles).toContain("trigger-1.json");

      const dataFiles = await fs.readdir(path.join(tempDir, "data", "trigger"));
      expect(dataFiles).toContain("trigger-1.bin");
    });
  });

  describe("delete", () => {
    it("should delete trigger", async () => {
      const triggerId = "trigger-1";
      await storage.save(triggerId, new Uint8Array([1]), createMetadata());

      await storage.delete(triggerId);

      const loaded = await storage.load(triggerId);
      expect(loaded).toBeNull();
    });
  });

  describe("list", () => {
    beforeEach(async () => {
      await storage.save(
        "trigger-1",
        new Uint8Array([1]),
        createMetadata({
          name: "Trigger One",
          category: "http",
          enabled: true,
          createdAt: 1000,
          updatedAt: 1000,
        }),
      );
      await storage.save(
        "trigger-2",
        new Uint8Array([2]),
        createMetadata({
          name: "Trigger Two",
          category: "webhook",
          enabled: false,
          createdAt: 2000,
          updatedAt: 2000,
        }),
      );
      await storage.save(
        "trigger-3",
        new Uint8Array([3]),
        createMetadata({
          name: "Another Trigger",
          category: "http",
          enabled: true,
          createdAt: 3000,
          updatedAt: 3000,
        }),
      );
    });

    it("should list all triggers", async () => {
      const ids = await storage.list();
      expect(ids).toHaveLength(3);
    });

    it("should return trigger IDs in insertion order", async () => {
      const ids = await storage.list();
      expect(ids).toContain("trigger-1");
      expect(ids).toContain("trigger-2");
      expect(ids).toContain("trigger-3");
    });

    it("should filter by nameContains", async () => {
      const ids = await storage.list({ nameContains: "Another" });
      expect(ids).toHaveLength(1);
      expect(ids).toContain("trigger-3");
    });

    it("should filter by single category", async () => {
      const ids = await storage.list({ category: "http" });
      expect(ids).toHaveLength(2);
      expect(ids).toContain("trigger-1");
      expect(ids).toContain("trigger-3");
    });

    it("should filter by multiple categories", async () => {
      const ids = await storage.list({ category: ["http", "webhook"] });
      expect(ids).toHaveLength(3);
    });

    it("should filter by enabled status", async () => {
      const ids = await storage.list({ enabled: true });
      expect(ids).toHaveLength(2);
      expect(ids).toContain("trigger-1");
      expect(ids).toContain("trigger-3");
    });

    it("should filter by createdAfter", async () => {
      const ids = await storage.list({ createdAfter: 2500 });
      expect(ids).toHaveLength(1);
      expect(ids).toContain("trigger-3");
    });

    it("should filter by createdBefore", async () => {
      const ids = await storage.list({ createdBefore: 1500 });
      expect(ids).toHaveLength(1);
      expect(ids).toContain("trigger-1");
    });

    it("should filter by time range", async () => {
      const ids = await storage.list({ createdAfter: 1000, createdBefore: 2500 });
      expect(ids).toHaveLength(2);
      expect(ids).toContain("trigger-1");
      expect(ids).toContain("trigger-2");
    });

    it("should sort by createdAt ascending", async () => {
      const ids = await storage.list({ sortBy: "createdAt", sortOrder: "asc" });
      expect(ids).toEqual(["trigger-1", "trigger-2", "trigger-3"]);
    });

    it("should sort by updatedAt descending", async () => {
      const ids = await storage.list({ sortBy: "updatedAt", sortOrder: "desc" });
      expect(ids).toEqual(["trigger-3", "trigger-2", "trigger-1"]);
    });

    it("should support pagination with offset and limit", async () => {
      const ids = await storage.list({ offset: 1, limit: 1 });
      expect(ids).toHaveLength(1);
    });

    it("should combine multiple filters", async () => {
      const ids = await storage.list({ category: "http", enabled: true, sortBy: "name", sortOrder: "asc" });
      expect(ids).toEqual(["trigger-3", "trigger-1"]);
    });
  });

  describe("exists", () => {
    it("should return true for existing trigger", async () => {
      await storage.save("trigger-1", new Uint8Array([1]), createMetadata());
      expect(await storage.exists("trigger-1")).toBe(true);
    });

    it("should return false for non-existent trigger", async () => {
      expect(await storage.exists("non-existent")).toBe(false);
    });
  });

  describe("getMetadata", () => {
    it("should return metadata for existing trigger", async () => {
      const metadata = createMetadata({ name: "My Trigger" });
      await storage.save("trigger-1", new Uint8Array([1]), metadata);

      const loaded = await storage.getMetadata("trigger-1");
      expect(loaded).toEqual(metadata);
    });

    it("should return null for non-existent trigger", async () => {
      const loaded = await storage.getMetadata("non-existent");
      expect(loaded).toBeNull();
    });
  });

  describe("clear", () => {
    it("should clear all triggers", async () => {
      await storage.save("trigger-1", new Uint8Array([1]), createMetadata());
      await storage.save("trigger-2", new Uint8Array([2]), createMetadata());

      await storage.clear();

      const ids = await storage.list();
      expect(ids).toHaveLength(0);
    });
  });

  describe("tags filtering", () => {
    beforeEach(async () => {
      await storage.save(
        "trigger-1",
        new Uint8Array([1]),
        createMetadata({
          tags: ["important", "production"],
        }),
      );
      await storage.save(
        "trigger-2",
        new Uint8Array([2]),
        createMetadata({
          tags: ["test"],
        }),
      );
      await storage.save(
        "trigger-3",
        new Uint8Array([3]),
        createMetadata({
          tags: ["important"],
        }),
      );
    });

    it("should store triggers with tags", async () => {
      const metadata1 = await storage.getMetadata("trigger-1");
      expect(metadata1?.tags).toEqual(["important", "production"]);

      const metadata2 = await storage.getMetadata("trigger-2");
      expect(metadata2?.tags).toEqual(["test"]);
    });

    it("should filter by tags (any match)", async () => {
      const ids = await storage.list({ tags: ["important"] });
      expect(ids).toHaveLength(2);
      expect(ids).toContain("trigger-1");
      expect(ids).toContain("trigger-3");
    });

    it("should filter by tags with no match", async () => {
      const ids = await storage.list({ tags: ["nonexistent"] });
      expect(ids).toHaveLength(0);
    });
  });
});
