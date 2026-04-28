/**
 * JsonWorkflowStorage Tests
 * Tests for metadata-data separation
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as fs from "fs/promises";
import * as path from "path";
import * as os from "os";
import { JsonWorkflowStorage } from "../json-workflow-storage.js";
import type { WorkflowStorageMetadata } from "@wf-agent/types";

describe("JsonWorkflowStorage", () => {
  let storage: JsonWorkflowStorage;
  let tempDir: string;

  const createMetadata = (
    overrides?: Partial<WorkflowStorageMetadata>,
  ): WorkflowStorageMetadata => ({
    workflowId: "workflow-1",
    name: "Test Workflow",
    version: "1.0.0",
    nodeCount: 5,
    edgeCount: 4,
    createdAt: Date.now(),
    updatedAt: Date.now(),
    ...overrides,
  });

  beforeEach(async () => {
    tempDir = await fs.mkdtemp(path.join(os.tmpdir(), "json-workflow-test-"));
    storage = new JsonWorkflowStorage({ baseDir: tempDir });
    await storage.initialize();
  });

  afterEach(async () => {
    await storage.close();
    await fs.rm(tempDir, { recursive: true, force: true });
  });

  describe("initialize", () => {
    it("should create all directories including version directories", async () => {
      // Check metadata and data directories
      const metadataDir = path.join(tempDir, "metadata", "workflow");
      const dataDir = path.join(tempDir, "data", "workflow");
      const versionsMetadataDir = path.join(tempDir, "metadata", "versions");
      const versionsDataDir = path.join(tempDir, "data", "versions");

      const metadataStat = await fs.stat(metadataDir);
      const dataStat = await fs.stat(dataDir);
      const versionsMetadataStat = await fs.stat(versionsMetadataDir);
      const versionsDataStat = await fs.stat(versionsDataDir);

      expect(metadataStat.isDirectory()).toBe(true);
      expect(dataStat.isDirectory()).toBe(true);
      expect(versionsMetadataStat.isDirectory()).toBe(true);
      expect(versionsDataStat.isDirectory()).toBe(true);
    });
  });

  describe("save / load", () => {
    it("should save and load workflow", async () => {
      const workflowId = "workflow-1";
      const data = new Uint8Array([1, 2, 3, 4, 5]);
      const metadata = createMetadata();

      await storage.save(workflowId, data, metadata);

      const loaded = await storage.load(workflowId);
      expect(loaded).not.toBeNull();
      expect(loaded).toEqual(data);
    });

    it("should return null for non-existent workflow", async () => {
      const loaded = await storage.load("non-existent");
      expect(loaded).toBeNull();
    });

    it("should persist metadata and data to separate files", async () => {
      const workflowId = "workflow-1";
      const data = new Uint8Array([1, 2, 3]);

      await storage.save(workflowId, data, createMetadata());

      // Check metadata file exists
      const metadataFiles = await fs.readdir(path.join(tempDir, "metadata", "workflow"));
      expect(metadataFiles).toContain("workflow-1.json");

      // Check data file exists
      const dataFiles = await fs.readdir(path.join(tempDir, "data", "workflow"));
      expect(dataFiles).toContain("workflow-1.bin");
    });
  });

  describe("delete", () => {
    it("should delete workflow and its versions", async () => {
      const workflowId = "workflow-1";
      await storage.save(workflowId, new Uint8Array([1]), createMetadata());
      await storage.saveWorkflowVersion(workflowId, "1.0.0", new Uint8Array([1, 2]));

      await storage.delete(workflowId);

      const loaded = await storage.load(workflowId);
      expect(loaded).toBeNull();

      const versions = await storage.listWorkflowVersions(workflowId);
      expect(versions).toHaveLength(0);
    });
  });

  describe("list", () => {
    beforeEach(async () => {
      await storage.save(
        "workflow-1",
        new Uint8Array([1]),
        createMetadata({
          workflowId: "workflow-1",
          name: "Workflow One",
          author: "user-1",
          category: "test",
          enabled: true,
          createdAt: 1000,
          updatedAt: 1000,
        }),
      );
      await storage.save(
        "workflow-2",
        new Uint8Array([2]),
        createMetadata({
          workflowId: "workflow-2",
          name: "Workflow Two",
          author: "user-2",
          category: "prod",
          enabled: false,
          createdAt: 2000,
          updatedAt: 2000,
        }),
      );
      await storage.save(
        "workflow-3",
        new Uint8Array([3]),
        createMetadata({
          workflowId: "workflow-3",
          name: "Another Workflow",
          author: "user-1",
          category: "test",
          enabled: true,
          createdAt: 3000,
          updatedAt: 3000,
        }),
      );
    });

    it("should list all workflows", async () => {
      const ids = await storage.list();
      expect(ids).toHaveLength(3);
    });

    it("should filter by name (partial match)", async () => {
      const ids = await storage.list({ name: "Workflow" });
      expect(ids).toHaveLength(3);
    });

    it("should filter by author", async () => {
      const ids = await storage.list({ author: "user-1" });
      expect(ids).toHaveLength(2);
    });

    it("should filter by category", async () => {
      const ids = await storage.list({ category: "test" });
      expect(ids).toHaveLength(2);
    });

    it("should filter by enabled", async () => {
      const ids = await storage.list({ enabled: true });
      expect(ids).toHaveLength(2);
    });

    it("should filter by createdAt range", async () => {
      const ids = await storage.list({ createdAtFrom: 1500, createdAtTo: 2500 });
      expect(ids).toHaveLength(1);
      expect(ids).toContain("workflow-2");
    });

    it("should filter by updatedAt range", async () => {
      const ids = await storage.list({ updatedAtFrom: 2500 });
      expect(ids).toHaveLength(1);
      expect(ids).toContain("workflow-3");
    });

    it("should sort by updatedAt descending by default", async () => {
      const ids = await storage.list();
      expect(ids).toEqual(["workflow-3", "workflow-2", "workflow-1"]);
    });

    it("should sort by name", async () => {
      const ids = await storage.list({ sortBy: "name", sortOrder: "asc" });
      expect(ids).toEqual(["workflow-3", "workflow-1", "workflow-2"]);
    });

    it("should support pagination", async () => {
      const ids = await storage.list({ limit: 2 });
      expect(ids).toHaveLength(2);

      const ids2 = await storage.list({ limit: 2, offset: 2 });
      expect(ids2).toHaveLength(1);
    });
  });

  describe("exists", () => {
    it("should return true for existing workflow", async () => {
      await storage.save("workflow-1", new Uint8Array([1]), createMetadata());
      expect(await storage.exists("workflow-1")).toBe(true);
    });

    it("should return false for non-existent workflow", async () => {
      expect(await storage.exists("non-existent")).toBe(false);
    });
  });

  describe("getMetadata", () => {
    it("should return metadata for existing workflow", async () => {
      const metadata = createMetadata({ workflowId: "workflow-1" });
      await storage.save("workflow-1", new Uint8Array([1]), metadata);

      const loaded = await storage.getMetadata("workflow-1");
      expect(loaded).toEqual(metadata);
    });

    it("should return null for non-existent workflow", async () => {
      const loaded = await storage.getMetadata("non-existent");
      expect(loaded).toBeNull();
    });
  });

  describe("updateWorkflowMetadata", () => {
    it("should update workflow metadata", async () => {
      await storage.save(
        "workflow-1",
        new Uint8Array([1, 2, 3]),
        createMetadata({
          name: "Old Name",
          description: "Old description",
        }),
      );

      await storage.updateWorkflowMetadata("workflow-1", {
        name: "New Name",
        description: "New description",
      });

      const metadata = await storage.getMetadata("workflow-1");
      expect(metadata?.name).toBe("New Name");
      expect(metadata?.description).toBe("New description");
    });

    it("should preserve data when updating metadata", async () => {
      const data = new Uint8Array([1, 2, 3, 4, 5]);
      await storage.save("workflow-1", data, createMetadata());

      await storage.updateWorkflowMetadata("workflow-1", { name: "Updated" });

      const loaded = await storage.load("workflow-1");
      expect(loaded).toEqual(data);
    });

    it("should throw for non-existent workflow", async () => {
      await expect(
        storage.updateWorkflowMetadata("non-existent", { name: "test" }),
      ).rejects.toThrow();
    });
  });

  describe("workflow versions", () => {
    beforeEach(async () => {
      await storage.save(
        "workflow-1",
        new Uint8Array([1]),
        createMetadata({
          version: "1.0.0",
        }),
      );
    });

    it("should save workflow version with metadata-data separation", async () => {
      const versionData = new Uint8Array([1, 2, 3]);
      await storage.saveWorkflowVersion("workflow-1", "1.0.0", versionData, "Initial version");

      // Check version metadata file exists
      const versionMetaFiles = await fs.readdir(path.join(tempDir, "metadata", "versions"));
      expect(versionMetaFiles.some(f => f.startsWith("workflow-1_") && f.endsWith(".json"))).toBe(
        true,
      );

      // Check version data file exists
      const versionDataFiles = await fs.readdir(path.join(tempDir, "data", "versions"));
      expect(versionDataFiles.some(f => f.startsWith("workflow-1_") && f.endsWith(".bin"))).toBe(
        true,
      );

      const loaded = await storage.loadWorkflowVersion("workflow-1", "1.0.0");
      expect(loaded).toEqual(versionData);
    });

    it("should list workflow versions", async () => {
      await storage.saveWorkflowVersion("workflow-1", "1.0.0", new Uint8Array([1]), "v1");
      await storage.saveWorkflowVersion("workflow-1", "1.1.0", new Uint8Array([2]), "v2");

      const versions = await storage.listWorkflowVersions("workflow-1");
      expect(versions).toHaveLength(2);
      expect(versions.map(v => v.version)).toContain("1.0.0");
      expect(versions.map(v => v.version)).toContain("1.1.0");
    });

    it("should mark current version", async () => {
      await storage.saveWorkflowVersion("workflow-1", "1.0.0", new Uint8Array([1]));
      await storage.saveWorkflowVersion("workflow-1", "1.1.0", new Uint8Array([2]));

      const versions = await storage.listWorkflowVersions("workflow-1");
      const currentVersion = versions.find(v => v.version === "1.0.0");
      expect(currentVersion?.isCurrent).toBe(true);
    });

    it("should load specific version", async () => {
      const data1 = new Uint8Array([1, 2]);
      const data2 = new Uint8Array([3, 4]);

      await storage.saveWorkflowVersion("workflow-1", "1.0.0", data1);
      await storage.saveWorkflowVersion("workflow-1", "1.1.0", data2);

      const loaded = await storage.loadWorkflowVersion("workflow-1", "1.1.0");
      expect(loaded).toEqual(data2);
    });

    it("should return null for non-existent version", async () => {
      const loaded = await storage.loadWorkflowVersion("workflow-1", "99.0.0");
      expect(loaded).toBeNull();
    });

    it("should delete workflow version", async () => {
      await storage.saveWorkflowVersion("workflow-1", "1.0.0", new Uint8Array([1]));
      await storage.saveWorkflowVersion("workflow-1", "1.1.0", new Uint8Array([2]));

      await storage.deleteWorkflowVersion("workflow-1", "1.0.0");

      const versions = await storage.listWorkflowVersions("workflow-1");
      expect(versions).toHaveLength(1);
      expect(versions[0]!.version).toBe("1.1.0");
    });

    it("should support pagination for versions", async () => {
      await storage.saveWorkflowVersion("workflow-1", "1.0.0", new Uint8Array([1]));
      await storage.saveWorkflowVersion("workflow-1", "1.1.0", new Uint8Array([2]));
      await storage.saveWorkflowVersion("workflow-1", "1.2.0", new Uint8Array([3]));

      const versions = await storage.listWorkflowVersions("workflow-1", { limit: 2 });
      expect(versions).toHaveLength(2);
    });
  });

  describe("tags filtering", () => {
    beforeEach(async () => {
      await storage.save(
        "workflow-1",
        new Uint8Array([1]),
        createMetadata({
          tags: ["important", "production"],
        }),
      );
      await storage.save(
        "workflow-2",
        new Uint8Array([2]),
        createMetadata({
          tags: ["test"],
        }),
      );
    });

    it("should filter by tags", async () => {
      const ids = await storage.list({ tags: ["important"] });
      expect(ids).toHaveLength(1);
      expect(ids).toContain("workflow-1");
    });
  });

  describe("clear", () => {
    it("should clear all workflows and versions", async () => {
      await storage.save("workflow-1", new Uint8Array([1]), createMetadata());
      await storage.saveWorkflowVersion("workflow-1", "1.0.0", new Uint8Array([1]));

      await storage.clear();

      const ids = await storage.list();
      expect(ids).toHaveLength(0);

      const versions = await storage.listWorkflowVersions("workflow-1");
      expect(versions).toHaveLength(0);
    });
  });
});
