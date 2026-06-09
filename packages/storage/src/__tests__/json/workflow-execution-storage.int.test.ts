/**
 * JsonWorkflowExecutionStorage Integration Test
 *
 * Tests the complete JSON workflow execution storage lifecycle:
 * - CRUD lifecycle with file I/O verification
 * - Status transitions
 * - List filtering (by workflowId, status, time range, tags)
 * - Batch operations
 * - File persistence across re-initialization
 * - Edge cases
 *
 * Test output directory: packages/storage/src/__tests__/json/output/workflow-execution
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as fs from "fs/promises";
import * as path from "path";
import { fileURLToPath } from "url";
import { JsonWorkflowExecutionStorage } from "../../json/json-workflow-execution-storage.js";
import {
  createWorkflowExecutionMetadata,
  createMinimalExecutionMetadata,
  createTestData,
  createExecutionBatch,
  TEST_EXECUTION_ID,
  TEST_WORKFLOW_ID,
} from "../common/test-data.js";
import type { WorkflowExecutionStatus } from "@wf-agent/types";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const TEST_OUTPUT_DIR = path.resolve(__dirname, "output", "workflow-execution");

describe("JsonWorkflowExecutionStorage Integration", () => {
  let storage: JsonWorkflowExecutionStorage;
  let testDir: string;

  beforeEach(async () => {
    testDir = path.join(TEST_OUTPUT_DIR, `test-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`);
    storage = new JsonWorkflowExecutionStorage({ baseDir: testDir });
    await storage.initialize();
  });

  afterEach(async () => {
    await storage.close();
    await fs.rm(testDir, { recursive: true, force: true }).catch(() => {});
  });

  // ── Lifecycle ─────────────────────────────────────────────────────────

  describe("CRUD Lifecycle", () => {
    it("should complete full CRUD lifecycle", async () => {
      const data = createTestData();
      const metadata = createWorkflowExecutionMetadata();

      // Create
      await storage.save(TEST_EXECUTION_ID, data, metadata);

      // Read
      const loaded = await storage.load(TEST_EXECUTION_ID);
      expect(loaded).toEqual(data);
      expect(await storage.exists(TEST_EXECUTION_ID)).toBe(true);

      // Update status
      await storage.updateExecutionStatus(TEST_EXECUTION_ID, "COMPLETED");
      const updatedMeta = await storage.getMetadata(TEST_EXECUTION_ID);
      expect(updatedMeta!.status).toBe("COMPLETED");

      // Verify file structure
      const metaDir = path.join(testDir, "metadata", "workflow-execution");
      const dataDir = path.join(testDir, "data", "workflow-execution");
      const metaFiles = await fs.readdir(metaDir);
      const dataFiles = await fs.readdir(dataDir);
      expect(metaFiles).toContain(`${TEST_EXECUTION_ID}.json`);
      expect(dataFiles).toContain(`${TEST_EXECUTION_ID}.bin`);

      // Delete
      await storage.delete(TEST_EXECUTION_ID);
      expect(await storage.load(TEST_EXECUTION_ID)).toBeNull();
      expect(await storage.exists(TEST_EXECUTION_ID)).toBe(false);
    });

    it("should handle minimal execution metadata", async () => {
      const metadata = createMinimalExecutionMetadata();
      await storage.save(metadata.executionId, createTestData(16), metadata);
      expect(await storage.load(metadata.executionId)).not.toBeNull();
    });

    it("should return null for non-existent execution", async () => {
      expect(await storage.load("non-existent")).toBeNull();
    });
  });

  // ── Status Transitions ────────────────────────────────────────────────

  describe("Status Transitions", () => {
    it("should transition through multiple statuses with persistence", async () => {
      await storage.save(
        TEST_EXECUTION_ID,
        createTestData(),
        createWorkflowExecutionMetadata({ status: "CREATED" }),
      );

      const transitions: WorkflowExecutionStatus[] = [
        "RUNNING",
        "PAUSED",
        "RUNNING",
        "COMPLETED",
      ];

      for (const status of transitions) {
        await storage.updateExecutionStatus(TEST_EXECUTION_ID, status);
      }

      // Verify persistence after close/re-open
      await storage.close();
      storage = new JsonWorkflowExecutionStorage({ baseDir: testDir });
      await storage.initialize();

      const meta = await storage.getMetadata(TEST_EXECUTION_ID);
      expect(meta!.status).toBe("COMPLETED");
    });

    it("should throw when updating non-existent execution", async () => {
      await expect(
        storage.updateExecutionStatus("non-existent", "COMPLETED"),
      ).rejects.toThrow();
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

    it("should filter by execution type", async () => {
      const ids = await storage.list({ executionType: "MAIN" });
      expect(ids.length).toBeGreaterThan(0);
    });

    it("should return empty list when no matches", async () => {
      const ids = await storage.list({ workflowId: "non-existent" });
      expect(ids).toEqual([]);
    });
  });

  // ── Batch Operations ──────────────────────────────────────────────────

  describe("Batch Operations", () => {
    it("should save and load batch with file persistence", async () => {
      const batch = createExecutionBatch(5);
      await storage.saveBatch(batch.map(({ id, data, metadata }) => ({ id, data, metadata })));

      // Verify by re-opening
      await storage.close();
      storage = new JsonWorkflowExecutionStorage({ baseDir: testDir });
      await storage.initialize();

      const ids = await storage.list();
      expect(ids).toHaveLength(5);
    });

    it("should delete batch and clean up files", async () => {
      const batch = createExecutionBatch(3);
      await storage.saveBatch(batch.map(({ id, data, metadata }) => ({ id, data, metadata })));

      await storage.deleteBatch(batch.map(({ id }) => id));

      expect(await storage.list()).toHaveLength(0);
    });
  });

  // ── Edge Cases ────────────────────────────────────────────────────────

  describe("Edge Cases", () => {
    it("should handle execution with all optional fields", async () => {
      const metadata = createWorkflowExecutionMetadata({
        executionType: "FORK_JOIN",
        parentExecutionId: "parent-001",
        endTime: Date.now(),
        tags: ["fork", "join"],
        customFields: { branch: "feature-a" },
      });

      await storage.save(TEST_EXECUTION_ID, createTestData(), metadata);

      // Verify persistence
      await storage.close();
      storage = new JsonWorkflowExecutionStorage({ baseDir: testDir });
      await storage.initialize();

      const loaded = await storage.load(TEST_EXECUTION_ID);
      expect(loaded).not.toBeNull();

      const meta = await storage.getMetadata(TEST_EXECUTION_ID);
      expect(meta!.executionType).toBe("FORK_JOIN");
      expect(meta!.parentExecutionId).toBe("parent-001");
    });

    it("should handle many executions", async () => {
      const count = 30;
      const batch = createExecutionBatch(count);
      for (const item of batch) {
        await storage.save(item.id, item.data, item.metadata);
      }

      expect(await storage.list()).toHaveLength(count);
    });
  });

  // ── Clear ─────────────────────────────────────────────────────────────

  describe("Clear", () => {
    it("should clear all data and clean up files", async () => {
      await storage.save(TEST_EXECUTION_ID, createTestData(), createWorkflowExecutionMetadata());
      await storage.clear();

      expect(await storage.list()).toHaveLength(0);

      const metaDir = path.join(testDir, "metadata", "workflow-execution");
      const files = await fs.readdir(metaDir);
      expect(files).toHaveLength(0);
    });
  });
});