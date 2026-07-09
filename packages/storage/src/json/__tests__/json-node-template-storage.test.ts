/**
 * JsonNodeTemplateStorage Tests
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as fs from "fs/promises";
import * as path from "path";
import * as os from "os";
import { JsonNodeTemplateStorage } from "../json-node-template-storage.js";
import type { NodeTemplateStorageMetadata } from "@wf-agent/types";

describe("JsonNodeTemplateStorage", () => {
  let storage: JsonNodeTemplateStorage;
  let tempDir: string;

  const createMetadata = (
    overrides?: Partial<NodeTemplateStorageMetadata>,
  ): NodeTemplateStorageMetadata => ({
    name: "Test Node Template",
    type: "action",
    description: "Test node template description",
    category: "builtin",
    createdAt: Date.now(),
    updatedAt: Date.now(),
    ...overrides,
  });

  beforeEach(async () => {
    tempDir = await fs.mkdtemp(path.join(os.tmpdir(), "json-node-template-test-"));
    storage = new JsonNodeTemplateStorage({ baseDir: tempDir });
    await storage.initialize();
  });

  afterEach(async () => {
    await storage.close();
    await fs.rm(tempDir, { recursive: true, force: true });
  });

  describe("initialize", () => {
    it("should create metadata and data directories", async () => {
      const metadataDir = path.join(tempDir, "metadata", "nodeTemplate");
      const dataDir = path.join(tempDir, "data", "nodeTemplate");

      const metadataStat = await fs.stat(metadataDir);
      const dataStat = await fs.stat(dataDir);

      expect(metadataStat.isDirectory()).toBe(true);
      expect(dataStat.isDirectory()).toBe(true);
    });
  });

  describe("save / load", () => {
    it("should save and load node template", async () => {
      const templateId = "node-template-1";
      const data = new Uint8Array([1, 2, 3, 4, 5]);
      const metadata = createMetadata();

      await storage.save(templateId, data, metadata);

      const loaded = await storage.load(templateId);
      expect(loaded).not.toBeNull();
      expect(loaded).toEqual(data);
    });

    it("should return null for non-existent node template", async () => {
      const loaded = await storage.load("non-existent");
      expect(loaded).toBeNull();
    });

    it("should persist metadata and data to separate files", async () => {
      const templateId = "node-template-1";
      const data = new Uint8Array([1, 2, 3]);

      await storage.save(templateId, data, createMetadata());

      const metadataFiles = await fs.readdir(path.join(tempDir, "metadata", "nodeTemplate"));
      expect(metadataFiles).toContain("node-template-1.json");

      const dataFiles = await fs.readdir(path.join(tempDir, "data", "nodeTemplate"));
      expect(dataFiles).toContain("node-template-1.bin");
    });
  });

  describe("delete", () => {
    it("should delete node template", async () => {
      const templateId = "node-template-1";
      await storage.save(templateId, new Uint8Array([1]), createMetadata());

      await storage.delete(templateId);

      const loaded = await storage.load(templateId);
      expect(loaded).toBeNull();
    });
  });

  describe("list", () => {
    beforeEach(async () => {
      await storage.save(
        "node-template-1",
        new Uint8Array([1]),
        createMetadata({
          name: "Action Node",
          type: "action",
          category: "builtin",
          createdAt: 1000,
          updatedAt: 1000,
        }),
      );
      await storage.save(
        "node-template-2",
        new Uint8Array([2]),
        createMetadata({
          name: "Condition Node",
          type: "condition",
          category: "builtin",
          createdAt: 2000,
          updatedAt: 2000,
        }),
      );
      await storage.save(
        "node-template-3",
        new Uint8Array([3]),
        createMetadata({
          name: "Loop Action",
          type: "action",
          category: "custom",
          createdAt: 3000,
          updatedAt: 3000,
        }),
      );
    });

    it("should list all node templates", async () => {
      const ids = await storage.list();
      expect(ids).toHaveLength(3);
    });

    it("should return node template IDs in insertion order", async () => {
      const ids = await storage.list();
      expect(ids).toContain("node-template-1");
      expect(ids).toContain("node-template-2");
      expect(ids).toContain("node-template-3");
    });

    it("should filter by nameContains", async () => {
      const ids = await storage.list({ nameContains: "Action" });
      expect(ids).toHaveLength(2);
      expect(ids).toContain("node-template-1");
      expect(ids).toContain("node-template-3");
    });

    it("should filter by type", async () => {
      const ids = await storage.list({ type: "action" });
      expect(ids).toHaveLength(2);
      expect(ids).toContain("node-template-1");
      expect(ids).toContain("node-template-3");
    });

    it("should filter by multiple types", async () => {
      const ids = await storage.list({ type: ["action", "condition"] });
      expect(ids).toHaveLength(3);
    });

    it("should filter by category", async () => {
      const ids = await storage.list({ category: "builtin" });
      expect(ids).toHaveLength(2);
      expect(ids).toContain("node-template-1");
      expect(ids).toContain("node-template-2");
    });

    it("should filter by createdAfter", async () => {
      const ids = await storage.list({ createdAfter: 2500 });
      expect(ids).toHaveLength(1);
      expect(ids).toContain("node-template-3");
    });

    it("should sort by type ascending", async () => {
      const ids = await storage.list({ sortBy: "type", sortOrder: "asc" });
      expect(ids).toEqual(["node-template-1", "node-template-3", "node-template-2"]);
    });

    it("should sort by name descending", async () => {
      const ids = await storage.list({ sortBy: "name", sortOrder: "desc" });
      expect(ids).toEqual(["node-template-3", "node-template-2", "node-template-1"]);
    });

    it("should support pagination", async () => {
      const ids = await storage.list({ offset: 1, limit: 1 });
      expect(ids).toHaveLength(1);
    });

    it("should combine filters with sorting", async () => {
      const ids = await storage.list({ type: "action", sortBy: "name", sortOrder: "asc" });
      expect(ids).toEqual(["node-template-1", "node-template-3"]);
    });
  });

  describe("exists", () => {
    it("should return true for existing node template", async () => {
      await storage.save("node-template-1", new Uint8Array([1]), createMetadata());
      expect(await storage.exists("node-template-1")).toBe(true);
    });

    it("should return false for non-existent node template", async () => {
      expect(await storage.exists("non-existent")).toBe(false);
    });
  });

  describe("getMetadata", () => {
    it("should return metadata for existing node template", async () => {
      const metadata = createMetadata({ name: "My Action" });
      await storage.save("node-template-1", new Uint8Array([1]), metadata);

      const loaded = await storage.getMetadata("node-template-1");
      expect(loaded).toEqual(metadata);
    });

    it("should return null for non-existent node template", async () => {
      const loaded = await storage.getMetadata("non-existent");
      expect(loaded).toBeNull();
    });
  });

  describe("clear", () => {
    it("should clear all node templates", async () => {
      await storage.save("node-template-1", new Uint8Array([1]), createMetadata());
      await storage.save("node-template-2", new Uint8Array([2]), createMetadata());

      await storage.clear();

      const ids = await storage.list();
      expect(ids).toHaveLength(0);
    });
  });

  describe("tags filtering", () => {
    beforeEach(async () => {
      await storage.save(
        "node-template-1",
        new Uint8Array([1]),
        createMetadata({
          tags: ["core", "essential"],
        }),
      );
      await storage.save(
        "node-template-2",
        new Uint8Array([2]),
        createMetadata({
          tags: ["extension"],
        }),
      );
    });

    it("should store node templates with tags", async () => {
      const metadata1 = await storage.getMetadata("node-template-1");
      expect(metadata1?.tags).toEqual(["core", "essential"]);

      const metadata2 = await storage.getMetadata("node-template-2");
      expect(metadata2?.tags).toEqual(["extension"]);
    });

    it("should filter by tags", async () => {
      const ids = await storage.list({ tags: ["core"] });
      expect(ids).toHaveLength(1);
      expect(ids).toContain("node-template-1");
    });
  });
});
