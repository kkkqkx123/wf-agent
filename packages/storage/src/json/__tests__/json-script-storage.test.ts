/**
 * JsonScriptStorage Tests
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as fs from "fs/promises";
import * as path from "path";
import * as os from "os";
import { JsonScriptStorage } from "../json-script-storage.js";
import type { ScriptStorageMetadata } from "@wf-agent/types";

describe("JsonScriptStorage", () => {
  let storage: JsonScriptStorage;
  let tempDir: string;

  const createMetadata = (
    overrides?: Partial<ScriptStorageMetadata>,
  ): ScriptStorageMetadata => ({
    name: "Test Script",
    description: "Test script description",
    category: "test",
    enabled: true,
    createdAt: Date.now(),
    updatedAt: Date.now(),
    ...overrides,
  });

  beforeEach(async () => {
    tempDir = await fs.mkdtemp(path.join(os.tmpdir(), "json-script-test-"));
    storage = new JsonScriptStorage({ baseDir: tempDir });
    await storage.initialize();
  });

  afterEach(async () => {
    await storage.close();
    await fs.rm(tempDir, { recursive: true, force: true });
  });

  describe("initialize", () => {
    it("should create metadata and data directories", async () => {
      const metadataDir = path.join(tempDir, "metadata", "script");
      const dataDir = path.join(tempDir, "data", "script");

      const metadataStat = await fs.stat(metadataDir);
      const dataStat = await fs.stat(dataDir);

      expect(metadataStat.isDirectory()).toBe(true);
      expect(dataStat.isDirectory()).toBe(true);
    });
  });

  describe("save / load", () => {
    it("should save and load script", async () => {
      const scriptId = "script-1";
      const data = new Uint8Array([1, 2, 3, 4, 5]);
      const metadata = createMetadata();

      await storage.save(scriptId, data, metadata);

      const loaded = await storage.load(scriptId);
      expect(loaded).not.toBeNull();
      expect(loaded).toEqual(data);
    });

    it("should return null for non-existent script", async () => {
      const loaded = await storage.load("non-existent");
      expect(loaded).toBeNull();
    });

    it("should persist metadata and data to separate files", async () => {
      const scriptId = "script-1";
      const data = new Uint8Array([1, 2, 3]);

      await storage.save(scriptId, data, createMetadata());

      const metadataFiles = await fs.readdir(path.join(tempDir, "metadata", "script"));
      expect(metadataFiles).toContain("script-1.json");

      const dataFiles = await fs.readdir(path.join(tempDir, "data", "script"));
      expect(dataFiles).toContain("script-1.bin");
    });
  });

  describe("delete", () => {
    it("should delete script", async () => {
      const scriptId = "script-1";
      await storage.save(scriptId, new Uint8Array([1]), createMetadata());

      await storage.delete(scriptId);

      const loaded = await storage.load(scriptId);
      expect(loaded).toBeNull();
    });
  });

  describe("list", () => {
    beforeEach(async () => {
      await storage.save(
        "script-1",
        new Uint8Array([1]),
        createMetadata({
          name: "Script One",
          category: "util",
          enabled: true,
          createdAt: 1000,
          updatedAt: 1000,
        }),
      );
      await storage.save(
        "script-2",
        new Uint8Array([2]),
        createMetadata({
          name: "Script Two",
          category: "transform",
          enabled: false,
          createdAt: 2000,
          updatedAt: 2000,
        }),
      );
      await storage.save(
        "script-3",
        new Uint8Array([3]),
        createMetadata({
          name: "Another Script",
          category: "util",
          enabled: true,
          createdAt: 3000,
          updatedAt: 3000,
        }),
      );
    });

    it("should list all scripts", async () => {
      const ids = await storage.list();
      expect(ids).toHaveLength(3);
    });

    it("should return script IDs in insertion order", async () => {
      const ids = await storage.list();
      expect(ids).toContain("script-1");
      expect(ids).toContain("script-2");
      expect(ids).toContain("script-3");
    });

    it("should filter by nameContains", async () => {
      const ids = await storage.list({ nameContains: "Another" });
      expect(ids).toHaveLength(1);
      expect(ids).toContain("script-3");
    });

    it("should filter by single category", async () => {
      const ids = await storage.list({ category: "util" });
      expect(ids).toHaveLength(2);
      expect(ids).toContain("script-1");
      expect(ids).toContain("script-3");
    });

    it("should filter by enabled status", async () => {
      const ids = await storage.list({ enabled: true });
      expect(ids).toHaveLength(2);
      expect(ids).toContain("script-1");
      expect(ids).toContain("script-3");
    });

    it("should filter by createdAfter", async () => {
      const ids = await storage.list({ createdAfter: 2500 });
      expect(ids).toHaveLength(1);
      expect(ids).toContain("script-3");
    });

    it("should filter by createdBefore", async () => {
      const ids = await storage.list({ createdBefore: 1500 });
      expect(ids).toHaveLength(1);
      expect(ids).toContain("script-1");
    });

    it("should sort by createdAt ascending", async () => {
      const ids = await storage.list({ sortBy: "createdAt", sortOrder: "asc" });
      expect(ids).toEqual(["script-1", "script-2", "script-3"]);
    });

    it("should sort by name descending", async () => {
      const ids = await storage.list({ sortBy: "name", sortOrder: "desc" });
      expect(ids).toEqual(["script-2", "script-1", "script-3"]);
    });

    it("should support pagination", async () => {
      const ids = await storage.list({ offset: 1, limit: 1 });
      expect(ids).toHaveLength(1);
    });

    it("should combine multiple filters", async () => {
      const ids = await storage.list({ category: "util", enabled: true, sortBy: "name", sortOrder: "asc" });
      expect(ids).toEqual(["script-3", "script-1"]);
    });
  });

  describe("exists", () => {
    it("should return true for existing script", async () => {
      await storage.save("script-1", new Uint8Array([1]), createMetadata());
      expect(await storage.exists("script-1")).toBe(true);
    });

    it("should return false for non-existent script", async () => {
      expect(await storage.exists("non-existent")).toBe(false);
    });
  });

  describe("getMetadata", () => {
    it("should return metadata for existing script", async () => {
      const metadata = createMetadata({ name: "My Script" });
      await storage.save("script-1", new Uint8Array([1]), metadata);

      const loaded = await storage.getMetadata("script-1");
      expect(loaded).toEqual(metadata);
    });

    it("should return null for non-existent script", async () => {
      const loaded = await storage.getMetadata("non-existent");
      expect(loaded).toBeNull();
    });
  });

  describe("clear", () => {
    it("should clear all scripts", async () => {
      await storage.save("script-1", new Uint8Array([1]), createMetadata());
      await storage.save("script-2", new Uint8Array([2]), createMetadata());

      await storage.clear();

      const ids = await storage.list();
      expect(ids).toHaveLength(0);
    });
  });

  describe("tags filtering", () => {
    beforeEach(async () => {
      await storage.save(
        "script-1",
        new Uint8Array([1]),
        createMetadata({
          tags: ["critical", "system"],
        }),
      );
      await storage.save(
        "script-2",
        new Uint8Array([2]),
        createMetadata({
          tags: ["debug"],
        }),
      );
    });

    it("should store scripts with tags", async () => {
      const metadata1 = await storage.getMetadata("script-1");
      expect(metadata1?.tags).toEqual(["critical", "system"]);

      const metadata2 = await storage.getMetadata("script-2");
      expect(metadata2?.tags).toEqual(["debug"]);
    });

    it("should filter by tags", async () => {
      const ids = await storage.list({ tags: ["critical"] });
      expect(ids).toHaveLength(1);
      expect(ids).toContain("script-1");
    });
  });
});
