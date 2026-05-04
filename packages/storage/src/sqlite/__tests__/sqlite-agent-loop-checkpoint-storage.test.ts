/**
 * Unit tests for SqliteAgentLoopCheckpointStorage
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import fs from "fs";
import path from "path";
import os from "os";
import { SqliteAgentLoopCheckpointStorage } from "../sqlite-agent-loop-checkpoint-storage.js";
import type { AgentCheckpointMetadata } from "@wf-agent/types";

describe("SqliteAgentLoopCheckpointStorage", () => {
  let tempDir: string;
  let dbPath: string;
  let storage: SqliteAgentLoopCheckpointStorage;

  const createTestMetadata = (agentLoopId: string): AgentCheckpointMetadata => ({
    agentLoopId,
    timestamp: Date.now(),
    type: "FULL",
    version: 1,
    tags: ["test", "checkpoint"],
    customFields: { testKey: "testValue" },
  });

  const createTestData = (size: number = 100): Uint8Array => {
    const data = new Uint8Array(size);
    for (let i = 0; i < size; i++) {
      data[i] = i % 256;
    }
    return data;
  };

  beforeEach(async () => {
    // Create temporary directory for test databases
    tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "sqlite-checkpoint-test-"));
    dbPath = path.join(tempDir, "test.db");
    
    storage = new SqliteAgentLoopCheckpointStorage({
      dbPath,
      useConnectionPool: false, // Use dedicated connection for tests
    });
    
    await storage.initialize();
  });

  afterEach(async () => {
    // Clean up
    await storage.close();
    if (fs.existsSync(tempDir)) {
      fs.rmSync(tempDir, { recursive: true, force: true });
    }
  });

  describe("save and load", () => {
    it("should save and load checkpoint data", async () => {
      const checkpointId = "checkpoint-1";
      const data = createTestData(1000);
      const metadata = createTestMetadata("agent-loop-1");

      await storage.save(checkpointId, data, metadata);
      
      const loaded = await storage.load(checkpointId);
      expect(loaded).toBeDefined();
      expect(loaded!.length).toBe(data.length);
      expect(Buffer.from(loaded!).equals(Buffer.from(data))).toBe(true);
    });

    it("should return null for non-existent checkpoint", async () => {
      const loaded = await storage.load("non-existent");
      expect(loaded).toBeNull();
    });

    it("should update existing checkpoint", async () => {
      const checkpointId = "checkpoint-1";
      const data1 = createTestData(100);
      const data2 = createTestData(200);
      const metadata = createTestMetadata("agent-loop-1");

      await storage.save(checkpointId, data1, metadata);
      await storage.save(checkpointId, data2, metadata);
      
      const loaded = await storage.load(checkpointId);
      expect(loaded!.length).toBe(data2.length);
      expect(Buffer.from(loaded!).equals(Buffer.from(data2))).toBe(true);
    });

    it("should handle large data with compression", async () => {
      const checkpointId = "checkpoint-large";
      // Create compressible data (repeated pattern)
      const data = new Uint8Array(10000);
      for (let i = 0; i < data.length; i++) {
        data[i] = i % 10; // Highly compressible
      }
      const metadata = createTestMetadata("agent-loop-1");

      await storage.save(checkpointId, data, metadata);
      
      const loaded = await storage.load(checkpointId);
      expect(loaded).toBeDefined();
      expect(loaded!.length).toBe(data.length);
      expect(Buffer.from(loaded!).equals(Buffer.from(data))).toBe(true);
    });
  });

  describe("delete", () => {
    it("should delete checkpoint", async () => {
      const checkpointId = "checkpoint-1";
      const data = createTestData(100);
      const metadata = createTestMetadata("agent-loop-1");

      await storage.save(checkpointId, data, metadata);
      await storage.delete(checkpointId);
      
      const loaded = await storage.load(checkpointId);
      expect(loaded).toBeNull();
    });

    it("should handle deleting non-existent checkpoint", async () => {
      // Should not throw
      await expect(storage.delete("non-existent")).resolves.not.toThrow();
    });
  });

  describe("exists", () => {
    it("should return true for existing checkpoint", async () => {
      const checkpointId = "checkpoint-1";
      const data = createTestData(100);
      const metadata = createTestMetadata("agent-loop-1");

      await storage.save(checkpointId, data, metadata);
      
      expect(await storage.exists(checkpointId)).toBe(true);
    });

    it("should return false for non-existent checkpoint", async () => {
      expect(await storage.exists("non-existent")).toBe(false);
    });
  });

  describe("list", () => {
    it("should list all checkpoints", async () => {
      const metadata1 = createTestMetadata("agent-loop-1");
      const metadata2 = createTestMetadata("agent-loop-2");

      await storage.save("cp-1", createTestData(100), metadata1);
      await storage.save("cp-2", createTestData(100), metadata2);
      
      const list = await storage.list();
      expect(list).toHaveLength(2);
      expect(list).toContain("cp-1");
      expect(list).toContain("cp-2");
    });

    it("should filter by agentLoopId", async () => {
      const metadata1 = createTestMetadata("agent-loop-1");
      const metadata2 = createTestMetadata("agent-loop-2");

      await storage.save("cp-1", createTestData(100), metadata1);
      await storage.save("cp-2", createTestData(100), metadata2);
      
      const list = await storage.list({ agentLoopId: "agent-loop-1" });
      expect(list).toHaveLength(1);
      expect(list).toContain("cp-1");
    });

    it("should filter by type", async () => {
      const metadata1 = createTestMetadata("agent-loop-1");
      metadata1.type = "FULL";
      const metadata2 = createTestMetadata("agent-loop-1");
      metadata2.type = "DELTA";

      await storage.save("cp-1", createTestData(100), metadata1);
      await storage.save("cp-2", createTestData(100), metadata2);
      
      const list = await storage.list({ type: "FULL" });
      expect(list).toHaveLength(1);
      expect(list).toContain("cp-1");
    });

    it("should filter by tags", async () => {
      const metadata1 = createTestMetadata("agent-loop-1");
      metadata1.tags = ["tag1", "tag2"];
      const metadata2 = createTestMetadata("agent-loop-1");
      metadata2.tags = ["tag3"];

      await storage.save("cp-1", createTestData(100), metadata1);
      await storage.save("cp-2", createTestData(100), metadata2);
      
      const list = await storage.list({ tags: ["tag1"] });
      expect(list).toHaveLength(1);
      expect(list).toContain("cp-1");
    });

    it("should support pagination with limit", async () => {
      for (let i = 0; i < 5; i++) {
        await storage.save(`cp-${i}`, createTestData(100), createTestMetadata("agent-loop-1"));
      }
      
      const list = await storage.list({ limit: 3 });
      expect(list).toHaveLength(3);
    });

    it("should support pagination with offset", async () => {
      for (let i = 0; i < 5; i++) {
        await storage.save(`cp-${i}`, createTestData(100), createTestMetadata("agent-loop-1"));
      }
      
      const list1 = await storage.list({ limit: 2, offset: 0 });
      const list2 = await storage.list({ limit: 2, offset: 2 });
      
      expect(list1).toHaveLength(2);
      expect(list2).toHaveLength(2);
      expect(list1[0]).not.toBe(list2[0]);
    });

    it("should order by timestamp DESC", async () => {
      const baseTime = Date.now();
      await storage.save("cp-1", createTestData(100), { ...createTestMetadata("agent-loop-1"), timestamp: baseTime - 2000 });
      await storage.save("cp-2", createTestData(100), { ...createTestMetadata("agent-loop-1"), timestamp: baseTime - 1000 });
      await storage.save("cp-3", createTestData(100), { ...createTestMetadata("agent-loop-1"), timestamp: baseTime });
      
      const list = await storage.list({ agentLoopId: "agent-loop-1" });
      expect(list).toEqual(["cp-3", "cp-2", "cp-1"]);
    });
  });

  describe("getMetadata", () => {
    it("should get checkpoint metadata", async () => {
      const checkpointId = "checkpoint-1";
      const metadata = createTestMetadata("agent-loop-1");

      await storage.save(checkpointId, createTestData(100), metadata);
      
      const retrieved = await storage.getMetadata(checkpointId);
      expect(retrieved).toBeDefined();
      expect(retrieved!.agentLoopId).toBe(metadata.agentLoopId);
      expect(retrieved!.timestamp).toBe(metadata.timestamp);
      expect(retrieved!.type).toBe(metadata.type);
      expect(retrieved!.version).toBe(metadata.version);
      expect(retrieved!.tags).toEqual(metadata.tags);
      expect(retrieved!.customFields).toEqual(metadata.customFields);
    });

    it("should return null for non-existent checkpoint", async () => {
      const retrieved = await storage.getMetadata("non-existent");
      expect(retrieved).toBeNull();
    });
  });

  describe("listByAgentLoop", () => {
    it("should list checkpoints for specific agent loop", async () => {
      const metadata1 = createTestMetadata("agent-loop-1");
      const metadata2 = createTestMetadata("agent-loop-2");

      await storage.save("cp-1", createTestData(100), metadata1);
      await storage.save("cp-2", createTestData(100), metadata2);
      
      const list = await storage.listByAgentLoop("agent-loop-1");
      expect(list).toHaveLength(1);
      expect(list).toContain("cp-1");
    });

    it("should support filtering by type", async () => {
      const metadata1 = createTestMetadata("agent-loop-1");
      metadata1.type = "FULL";
      const metadata2 = createTestMetadata("agent-loop-1");
      metadata2.type = "DELTA";

      await storage.save("cp-1", createTestData(100), metadata1);
      await storage.save("cp-2", createTestData(100), metadata2);
      
      const list = await storage.listByAgentLoop("agent-loop-1", { type: "FULL" });
      expect(list).toHaveLength(1);
      expect(list).toContain("cp-1");
    });

    it("should support pagination", async () => {
      for (let i = 0; i < 5; i++) {
        await storage.save(`cp-${i}`, createTestData(100), createTestMetadata("agent-loop-1"));
      }
      
      const list = await storage.listByAgentLoop("agent-loop-1", { limit: 2, offset: 1 });
      expect(list).toHaveLength(2);
    });
  });

  describe("getLatestCheckpoint", () => {
    it("should get latest checkpoint by timestamp", async () => {
      const baseTime = Date.now();
      await storage.save("cp-1", createTestData(100), { ...createTestMetadata("agent-loop-1"), timestamp: baseTime - 2000 });
      await storage.save("cp-2", createTestData(100), { ...createTestMetadata("agent-loop-1"), timestamp: baseTime });
      await storage.save("cp-3", createTestData(100), { ...createTestMetadata("agent-loop-1"), timestamp: baseTime - 1000 });
      
      const latest = await storage.getLatestCheckpoint("agent-loop-1");
      expect(latest).toBe("cp-2");
    });

    it("should return null when no checkpoints exist", async () => {
      const latest = await storage.getLatestCheckpoint("non-existent");
      expect(latest).toBeNull();
    });
  });

  describe("deleteByAgentLoop", () => {
    it("should delete all checkpoints for an agent loop", async () => {
      await storage.save("cp-1", createTestData(100), createTestMetadata("agent-loop-1"));
      await storage.save("cp-2", createTestData(100), createTestMetadata("agent-loop-1"));
      await storage.save("cp-3", createTestData(100), createTestMetadata("agent-loop-2"));
      
      const deleted = await storage.deleteByAgentLoop("agent-loop-1");
      expect(deleted).toBe(2);
      
      const list = await storage.list();
      expect(list).toHaveLength(1);
      expect(list).toContain("cp-3");
    });

    it("should return 0 when no checkpoints to delete", async () => {
      const deleted = await storage.deleteByAgentLoop("non-existent");
      expect(deleted).toBe(0);
    });
  });

  describe("clear", () => {
    it("should clear all checkpoints", async () => {
      await storage.save("cp-1", createTestData(100), createTestMetadata("agent-loop-1"));
      await storage.save("cp-2", createTestData(100), createTestMetadata("agent-loop-2"));
      
      await storage.clear();
      
      const list = await storage.list();
      expect(list).toHaveLength(0);
    });
  });

  describe("compression", () => {
    it("should compress and decompress data correctly", async () => {
      const checkpointId = "checkpoint-compress";
      // Create highly compressible data
      const data = new Uint8Array(5000);
      for (let i = 0; i < data.length; i++) {
        data[i] = 65; // All 'A' characters
      }
      const metadata = createTestMetadata("agent-loop-1");

      await storage.save(checkpointId, data, metadata);
      
      const loaded = await storage.load(checkpointId);
      expect(loaded).toBeDefined();
      expect(loaded!.length).toBe(data.length);
      expect(Buffer.from(loaded!).equals(Buffer.from(data))).toBe(true);
    });

    it("should handle small data without compression", async () => {
      const checkpointId = "checkpoint-small";
      const data = createTestData(50); // Below compression threshold
      const metadata = createTestMetadata("agent-loop-1");

      await storage.save(checkpointId, data, metadata);
      
      const loaded = await storage.load(checkpointId);
      expect(loaded).toBeDefined();
      expect(Buffer.from(loaded!).equals(Buffer.from(data))).toBe(true);
    });
  });

  describe("error handling", () => {
    it("should handle database errors gracefully", async () => {
      // Close storage to simulate error state
      await storage.close();
      
      await expect(storage.save("cp-1", createTestData(100), createTestMetadata("agent-loop-1"))).rejects.toThrow();
    });
  });
});
