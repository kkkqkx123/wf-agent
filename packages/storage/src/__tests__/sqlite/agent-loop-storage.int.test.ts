/**
 * SqliteAgentLoopStorage Integration Test
 *
 * Tests the complete agent loop storage lifecycle with SQLite backend:
 * - CRUD lifecycle
 * - Status transitions and listByStatus
 * - Agent loop statistics (getAgentLoopStats)
 * - List filtering
 * - Batch operations
 * - Edge cases
 *
 * SQLite database files are created in temp directory and cleaned up after tests.
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as fs from "fs/promises";
import * as path from "path";
import * as os from "os";
import { SqliteAgentLoopStorage } from "../../sqlite/sqlite-agent-loop-storage.js";
import {
  createAgentEntityMetadata,
  createMinimalAgentEntityMetadata,
  createTestData,
  createAgentEntityBatch,
  TEST_AGENT_LOOP_ID,
} from "../common/test-data.js";

describe("SqliteAgentLoopStorage Integration", () => {
  let storage: SqliteAgentLoopStorage;
  let tempDir: string;
  let dbPath: string;

  beforeEach(async () => {
    tempDir = await fs.mkdtemp(path.join(os.tmpdir(), "sqlite-al-int-"));
    dbPath = path.join(tempDir, "test.db");
    storage = new SqliteAgentLoopStorage({ dbPath });
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
      const metadata = createAgentEntityMetadata();

      // Create
      await storage.save(TEST_AGENT_LOOP_ID, data, metadata);

      // Read
      const loaded = await storage.load(TEST_AGENT_LOOP_ID);
      expect(loaded).toEqual(data);
      expect(await storage.exists(TEST_AGENT_LOOP_ID)).toBe(true);

      const meta = await storage.getMetadata(TEST_AGENT_LOOP_ID);
      expect(meta).not.toBeNull();
      expect(meta!.status).toBe("RUNNING");

      // Delete
      await storage.delete(TEST_AGENT_LOOP_ID);
      expect(await storage.load(TEST_AGENT_LOOP_ID)).toBeNull();
      expect(await storage.exists(TEST_AGENT_LOOP_ID)).toBe(false);
    });

    it("should handle minimal agent entity metadata", async () => {
      const metadata = createMinimalAgentEntityMetadata();
      const data = createTestData(16);

      await storage.save(metadata.agentLoopId, data, metadata);
      const loaded = await storage.load(metadata.agentLoopId);
      expect(loaded).toEqual(data);

      const meta = await storage.getMetadata(metadata.agentLoopId);
      expect(meta!.status).toBe("CREATED");
    });

    it("should return null for non-existent agent loop", async () => {
      expect(await storage.load("non-existent")).toBeNull();
      expect(await storage.getMetadata("non-existent")).toBeNull();
    });

    it("should update agent loop status", async () => {
      const metadata = createAgentEntityMetadata();
      await storage.save(TEST_AGENT_LOOP_ID, createTestData(), metadata);

      // Update status by saving with new metadata
      const updatedMetadata = createAgentEntityMetadata({ status: "COMPLETED" });
      await storage.save(TEST_AGENT_LOOP_ID, createTestData(), updatedMetadata);

      const meta = await storage.getMetadata(TEST_AGENT_LOOP_ID);
      expect(meta!.status).toBe("COMPLETED");
    });
  });

  // ── Agent Loop Statistics ─────────────────────────────────────────────

  describe("Agent Loop Statistics", () => {
    it("should return correct agent loop stats", async () => {
      const batch = createAgentEntityBatch(6);
      for (const item of batch) {
        await storage.save(item.id, item.data, item.metadata);
      }

      const stats = await storage.getAgentLoopStats();
      expect(stats.total).toBe(6);
    });

    it("should return zero stats for empty storage", async () => {
      const stats = await storage.getAgentLoopStats();
      expect(stats.total).toBe(0);
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
      const ids = await storage.list({ status: "RUNNING" });
      expect(ids.length).toBeGreaterThan(0);
    });
  });

  // ── List By Status ────────────────────────────────────────────────────

  describe("List By Status", () => {
    it("should list agent loops by status", async () => {
      const metadata = createAgentEntityMetadata({ status: "COMPLETED" });
      await storage.save("completed-1", createTestData(), metadata);

      const ids = await storage.listByStatus("COMPLETED");
      expect(ids).toContain("completed-1");
    });

    it("should return empty array for non-existent status", async () => {
      const ids = await storage.listByStatus("COMPLETED" as any);
      expect(ids).toHaveLength(0);
    });
  });

  // ── Batch Operations ──────────────────────────────────────────────────

  describe("Batch Operations", () => {
    it("should save and load multiple items individually", async () => {
      const batch = createAgentEntityBatch(5);
      for (const item of batch) {
        await storage.save(item.id, item.data, item.metadata);
      }

      for (const item of batch) {
        const loaded = await storage.load(item.id);
        expect(loaded).toEqual(item.data);
      }
    });
  });

  // ── File Persistence ──────────────────────────────────────────────────

  describe("File Persistence", () => {
    it("should persist data across re-initialization", async () => {
      const metadata = createAgentEntityMetadata();
      await storage.save("persist-1", createTestData(), metadata);
      await storage.close();

      storage = new SqliteAgentLoopStorage({ dbPath });
      await storage.initialize();

      const loaded = await storage.load("persist-1");
      expect(loaded).not.toBeNull();
    });
  });
});