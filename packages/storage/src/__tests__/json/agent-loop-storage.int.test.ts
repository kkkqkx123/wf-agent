/**
 * JsonAgentLoopStorage Integration Test
 *
 * Tests the complete JSON agent loop storage lifecycle:
 * - CRUD lifecycle with file I/O verification
 * - Status transitions and listByStatus
 * - Agent loop statistics
 * - List filtering
 * - Batch operations
 * - File persistence across re-initialization
 * - Edge cases
 *
 * Test output directory: packages/storage/src/__tests__/json/output/agent-loop
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as fs from "fs/promises";
import * as path from "path";
import { fileURLToPath } from "url";
import { JsonAgentLoopStorage } from "../../json/json-agent-loop-storage.js";
import {
  createAgentEntityMetadata,
  createMinimalAgentEntityMetadata,
  createTestData,
  createAgentEntityBatch,
  TEST_AGENT_LOOP_ID,
} from "../common/test-data.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const TEST_OUTPUT_DIR = path.resolve(__dirname, "output", "agent-loop");

describe("JsonAgentLoopStorage Integration", () => {
  let storage: JsonAgentLoopStorage;
  let testDir: string;

  beforeEach(async () => {
    testDir = path.join(TEST_OUTPUT_DIR, `test-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`);
    storage = new JsonAgentLoopStorage({ baseDir: testDir });
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
      const metadata = createAgentEntityMetadata();

      // Create
      await storage.save(TEST_AGENT_LOOP_ID, data, metadata);

      // Read
      const loaded = await storage.load(TEST_AGENT_LOOP_ID);
      expect(loaded).toEqual(data);

      // Update status
      await storage.updateAgentLoopStatus(TEST_AGENT_LOOP_ID, "COMPLETED");
      const updatedMeta = await storage.getMetadata(TEST_AGENT_LOOP_ID);
      expect(updatedMeta!.status).toBe("COMPLETED");

      // Verify file structure
      const metaDir = path.join(testDir, "metadata", "agent-loop");
      const dataDir = path.join(testDir, "data", "agent-loop");
      expect(await fs.readdir(metaDir)).toContain(`${TEST_AGENT_LOOP_ID}.json`);
      expect(await fs.readdir(dataDir)).toContain(`${TEST_AGENT_LOOP_ID}.bin`);

      // Delete
      await storage.delete(TEST_AGENT_LOOP_ID);
      expect(await storage.load(TEST_AGENT_LOOP_ID)).toBeNull();
    });

    it("should handle minimal agent entity metadata", async () => {
      const metadata = createMinimalAgentEntityMetadata();
      await storage.save(metadata.agentLoopId, createTestData(16), metadata);
      expect(await storage.load(metadata.agentLoopId)).not.toBeNull();
    });
  });

  // ── Status Management ─────────────────────────────────────────────────

  describe("Status Management", () => {
    it("should update status and persist", async () => {
      await storage.save(
        TEST_AGENT_LOOP_ID,
        createTestData(),
        createAgentEntityMetadata({ status: "CREATED" }),
      );

      await storage.updateAgentLoopStatus(TEST_AGENT_LOOP_ID, "RUNNING");
      await storage.updateAgentLoopStatus(TEST_AGENT_LOOP_ID, "COMPLETED");

      // Persist and verify
      await storage.close();
      storage = new JsonAgentLoopStorage({ baseDir: testDir });
      await storage.initialize();

      const meta = await storage.getMetadata(TEST_AGENT_LOOP_ID);
      expect(meta!.status).toBe("COMPLETED");
      expect(meta!.completedAt).toBeDefined();
    });

    it("should list by status", async () => {
      const statuses = ["CREATED", "RUNNING", "COMPLETED", "FAILED"] as const;
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
      for (const count of Object.values(stats.byStatus)) {
        expect(count).toBe(2);
      }
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

    it("should list with status filter", async () => {
      const ids = await storage.list({ status: "RUNNING" });
      expect(ids.length).toBeGreaterThan(0);
    });

    it("should list with profileId filter", async () => {
      const ids = await storage.list({ profileId: "profile-001" });
      expect(ids.length).toBeGreaterThan(0);
    });
  });

  // ── Batch Operations ──────────────────────────────────────────────────

  describe("Batch Operations", () => {
    it("should persist batch across re-initialization", async () => {
      const batch = createAgentEntityBatch(5);
      await storage.saveBatch(batch.map(({ id, data, metadata }) => ({ id, data, metadata })));

      await storage.close();
      storage = new JsonAgentLoopStorage({ baseDir: testDir });
      await storage.initialize();

      expect(await storage.list()).toHaveLength(5);
    });

    it("should delete batch", async () => {
      const batch = createAgentEntityBatch(3);
      await storage.saveBatch(batch.map(({ id, data, metadata }) => ({ id, data, metadata })));

      await storage.deleteBatch(batch.map(({ id }) => id));
      expect(await storage.list()).toHaveLength(0);
    });
  });

  // ── Clear ─────────────────────────────────────────────────────────────

  describe("Clear", () => {
    it("should clear all data and files", async () => {
      await storage.save(TEST_AGENT_LOOP_ID, createTestData(), createAgentEntityMetadata());
      await storage.clear();

      expect(await storage.list()).toHaveLength(0);

      const metaDir = path.join(testDir, "metadata", "agent-loop");
      expect(await fs.readdir(metaDir)).toHaveLength(0);
    });
  });
});