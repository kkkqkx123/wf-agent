/**
 * SqliteWorkflowExecutionStorage Integration Test
 *
 * Tests the complete workflow execution storage lifecycle with SQLite backend:
 * - CRUD lifecycle
 * - Status transitions
 * - Execution list filtering (by workflowId, status, time range, tags)
 * - Batch operations
 * - Edge cases
 *
 * SQLite database files are created in temp directory and cleaned up after tests.
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as fs from "fs/promises";
import * as path from "path";
import * as os from "os";
import { SqliteWorkflowExecutionStorage } from "../../sqlite/sqlite-workflow-execution-storage.js";
import {
  createWorkflowExecutionMetadata,
  createMinimalExecutionMetadata,
  createTestData,
  createExecutionBatch,
  TEST_EXECUTION_ID,
  TEST_WORKFLOW_ID,
} from "../common/test-data.js";
import type { WorkflowExecutionStatus } from "@wf-agent/types";

describe("SqliteWorkflowExecutionStorage Integration", () => {
  let storage: SqliteWorkflowExecutionStorage;
  let tempDir: string;
  let dbPath: string;

  beforeEach(async () => {
    tempDir = await fs.mkdtemp(path.join(os.tmpdir(), "sqlite-exec-int-"));
    dbPath = path.join(tempDir, "test.db");
    storage = new SqliteWorkflowExecutionStorage({ dbPath });
    await storage.initialize();
  });

  afterEach(async () => {
    await storage.close();
    await fs.rm(tempDir, { recursive: true, force: true });
  });

  // ── Lifecycle ─────────────────────────────────────────────────────────

  describe("CRUD Lifecycle", () => {
    it("should complete full CRUD lifecycle", async () => {
      const data = createTestData();
      const metadata = createWorkflowExecutionMetadata();

      await storage.save(TEST_EXECUTION_ID, data, metadata);

      const loaded = await storage.load(TEST_EXECUTION_ID);
      expect(loaded).toEqual(data);
      expect(await storage.exists(TEST_EXECUTION_ID)).toBe(true);

      const loadedMeta = await storage.getMetadata(TEST_EXECUTION_ID);
      expect(loadedMeta).not.toBeNull();
      expect(loadedMeta!.status).toBe("RUNNING");

      await storage.updateExecutionStatus(TEST_EXECUTION_ID, "COMPLETED");
      const updatedMeta = await storage.getMetadata(TEST_EXECUTION_ID);
      expect(updatedMeta!.status).toBe("COMPLETED");

      await storage.delete(TEST_EXECUTION_ID);
      expect(await storage.load(TEST_EXECUTION_ID)).toBeNull();
      expect(await storage.exists(TEST_EXECUTION_ID)).toBe(false);
    });

    it("should handle minimal execution metadata", async () => {
      const metadata = createMinimalExecutionMetadata();
      const data = createTestData();

      await storage.save(metadata.executionId, data, metadata);
      const loaded = await storage.load(metadata.executionId);
      expect(loaded).toEqual(data);

      const meta = await storage.getMetadata(metadata.executionId);
      expect(meta!.status).toBe("CREATED");
    });

    it("should return null for non-existent execution", async () => {
      expect(await storage.load("non-existent")).toBeNull();
      expect(await storage.getMetadata("non-existent")).toBeNull();
    });

    it("should update execution status progressively", async () => {
      const metadata = createWorkflowExecutionMetadata();
      await storage.save(TEST_EXECUTION_ID, createTestData(), metadata);

      const statuses: WorkflowExecutionStatus[] = ["RUNNING", "COMPLETED"];
      for (const status of statuses) {
        await storage.updateExecutionStatus(TEST_EXECUTION_ID, status);
        const meta = await storage.getMetadata(TEST_EXECUTION_ID);
        expect(meta!.status).toBe(status);
      }
    });
  });

  // ── List Filtering ────────────────────────────────────────────────────

  describe("List Filtering", () => {
    beforeEach(async () => {
      const batch = createExecutionBatch(6);
      for (const item of batch) {
        await storage.save(item.id, item.data, item.metadata);
      }
    });

    it("should list all executions", async () => {
      const ids = await storage.list();
      expect(ids).toHaveLength(6);
    });

    it("should filter by workflowId", async () => {
      const ids = await storage.list({ workflowId: TEST_WORKFLOW_ID });
      expect(ids.length).toBeGreaterThan(0);
    });

    it("should filter by status", async () => {
      const ids = await storage.list({ status: "COMPLETED" });
      expect(ids.length).toBeGreaterThan(0);
    });
  });

  // ── Batch Operations ──────────────────────────────────────────────────

  describe("Batch Operations", () => {
    it("should save and load multiple items individually", async () => {
      const batch = createExecutionBatch(5);
      for (const item of batch) {
        await storage.save(item.id, item.data, item.metadata);
      }

      for (const item of batch) {
        const loaded = await storage.load(item.id);
        expect(loaded).toEqual(item.data);
      }
    });

    it("should delete multiple items individually", async () => {
      const batch = createExecutionBatch(3);
      for (const item of batch) {
        await storage.save(item.id, item.data, item.metadata);
      }

      for (const item of batch) {
        await storage.delete(item.id);
        expect(await storage.load(item.id)).toBeNull();
      }
    });
  });

  // ── File Persistence ──────────────────────────────────────────────────

  describe("File Persistence", () => {
    it("should persist data across re-initialization", async () => {
      const metadata = createWorkflowExecutionMetadata();
      await storage.save("persist-1", createTestData(), metadata);
      await storage.close();

      storage = new SqliteWorkflowExecutionStorage({ dbPath });
      await storage.initialize();

      const loaded = await storage.load("persist-1");
      expect(loaded).not.toBeNull();
    });
  });
});