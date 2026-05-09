/**
 * Storage Integration Tests
 * 
 * These tests verify the core storage functionality across different backends.
 * They specifically test:
 * 1. JSON file storage initialization and CRUD operations
 * 2. SQLite storage initialization and CRUD operations
 * 3. Path resolution (relative vs absolute)
 * 4. Directory structure creation
 * 5. Data persistence and retrieval
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as fs from "fs/promises";
import * as path from "path";
import * as os from "os";
import { tmpdir } from "os";
import { JsonWorkflowStorage } from "../json/json-workflow-storage.js";
import { SqliteWorkflowStorage } from "../sqlite/sqlite-workflow-storage.js";
import type { WorkflowStorageMetadata } from "@wf-agent/types";

describe("Storage Integration Tests", () => {
  let tempBaseDir: string;

  const createMetadata = (
    workflowId: string,
    overrides?: Partial<WorkflowStorageMetadata>,
  ): WorkflowStorageMetadata => ({
    workflowId,
    name: `Test Workflow ${workflowId}`,
    version: "1.0.0",
    nodeCount: 5,
    edgeCount: 4,
    createdAt: Date.now(),
    updatedAt: Date.now(),
    ...overrides,
  });

  beforeEach(async () => {
    // Create a unique temp directory for each test using tmpdir()
    tempBaseDir = await fs.mkdtemp(path.join(tmpdir(), "storage-integration-test-"));
  });

  afterEach(async () => {
    // Clean up temp directory after each test
    try {
      await fs.rm(tempBaseDir, { recursive: true, force: true });
    } catch (error) {
      console.warn(`Failed to clean up temp directory: ${tempBaseDir}`, error);
    }
  });

  describe("JSON File Storage", () => {
    let storage: JsonWorkflowStorage;

    beforeEach(async () => {
      storage = new JsonWorkflowStorage({ baseDir: tempBaseDir });
      await storage.initialize();
    });

    afterEach(async () => {
      await storage.close();
    });

    it("should initialize and create correct directory structure", async () => {
      // Verify all required directories are created
      const expectedDirs = [
        path.join(tempBaseDir, "metadata", "workflow"),
        path.join(tempBaseDir, "data", "workflow"),
        path.join(tempBaseDir, "metadata", "versions"),
        path.join(tempBaseDir, "data", "versions"),
      ];

      for (const dir of expectedDirs) {
        const stat = await fs.stat(dir);
        expect(stat.isDirectory()).toBe(true);
      }
    });

    it("should save and load workflow data correctly", async () => {
      const workflowId = "test-workflow-1";
      const testData = new Uint8Array([1, 2, 3, 4, 5]);
      const metadata = createMetadata(workflowId);

      // Save workflow
      await storage.save(workflowId, testData, metadata);

      // Load workflow
      const loaded = await storage.load(workflowId);
      expect(loaded).not.toBeNull();
      expect(loaded).toEqual(testData);

      // Verify files exist on disk
      const metadataFile = path.join(tempBaseDir, "metadata", "workflow", `${workflowId}.json`);
      const dataFile = path.join(tempBaseDir, "data", "workflow", `${workflowId}.bin`);

      const metadataStat = await fs.stat(metadataFile);
      const dataStat = await fs.stat(dataFile);

      expect(metadataStat.isFile()).toBe(true);
      expect(dataStat.isFile()).toBe(true);
    });

    it("should handle multiple workflows independently", async () => {
      const workflow1 = "workflow-alpha";
      const workflow2 = "workflow-beta";
      
      const data1 = new Uint8Array([10, 20, 30]);
      const data2 = new Uint8Array([40, 50, 60]);

      await storage.save(workflow1, data1, createMetadata(workflow1));
      await storage.save(workflow2, data2, createMetadata(workflow2));

      const loaded1 = await storage.load(workflow1);
      const loaded2 = await storage.load(workflow2);

      expect(loaded1).toEqual(data1);
      expect(loaded2).toEqual(data2);
      expect(loaded1).not.toEqual(loaded2);
    });

    it("should return null for non-existent workflow", async () => {
      const loaded = await storage.load("non-existent-workflow");
      expect(loaded).toBeNull();
    });

    it("should delete workflow completely", async () => {
      const workflowId = "to-be-deleted";
      const testData = new Uint8Array([99, 98, 97]);

      await storage.save(workflowId, testData, createMetadata(workflowId));
      
      // Verify it exists
      const beforeDelete = await storage.load(workflowId);
      expect(beforeDelete).not.toBeNull();

      // Delete it
      await storage.delete(workflowId);

      // Verify it's gone
      const afterDelete = await storage.load(workflowId);
      expect(afterDelete).toBeNull();

      // Verify files are removed from disk
      const metadataFile = path.join(tempBaseDir, "metadata", "workflow", `${workflowId}.json`);
      const dataFile = path.join(tempBaseDir, "data", "workflow", `${workflowId}.bin`);

      await expect(fs.stat(metadataFile)).rejects.toThrow();
      await expect(fs.stat(dataFile)).rejects.toThrow();
    });

    it("should list workflows correctly", async () => {
      await storage.save("wf-1", new Uint8Array([1]), createMetadata("wf-1"));
      await storage.save("wf-2", new Uint8Array([2]), createMetadata("wf-2"));
      await storage.save("wf-3", new Uint8Array([3]), createMetadata("wf-3"));

      const list = await storage.list();
      expect(list).toHaveLength(3);
      expect(list).toContain("wf-1");
      expect(list).toContain("wf-2");
      expect(list).toContain("wf-3");
    });

    it("should handle absolute paths correctly", async () => {
      // Create a subdirectory with absolute path
      const absolutePath = path.join(tempBaseDir, "absolute-test");
      await fs.mkdir(absolutePath, { recursive: true });

      const absoluteStorage = new JsonWorkflowStorage({ baseDir: absolutePath });
      await absoluteStorage.initialize();

      const workflowId = "absolute-path-wf";
      const testData = new Uint8Array([255, 254, 253]);

      await absoluteStorage.save(workflowId, testData, createMetadata(workflowId));
      const loaded = await absoluteStorage.load(workflowId);

      expect(loaded).toEqual(testData);

      // Verify data is in the absolute path location
      const dataFile = path.join(absolutePath, "data", "workflow", `${workflowId}.bin`);
      const stat = await fs.stat(dataFile);
      expect(stat.isFile()).toBe(true);

      await absoluteStorage.close();
    });
  });

  describe("SQLite Storage", () => {
    let storage: SqliteWorkflowStorage;

    beforeEach(async () => {
      const dbPath = path.join(tempBaseDir, "test.db");
      storage = new SqliteWorkflowStorage({ dbPath });
      await storage.initialize();
    });

    afterEach(async () => {
      await storage.close();
    });

    it("should initialize and create database file", async () => {
      const dbPath = path.join(tempBaseDir, "test.db");
      const stat = await fs.stat(dbPath);
      expect(stat.isFile()).toBe(true);
    });

    it("should save and load workflow data correctly", async () => {
      const workflowId = "sqlite-test-1";
      const testData = new Uint8Array([100, 200, 150]);
      const metadata = createMetadata(workflowId);

      await storage.save(workflowId, testData, metadata);

      const loaded = await storage.load(workflowId);
      expect(loaded).not.toBeNull();
      expect(loaded).toEqual(testData);
    });

    it("should handle multiple workflows independently", async () => {
      const wf1 = "sqlite-wf-1";
      const wf2 = "sqlite-wf-2";
      
      await storage.save(wf1, new Uint8Array([1, 2]), createMetadata(wf1));
      await storage.save(wf2, new Uint8Array([3, 4]), createMetadata(wf2));

      const loaded1 = await storage.load(wf1);
      const loaded2 = await storage.load(wf2);

      expect(loaded1).toEqual(new Uint8Array([1, 2]));
      expect(loaded2).toEqual(new Uint8Array([3, 4]));
    });

    it("should return null for non-existent workflow", async () => {
      const loaded = await storage.load("does-not-exist");
      expect(loaded).toBeNull();
    });

    it("should delete workflow from database", async () => {
      const workflowId = "sqlite-delete-test";
      await storage.save(workflowId, new Uint8Array([99]), createMetadata(workflowId));

      const beforeDelete = await storage.load(workflowId);
      expect(beforeDelete).not.toBeNull();

      await storage.delete(workflowId);

      const afterDelete = await storage.load(workflowId);
      expect(afterDelete).toBeNull();
    });

    it("should list workflows from database", async () => {
      await storage.save("sqlite-1", new Uint8Array([1]), createMetadata("sqlite-1"));
      await storage.save("sqlite-2", new Uint8Array([2]), createMetadata("sqlite-2"));

      const list = await storage.list();
      expect(list.length).toBeGreaterThanOrEqual(2);
      expect(list).toContain("sqlite-1");
      expect(list).toContain("sqlite-2");
    });
  });

  describe("Path Resolution", () => {
    it("should handle relative paths by resolving them to absolute", async () => {
      // Create a relative path within temp directory
      const relativePath = path.relative(process.cwd(), tempBaseDir);
      
      const storage = new JsonWorkflowStorage({ baseDir: relativePath });
      await storage.initialize();

      const workflowId = "relative-path-test";
      await storage.save(workflowId, new Uint8Array([42]), createMetadata(workflowId));

      const loaded = await storage.load(workflowId);
      expect(loaded).toEqual(new Uint8Array([42]));

      await storage.close();
    });

    it("should maintain isolation between different storage instances", async () => {
      const dir1 = path.join(tempBaseDir, "instance-1");
      const dir2 = path.join(tempBaseDir, "instance-2");

      await fs.mkdir(dir1, { recursive: true });
      await fs.mkdir(dir2, { recursive: true });

      const storage1 = new JsonWorkflowStorage({ baseDir: dir1 });
      const storage2 = new JsonWorkflowStorage({ baseDir: dir2 });

      await storage1.initialize();
      await storage2.initialize();

      // Save different data to each
      await storage1.save("shared-id", new Uint8Array([1]), createMetadata("shared-id"));
      await storage2.save("shared-id", new Uint8Array([2]), createMetadata("shared-id"));

      const loaded1 = await storage1.load("shared-id");
      const loaded2 = await storage2.load("shared-id");

      // Each should have its own data
      expect(loaded1).toEqual(new Uint8Array([1]));
      expect(loaded2).toEqual(new Uint8Array([2]));

      await storage1.close();
      await storage2.close();
    });
  });
});
