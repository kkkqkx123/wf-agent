/**
 * JsonToolStorage Tests
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as fs from "fs/promises";
import * as path from "path";
import * as os from "os";
import { JsonToolStorage } from "../json-tool-storage.js";
import type { ToolStorageMetadata } from "@wf-agent/types";

describe("JsonToolStorage", () => {
  let storage: JsonToolStorage;
  let tempDir: string;

  const createMetadata = (
    overrides?: Partial<ToolStorageMetadata>,
  ): ToolStorageMetadata => ({
    toolId: "tool-1",
    type: "http",
    description: "Test tool description",
    category: "api",
    ...overrides,
  });

  beforeEach(async () => {
    tempDir = await fs.mkdtemp(path.join(os.tmpdir(), "json-tool-test-"));
    storage = new JsonToolStorage({ baseDir: tempDir });
    await storage.initialize();
  });

  afterEach(async () => {
    await storage.close();
    await fs.rm(tempDir, { recursive: true, force: true });
  });

  describe("initialize", () => {
    it("should create metadata and data directories", async () => {
      const metadataDir = path.join(tempDir, "metadata", "tool");
      const dataDir = path.join(tempDir, "data", "tool");

      const metadataStat = await fs.stat(metadataDir);
      const dataStat = await fs.stat(dataDir);

      expect(metadataStat.isDirectory()).toBe(true);
      expect(dataStat.isDirectory()).toBe(true);
    });
  });

  describe("save / load", () => {
    it("should save and load tool", async () => {
      const toolId = "tool-1";
      const data = new Uint8Array([1, 2, 3, 4, 5]);
      const metadata = createMetadata();

      await storage.save(toolId, data, metadata);

      const loaded = await storage.load(toolId);
      expect(loaded).not.toBeNull();
      expect(loaded).toEqual(data);
    });

    it("should return null for non-existent tool", async () => {
      const loaded = await storage.load("non-existent");
      expect(loaded).toBeNull();
    });

    it("should persist metadata and data to separate files", async () => {
      const toolId = "tool-1";
      const data = new Uint8Array([1, 2, 3]);

      await storage.save(toolId, data, createMetadata());

      const metadataFiles = await fs.readdir(path.join(tempDir, "metadata", "tool"));
      expect(metadataFiles).toContain("tool-1.json");

      const dataFiles = await fs.readdir(path.join(tempDir, "data", "tool"));
      expect(dataFiles).toContain("tool-1.bin");
    });
  });

  describe("delete", () => {
    it("should delete tool", async () => {
      const toolId = "tool-1";
      await storage.save(toolId, new Uint8Array([1]), createMetadata());

      await storage.delete(toolId);

      const loaded = await storage.load(toolId);
      expect(loaded).toBeNull();
    });
  });

  describe("list", () => {
    beforeEach(async () => {
      await storage.save(
        "tool-1",
        new Uint8Array([1]),
        createMetadata({
          toolId: "tool-1",
          type: "http",
          category: "api",
        }),
      );
      await storage.save(
        "tool-2",
        new Uint8Array([2]),
        createMetadata({
          toolId: "tool-2",
          type: "database",
          category: "db",
        }),
      );
      await storage.save(
        "tool-3",
        new Uint8Array([3]),
        createMetadata({
          toolId: "tool-3",
          type: "http",
          category: "api",
        }),
      );
    });

    it("should list all tools", async () => {
      const ids = await storage.list();
      expect(ids).toHaveLength(3);
    });

    it("should return tool IDs in insertion order", async () => {
      const ids = await storage.list();
      expect(ids).toContain("tool-1");
      expect(ids).toContain("tool-2");
      expect(ids).toContain("tool-3");
    });

    it("should filter by toolId", async () => {
      const ids = await storage.list({ toolId: "tool-1" });
      expect(ids).toHaveLength(1);
      expect(ids).toContain("tool-1");
    });

    it("should filter by type", async () => {
      const ids = await storage.list({ type: "http" });
      expect(ids).toHaveLength(2);
      expect(ids).toContain("tool-1");
      expect(ids).toContain("tool-3");
    });

    it("should filter by multiple types", async () => {
      const ids = await storage.list({ type: ["http", "database"] });
      expect(ids).toHaveLength(3);
    });

    it("should filter by category", async () => {
      const ids = await storage.list({ category: "api" });
      expect(ids).toHaveLength(2);
      expect(ids).toContain("tool-1");
      expect(ids).toContain("tool-3");
    });

    it("should sort by type ascending", async () => {
      const ids = await storage.list({ sortBy: "type", sortOrder: "asc" });
      expect(ids).toEqual(["tool-2", "tool-1", "tool-3"]);
    });

    it("should sort by toolId descending", async () => {
      const ids = await storage.list({ sortBy: "toolId", sortOrder: "desc" });
      expect(ids).toEqual(["tool-3", "tool-2", "tool-1"]);
    });

    it("should support pagination", async () => {
      const ids = await storage.list({ offset: 1, limit: 1 });
      expect(ids).toHaveLength(1);
    });

    it("should combine filters", async () => {
      const ids = await storage.list({ type: "http", sortBy: "toolId", sortOrder: "asc" });
      expect(ids).toEqual(["tool-1", "tool-3"]);
    });
  });

  describe("exists", () => {
    it("should return true for existing tool", async () => {
      await storage.save("tool-1", new Uint8Array([1]), createMetadata());
      expect(await storage.exists("tool-1")).toBe(true);
    });

    it("should return false for non-existent tool", async () => {
      expect(await storage.exists("non-existent")).toBe(false);
    });
  });

  describe("getMetadata", () => {
    it("should return metadata for existing tool", async () => {
      const metadata = createMetadata({ toolId: "tool-1", type: "http" });
      await storage.save("tool-1", new Uint8Array([1]), metadata);

      const loaded = await storage.getMetadata("tool-1");
      expect(loaded).toEqual(metadata);
    });

    it("should return null for non-existent tool", async () => {
      const loaded = await storage.getMetadata("non-existent");
      expect(loaded).toBeNull();
    });
  });

  describe("clear", () => {
    it("should clear all tools", async () => {
      await storage.save("tool-1", new Uint8Array([1]), createMetadata());
      await storage.save("tool-2", new Uint8Array([2]), createMetadata());

      await storage.clear();

      const ids = await storage.list();
      expect(ids).toHaveLength(0);
    });
  });

  describe("tags filtering", () => {
    beforeEach(async () => {
      await storage.save(
        "tool-1",
        new Uint8Array([1]),
        createMetadata({
          tags: ["builtin", "official"],
        }),
      );
      await storage.save(
        "tool-2",
        new Uint8Array([2]),
        createMetadata({
          tags: ["custom"],
        }),
      );
    });

    it("should store tools with tags", async () => {
      const metadata1 = await storage.getMetadata("tool-1");
      expect(metadata1?.tags).toEqual(["builtin", "official"]);

      const metadata2 = await storage.getMetadata("tool-2");
      expect(metadata2?.tags).toEqual(["custom"]);
    });

    it("should filter by tags", async () => {
      const ids = await storage.list({ tags: ["builtin"] });
      expect(ids).toHaveLength(1);
      expect(ids).toContain("tool-1");
    });
  });
});
