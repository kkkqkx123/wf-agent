/**
 * MemoryWorkflowExecutionStorage Integration Test
 *
 * Tests the complete workflow execution storage lifecycle:
 * - CRUD lifecycle
 * - Status transitions
 * - Execution list filtering (by workflowId, status, time range, tags)
 * - Batch operations
 * - Edge cases
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { MemoryWorkflowExecutionStorage } from "../../memory/memory-workflow-execution-storage.js";
import {
  createWorkflowExecutionMetadata,
  createMinimalExecutionMetadata,
  createTestData,
  createExecutionBatch,
  TEST_EXECUTION_ID,
  TEST_WORKFLOW_ID,
} from "../common/test-data.js";
import type { WorkflowExecutionStatus } from "@wf-agent/types";

describe("MemoryWorkflowExecutionStorage Integration", () => {
  let storage: MemoryWorkflowExecutionStorage;

  beforeEach(async () => {
    storage = new MemoryWorkflowExecutionStorage();
    await storage.initialize();
  });

  afterEach(async () => {
    await storage.close();
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

      const loadedMeta = await storage.getMetadata(TEST_EXECUTION_ID);
      expect(loadedMeta).not.toBeNull();
      expect(loadedMeta!.status).toBe("RUNNING");

      // Update status
      await storage.updateExecutionStatus(TEST_EXECUTION_ID, "COMPLETED");
      const updatedMeta = await storage.getMetadata(TEST_EXECUTION_ID);
      expect(updatedMeta!.status).toBe("COMPLETED");

      // Delete
      await storage.delete(TEST_EXECUTION_ID);
      expect(await storage.load(TEST_EXECUTION_ID)).toBeNull();
      expect(await storage.exists(TEST_EXECUTION_ID)).toBe(false);
    });

    it("should handle minimal execution metadata", async () => {
      const metadata = createMinimalExecutionMetadata();
      const data = createTestData(16);

      await storage.save(metadata.executionId, data, metadata);
      const loaded = await storage.load(metadata.executionId);
      expect(loaded).toEqual(data);
    });

    it("should return null for non-existent execution", async () => {
      expect(await storage.load("non-existent")).toBeNull();
      expect(await storage.getMetadata("non-existent")).toBeNull();
    });

    it("should overwrite existing execution data", async () => {
      const data1 = createTestData(16);
      const data2 = createTestData(32);
      const metadata = createWorkflowExecutionMetadata();

      await storage.save(TEST_EXECUTION_ID, data1, metadata);
      await storage.save(TEST_EXECUTION_ID, data2, metadata);

      expect(await storage.load(TEST_EXECUTION_ID)).toEqual(data2);
    });
  });

  // ── Status Transitions ────────────────────────────────────────────────

  describe("Status Transitions", () => {
    const ALL_STATUSES: WorkflowExecutionStatus[] = [
      "CREATED",
      "RUNNING",
      "PAUSED",
      "STOPPED",
      "COMPLETED",
      "FAILED",
      "CANCELLED",
      "TIMEOUT",
    ];

    for (const status of ALL_STATUSES) {
      it(`should support list filtering by status: ${status}`, async () => {
        for (const s of ALL_STATUSES) {
          const execId = `exec-${s.toLowerCase()}`;
          await storage.save(
            execId,
            createTestData(8),
            createWorkflowExecutionMetadata({ executionId: execId, status: s }),
          );
        }

        const ids = await storage.list({ status });
        expect(ids).toHaveLength(1);
        expect(ids[0]).toBe(`exec-${status.toLowerCase()}`);
      });
    }

    it("should transition through multiple statuses", async () => {
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
        const meta = await storage.getMetadata(TEST_EXECUTION_ID);
        expect(meta!.status).toBe(status);
      }
    });

    it("should throw when updating status of non-existent execution", async () => {
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

    it("should filter by status array", async () => {
      const ids = await storage.list({
        status: ["COMPLETED", "FAILED"],
      });
      expect(ids).toHaveLength(6);
    });

    it("should filter by time range", async () => {
      const now = Date.now();
      const ids = await storage.list({
        startTimeFrom: now - 3600000,
        startTimeTo: now + 3600000,
      });
      expect(ids).toHaveLength(6);
    });

    it("should filter by tags", async () => {
      const ids = await storage.list({ tags: ["test"] });
      expect(ids.length).toBeGreaterThan(0);
    });

    it("should return empty list when no matches", async () => {
      const ids = await storage.list({
        workflowId: "non-existent-workflow",
      });
      expect(ids).toEqual([]);
    });
  });

  // ── Batch Operations ──────────────────────────────────────────────────

  describe("Batch Operations", () => {
    it("should save and load batch", async () => {
      const batch = createExecutionBatch(5);
      await storage.saveBatch(batch.map(({ id, data, metadata }) => ({ id, data, metadata })));

      const loaded = await storage.loadBatch(batch.map(({ id }) => id));
      expect(loaded).toHaveLength(5);
      expect(loaded.every((item) => item.data !== null)).toBe(true);
    });

    it("should delete batch", async () => {
      const batch = createExecutionBatch(3);
      await storage.saveBatch(batch.map(({ id, data, metadata }) => ({ id, data, metadata })));

      await storage.deleteBatch(batch.map(({ id }) => id));
      expect(await storage.list()).toHaveLength(0);
    });
  });

  // ── Edge Cases ────────────────────────────────────────────────────────

  describe("Edge Cases", () => {
    it("should handle many executions", async () => {
      const count = 50;
      const batch = createExecutionBatch(count);
      await storage.saveBatch(batch.map(({ id, data, metadata }) => ({ id, data, metadata })));

      const ids = await storage.list();
      expect(ids).toHaveLength(count);
    });

    it("should handle execution with all optional fields", async () => {
      const metadata = createWorkflowExecutionMetadata({
        executionType: "FORK_JOIN" as const,
        parentExecutionId: "parent-exec-001",
        endTime: Date.now(),
        tags: ["fork", "join", "parallel"],
        customFields: { branch: "feature-a", attempt: 3 },
      });

      await storage.save(TEST_EXECUTION_ID, createTestData(), metadata);
      const loaded = await storage.load(TEST_EXECUTION_ID);
      expect(loaded).not.toBeNull();
    });
  });

  // ── Clear ─────────────────────────────────────────────────────────────

  describe("Clear", () => {
    it("should clear all stored data", async () => {
      await storage.save(TEST_EXECUTION_ID, createTestData(), createWorkflowExecutionMetadata());
      await storage.clear();

      expect(await storage.list()).toHaveLength(0);
      expect(await storage.load(TEST_EXECUTION_ID)).toBeNull();
    });
  });
});