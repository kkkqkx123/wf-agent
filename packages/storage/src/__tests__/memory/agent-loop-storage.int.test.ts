/**
 * MemoryAgentLoopStorage Integration Test
 *
 * Tests the complete agent loop storage lifecycle:
 * - CRUD lifecycle
 * - Status transitions and listByStatus
 * - Agent loop statistics (getAgentLoopStats)
 * - List filtering
 * - Batch operations
 * - Edge cases
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { MemoryAgentLoopStorage } from "../../memory/memory-agent-loop-storage.js";
import {
  createAgentEntityMetadata,
  createMinimalAgentEntityMetadata,
  createTestData,
  createAgentEntityBatch,
  TEST_AGENT_LOOP_ID,
} from "../common/test-data.js";
import type { AgentLoopStatus } from "@wf-agent/types";

describe("MemoryAgentLoopStorage Integration", () => {
  let storage: MemoryAgentLoopStorage;

  beforeEach(async () => {
    storage = new MemoryAgentLoopStorage();
    await storage.initialize();
  });

  afterEach(async () => {
    await storage.close();
  });

  // ── Lifecycle ─────────────────────────────────────────────────────────

  describe("CRUD Lifecycle", () => {
    it("should complete full CRUD lifecycle", async () => {
      const data = createTestData();
      const metadata = createAgentEntityMetadata();

      // Create
      await storage.save(TEST_AGENT_LOOP_ID, data, metadata);

      // Read
      const loaded = await storage.load(TEST_AGENT_LOOP_ID);
      expect(loaded).toEqual(data);
      expect(await storage.exists(TEST_AGENT_LOOP_ID)).toBe(true);

      // Update status
      await storage.updateAgentLoopStatus(TEST_AGENT_LOOP_ID, "COMPLETED");
      const updatedMeta = await storage.getMetadata(TEST_AGENT_LOOP_ID);
      expect(updatedMeta!.status).toBe("COMPLETED");
      expect(updatedMeta!.completedAt).toBeDefined();

      // Delete
      await storage.delete(TEST_AGENT_LOOP_ID);
      expect(await storage.load(TEST_AGENT_LOOP_ID)).toBeNull();
      expect(await storage.exists(TEST_AGENT_LOOP_ID)).toBe(false);
    });

    it("should handle minimal agent entity metadata", async () => {
      const metadata = createMinimalAgentEntityMetadata();
      const data = createTestData(16);

      await storage.save(metadata.agentLoopId, data, metadata);
      expect(await storage.load(metadata.agentLoopId)).toEqual(data);
    });

    it("should return null for non-existent agent loop", async () => {
      expect(await storage.load("non-existent")).toBeNull();
      expect(await storage.getMetadata("non-existent")).toBeNull();
    });

    it("should overwrite existing data", async () => {
      const data1 = createTestData(16);
      const data2 = createTestData(32);

      await storage.save(TEST_AGENT_LOOP_ID, data1, createAgentEntityMetadata());
      await storage.save(TEST_AGENT_LOOP_ID, data2, createAgentEntityMetadata());

      expect(await storage.load(TEST_AGENT_LOOP_ID)).toEqual(data2);
    });
  });

  // ── Status Management ─────────────────────────────────────────────────

  describe("Status Management", () => {
    it("should update status and record completedAt for terminal states", async () => {
      await storage.save(
        TEST_AGENT_LOOP_ID,
        createTestData(),
        createAgentEntityMetadata({ status: "CREATED" }),
      );

      // CREATED → RUNNING
      await storage.updateAgentLoopStatus(TEST_AGENT_LOOP_ID, "RUNNING");
      let meta = await storage.getMetadata(TEST_AGENT_LOOP_ID);
      expect(meta!.status).toBe("RUNNING");
      expect(meta!.completedAt).toBeUndefined();

      // RUNNING → COMPLETED
      await storage.updateAgentLoopStatus(TEST_AGENT_LOOP_ID, "COMPLETED");
      meta = await storage.getMetadata(TEST_AGENT_LOOP_ID);
      expect(meta!.status).toBe("COMPLETED");
      expect(meta!.completedAt).toBeDefined();

      // COMPLETED → CANCELLED (should also record completedAt)
      await storage.updateAgentLoopStatus(TEST_AGENT_LOOP_ID, "CANCELLED");
      meta = await storage.getMetadata(TEST_AGENT_LOOP_ID);
      expect(meta!.status).toBe("CANCELLED");
    });

    it("should list agent loops by status", async () => {
      const statuses: AgentLoopStatus[] = [
        "CREATED",
        "RUNNING",
        "COMPLETED",
        "FAILED",
        "CANCELLED",
        "PAUSED",
      ];
      for (const s of statuses) {
        const id = `agent-${s.toLowerCase()}`;
        await storage.save(id, createTestData(8), createAgentEntityMetadata({
          agentLoopId: id,
          status: s,
        }));
      }

      for (const s of statuses) {
        const ids = await storage.listByStatus(s);
        expect(ids).toHaveLength(1);
        expect(ids[0]).toBe(`agent-${s.toLowerCase()}`);
      }
    });

    it("should throw when updating non-existent agent loop", async () => {
      await expect(
        storage.updateAgentLoopStatus("non-existent", "COMPLETED"),
      ).rejects.toThrow();
    });
  });

  // ── Statistics ────────────────────────────────────────────────────────

  describe("Agent Loop Statistics", () => {
    it("should compute stats correctly", async () => {
      const batch = createAgentEntityBatch(12);
      await storage.saveBatch(batch.map(({ id, data, metadata }) => ({ id, data, metadata })));

      const stats = await storage.getAgentLoopStats();
      expect(stats.total).toBe(12);
      // Each of the 6 statuses appears twice
      for (const count of Object.values(stats.byStatus)) {
        expect(count).toBe(2);
      }
    });

    it("should return zero stats for empty storage", async () => {
      const stats = await storage.getAgentLoopStats();
      expect(stats.total).toBe(0);
      expect(stats.byStatus["CREATED"]).toBe(0);
      expect(stats.byStatus["RUNNING"]).toBe(0);
    });
  });

  // ── List Filtering ────────────────────────────────────────────────────

  describe("List Filtering", () => {
    beforeEach(async () => {
      const batch = createAgentEntityBatch(6);
      for (const item of batch) {
        await storage.save(item.id, item.data, item.metadata);
      }
    });

    it("should list all agent loops", async () => {
      const ids = await storage.list();
      expect(ids).toHaveLength(6);
    });

    it("should filter by status", async () => {
      const ids = await storage.list(
        { status: "RUNNING" },
      );
      expect(ids.length).toBeGreaterThan(0);
    });

    it("should filter by profileId", async () => {
      const ids = await storage.list({ profileId: "profile-001" });
      expect(ids.length).toBeGreaterThan(0);
    });

    it("should filter by tags", async () => {
      const ids = await storage.list({ tags: ["test"] });
      expect(ids.length).toBeGreaterThan(0);
    });
  });

  // ── Batch Operations ──────────────────────────────────────────────────

  describe("Batch Operations", () => {
    it("should save and load batch", async () => {
      const batch = createAgentEntityBatch(5);
      await storage.saveBatch(batch.map(({ id, data, metadata }) => ({ id, data, metadata })));

      const loaded = await storage.loadBatch(batch.map(({ id }) => id));
      expect(loaded).toHaveLength(5);
      expect(loaded.every((item) => item.data !== null)).toBe(true);
    });

    it("should delete batch", async () => {
      const batch = createAgentEntityBatch(3);
      await storage.saveBatch(batch.map(({ id, data, metadata }) => ({ id, data, metadata })));

      await storage.deleteBatch(batch.map(({ id }) => id));
      expect(await storage.list()).toHaveLength(0);
    });
  });

  // ── Edge Cases ────────────────────────────────────────────────────────

  describe("Edge Cases", () => {
    it("should handle many agent loops", async () => {
      const count = 50;
      const batch = createAgentEntityBatch(count);
      await storage.saveBatch(batch.map(({ id, data, metadata }) => ({ id, data, metadata })));

      expect(await storage.list()).toHaveLength(count);
      const stats = await storage.getAgentLoopStats();
      expect(stats.total).toBe(count);
    });

    it("should handle agent loop with all custom fields", async () => {
      const metadata = createAgentEntityMetadata({
        tags: ["production", "high-priority", "monitored"],
        customFields: { team: "ai", region: "us-east", env: "prod" },
      });

      await storage.save(TEST_AGENT_LOOP_ID, createTestData(), metadata);
      const meta = await storage.getMetadata(TEST_AGENT_LOOP_ID);
      expect(meta!.tags).toEqual(["production", "high-priority", "monitored"]);
      expect(meta!.customFields).toEqual({ team: "ai", region: "us-east", env: "prod" });
    });
  });

  // ── Clear ─────────────────────────────────────────────────────────────

  describe("Clear", () => {
    it("should clear all agent loops", async () => {
      await storage.save(TEST_AGENT_LOOP_ID, createTestData(), createAgentEntityMetadata());
      await storage.clear();

      expect(await storage.list()).toHaveLength(0);
      expect(await storage.load(TEST_AGENT_LOOP_ID)).toBeNull();
    });
  });
});