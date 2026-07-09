/**
 * JsonHookTemplateStorage Tests
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as fs from "fs/promises";
import * as path from "path";
import * as os from "os";
import { JsonHookTemplateStorage } from "../json-hook-template-storage.js";
import type { HookTemplateStorageMetadata } from "@wf-agent/types";

describe("JsonHookTemplateStorage", () => {
  let storage: JsonHookTemplateStorage;
  let tempDir: string;

  const createMetadata = (
    overrides?: Partial<HookTemplateStorageMetadata>,
  ): HookTemplateStorageMetadata => ({
    name: "Test Hook Template",
    hookType: "beforeExecution",
    description: "Test hook template description",
    category: "system",
    createdAt: Date.now(),
    updatedAt: Date.now(),
    ...overrides,
  });

  beforeEach(async () => {
    tempDir = await fs.mkdtemp(path.join(os.tmpdir(), "json-hook-template-test-"));
    storage = new JsonHookTemplateStorage({ baseDir: tempDir });
    await storage.initialize();
  });

  afterEach(async () => {
    await storage.close();
    await fs.rm(tempDir, { recursive: true, force: true });
  });

  describe("initialize", () => {
    it("should create metadata and data directories", async () => {
      const metadataDir = path.join(tempDir, "metadata", "hookTemplate");
      const dataDir = path.join(tempDir, "data", "hookTemplate");

      const metadataStat = await fs.stat(metadataDir);
      const dataStat = await fs.stat(dataDir);

      expect(metadataStat.isDirectory()).toBe(true);
      expect(dataStat.isDirectory()).toBe(true);
    });
  });

  describe("save / load", () => {
    it("should save and load hook template", async () => {
      const templateId = "hook-template-1";
      const data = new Uint8Array([1, 2, 3, 4, 5]);
      const metadata = createMetadata();

      await storage.save(templateId, data, metadata);

      const loaded = await storage.load(templateId);
      expect(loaded).not.toBeNull();
      expect(loaded).toEqual(data);
    });

    it("should return null for non-existent hook template", async () => {
      const loaded = await storage.load("non-existent");
      expect(loaded).toBeNull();
    });

    it("should persist metadata and data to separate files", async () => {
      const templateId = "hook-template-1";
      const data = new Uint8Array([1, 2, 3]);

      await storage.save(templateId, data, createMetadata());

      const metadataFiles = await fs.readdir(path.join(tempDir, "metadata", "hookTemplate"));
      expect(metadataFiles).toContain("hook-template-1.json");

      const dataFiles = await fs.readdir(path.join(tempDir, "data", "hookTemplate"));
      expect(dataFiles).toContain("hook-template-1.bin");
    });
  });

  describe("delete", () => {
    it("should delete hook template", async () => {
      const templateId = "hook-template-1";
      await storage.save(templateId, new Uint8Array([1]), createMetadata());

      await storage.delete(templateId);

      const loaded = await storage.load(templateId);
      expect(loaded).toBeNull();
    });
  });

  describe("list", () => {
    beforeEach(async () => {
      await storage.save(
        "hook-template-1",
        new Uint8Array([1]),
        createMetadata({
          name: "Before Execution Hook",
          hookType: "beforeExecution",
          category: "system",
          createdAt: 1000,
          updatedAt: 1000,
        }),
      );
      await storage.save(
        "hook-template-2",
        new Uint8Array([2]),
        createMetadata({
          name: "After Execution Hook",
          hookType: "afterExecution",
          category: "system",
          createdAt: 2000,
          updatedAt: 2000,
        }),
      );
      await storage.save(
        "hook-template-3",
        new Uint8Array([3]),
        createMetadata({
          name: "Custom Hook",
          hookType: "custom",
          category: "user",
          createdAt: 3000,
          updatedAt: 3000,
        }),
      );
    });

    it("should list all hook templates", async () => {
      const ids = await storage.list();
      expect(ids).toHaveLength(3);
    });

    it("should return hook template IDs in insertion order", async () => {
      const ids = await storage.list();
      expect(ids).toContain("hook-template-1");
      expect(ids).toContain("hook-template-2");
      expect(ids).toContain("hook-template-3");
    });

    it("should filter by nameContains", async () => {
      const ids = await storage.list({ nameContains: "Before Execution" });
      expect(ids).toHaveLength(1);
      expect(ids).toContain("hook-template-1");
    });

    it("should filter by hookType", async () => {
      const ids = await storage.list({ hookType: "beforeExecution" });
      expect(ids).toHaveLength(1);
      expect(ids).toContain("hook-template-1");
    });

    it("should filter by multiple hookTypes", async () => {
      const ids = await storage.list({ hookType: ["beforeExecution", "afterExecution"] });
      expect(ids).toHaveLength(2);
    });

    it("should filter by category", async () => {
      const ids = await storage.list({ category: "system" });
      expect(ids).toHaveLength(2);
      expect(ids).toContain("hook-template-1");
      expect(ids).toContain("hook-template-2");
    });

    it("should filter by createdAfter", async () => {
      const ids = await storage.list({ createdAfter: 2500 });
      expect(ids).toHaveLength(1);
      expect(ids).toContain("hook-template-3");
    });

    it("should sort by hookType ascending", async () => {
      const ids = await storage.list({ sortBy: "hookType", sortOrder: "asc" });
      expect(ids).toEqual(["hook-template-2", "hook-template-1", "hook-template-3"]);
    });

    it("should sort by createdAt descending", async () => {
      const ids = await storage.list({ sortBy: "createdAt", sortOrder: "desc" });
      expect(ids).toEqual(["hook-template-3", "hook-template-2", "hook-template-1"]);
    });

    it("should support pagination", async () => {
      const ids = await storage.list({ offset: 1, limit: 1 });
      expect(ids).toHaveLength(1);
    });

    it("should combine filters with sorting", async () => {
      const ids = await storage.list({ category: "system", sortBy: "name", sortOrder: "asc" });
      expect(ids).toEqual(["hook-template-2", "hook-template-1"]);
    });
  });

  describe("exists", () => {
    it("should return true for existing hook template", async () => {
      await storage.save("hook-template-1", new Uint8Array([1]), createMetadata());
      expect(await storage.exists("hook-template-1")).toBe(true);
    });

    it("should return false for non-existent hook template", async () => {
      expect(await storage.exists("non-existent")).toBe(false);
    });
  });

  describe("getMetadata", () => {
    it("should return metadata for existing hook template", async () => {
      const metadata = createMetadata({ name: "My Hook" });
      await storage.save("hook-template-1", new Uint8Array([1]), metadata);

      const loaded = await storage.getMetadata("hook-template-1");
      expect(loaded).toEqual(metadata);
    });

    it("should return null for non-existent hook template", async () => {
      const loaded = await storage.getMetadata("non-existent");
      expect(loaded).toBeNull();
    });
  });

  describe("clear", () => {
    it("should clear all hook templates", async () => {
      await storage.save("hook-template-1", new Uint8Array([1]), createMetadata());
      await storage.save("hook-template-2", new Uint8Array([2]), createMetadata());

      await storage.clear();

      const ids = await storage.list();
      expect(ids).toHaveLength(0);
    });
  });

  describe("tags filtering", () => {
    beforeEach(async () => {
      await storage.save(
        "hook-template-1",
        new Uint8Array([1]),
        createMetadata({
          tags: ["logging", "monitoring"],
        }),
      );
      await storage.save(
        "hook-template-2",
        new Uint8Array([2]),
        createMetadata({
          tags: ["security"],
        }),
      );
    });

    it("should store hook templates with tags", async () => {
      const metadata1 = await storage.getMetadata("hook-template-1");
      expect(metadata1?.tags).toEqual(["logging", "monitoring"]);

      const metadata2 = await storage.getMetadata("hook-template-2");
      expect(metadata2?.tags).toEqual(["security"]);
    });

    it("should filter by tags", async () => {
      const ids = await storage.list({ tags: ["logging"] });
      expect(ids).toHaveLength(1);
      expect(ids).toContain("hook-template-1");
    });
  });
});
