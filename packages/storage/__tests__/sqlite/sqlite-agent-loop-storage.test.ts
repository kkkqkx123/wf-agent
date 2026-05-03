/**
 * Unit tests for SqliteAgentLoopStorage
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import fs from "fs";
import path from "path";
import os from "os";
import { SqliteAgentLoopStorage } from "../../src/sqlite/sqlite-agent-loop-storage.js";
import { AgentLoopStatus, type AgentEntityMetadata } from "@wf-agent/types";

describe("SqliteAgentLoopStorage", () => {
  let tempDir: string;
  let dbPath: string;
  let storage: SqliteAgentLoopStorage;

  const createTestMetadata = (agentLoopId: string, status: AgentLoopStatus = AgentLoopStatus.RUNNING): AgentEntityMetadata => ({
    agentLoopId,
    status: status,
    createdAt: Date.now(),
    updatedAt: Date.now(),
    profileId: `profile-${agentLoopId}`,
    tags: ["test", "agent"],
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
    tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "sqlite-agent-loop-test-"));
    dbPath = path.join(tempDir, "test.db");
    
    storage = new SqliteAgentLoopStorage({
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
    it("should save and load agent loop data", async () => {
      const agentLoopId = "agent-loop-1";
      const data = createTestData(1000);
      const metadata = createTestMetadata(agentLoopId);

      await storage.save(agentLoopId, data, metadata);
      
      const loaded = await storage.load(agentLoopId);
      expect(loaded).toBeDefined();
      expect(loaded!.length).toBe(data.length);
      expect(Buffer.from(loaded!).equals(Buffer.from(data))).toBe(true);
    });

    it("should return null for non-existent agent loop", async () => {
      const loaded = await storage.load("non-existent");
      expect(loaded).toBeNull();
    });

    it("should update existing agent loop", async () => {
      const agentLoopId = "agent-loop-1";
      const data1 = createTestData(100);
      const data2 = createTestData(200);
      const metadata = createTestMetadata(agentLoopId);

      await storage.save(agentLoopId, data1, metadata);
      await storage.save(agentLoopId, data2, metadata);
      
      const loaded = await storage.load(agentLoopId);
      expect(loaded!.length).toBe(data2.length);
      expect(Buffer.from(loaded!).equals(Buffer.from(data2))).toBe(true);
    });

    it("should handle large data with compression", async () => {
      const agentLoopId = "agent-loop-large";
      // Create compressible data (repeated pattern)
      const data = new Uint8Array(10000);
      for (let i = 0; i < data.length; i++) {
        data[i] = i % 10; // Highly compressible
      }
      const metadata = createTestMetadata(agentLoopId);

      await storage.save(agentLoopId, data, metadata);
      
      const loaded = await storage.load(agentLoopId);
      expect(loaded).toBeDefined();
      expect(loaded!.length).toBe(data.length);
      expect(Buffer.from(loaded!).equals(Buffer.from(data))).toBe(true);
    });
  });

  describe("delete", () => {
    it("should delete agent loop", async () => {
      const agentLoopId = "agent-loop-1";
      const data = createTestData(100);
      const metadata = createTestMetadata(agentLoopId);

      await storage.save(agentLoopId, data, metadata);
      await storage.delete(agentLoopId);
      
      const loaded = await storage.load(agentLoopId);
      expect(loaded).toBeNull();
    });

    it("should handle deleting non-existent agent loop", async () => {
      // Should not throw
      await expect(storage.delete("non-existent")).resolves.not.toThrow();
    });
  });

  describe("exists", () => {
    it("should return true for existing agent loop", async () => {
      const agentLoopId = "agent-loop-1";
      const data = createTestData(100);
      const metadata = createTestMetadata(agentLoopId);

      await storage.save(agentLoopId, data, metadata);
      
      expect(await storage.exists(agentLoopId)).toBe(true);
    });

    it("should return false for non-existent agent loop", async () => {
      expect(await storage.exists("non-existent")).toBe(false);
    });
  });

  describe("list", () => {
    it("should list all agent loops", async () => {
      const metadata1 = createTestMetadata("agent-loop-1");
      const metadata2 = createTestMetadata("agent-loop-2");

      await storage.save("agent-loop-1", createTestData(100), metadata1);
      await storage.save("agent-loop-2", createTestData(100), metadata2);
      
      const list = await storage.list();
      expect(list).toHaveLength(2);
      expect(list).toContain("agent-loop-1");
      expect(list).toContain("agent-loop-2");
    });

    it("should filter by status", async () => {
      const metadata1 = createTestMetadata("agent-loop-1", AgentLoopStatus.RUNNING);
      const metadata2 = createTestMetadata("agent-loop-2", AgentLoopStatus.COMPLETED);

      await storage.save("agent-loop-1", createTestData(100), metadata1);
      await storage.save("agent-loop-2", createTestData(100), metadata2);
      
      const list = await storage.list({ status: AgentLoopStatus.RUNNING });
      expect(list).toHaveLength(1);
      expect(list).toContain("agent-loop-1");
    });

    it("should filter by profileId", async () => {
      const metadata1 = createTestMetadata("agent-loop-1");
      metadata1.profileId = "profile-1";
      const metadata2 = createTestMetadata("agent-loop-2");
      metadata2.profileId = "profile-2";

      await storage.save("agent-loop-1", createTestData(100), metadata1);
      await storage.save("agent-loop-2", createTestData(100), metadata2);
      
      const list = await storage.list({ profileId: "profile-1" });
      expect(list).toHaveLength(1);
      expect(list).toContain("agent-loop-1");
    });

    it("should filter by tags", async () => {
      const metadata1 = createTestMetadata("agent-loop-1");
      metadata1.tags = ["tag1", "tag2"];
      const metadata2 = createTestMetadata("agent-loop-2");
      metadata2.tags = ["tag3"];

      await storage.save("agent-loop-1", createTestData(100), metadata1);
      await storage.save("agent-loop-2", createTestData(100), metadata2);
      
      const list = await storage.list({ tags: ["tag1"] });
      expect(list).toHaveLength(1);
      expect(list).toContain("agent-loop-1");
    });

    it("should filter by createdAfter", async () => {
      const baseTime = Date.now();
      const metadata1 = createTestMetadata("agent-loop-1");
      metadata1.createdAt = baseTime - 2000;
      const metadata2 = createTestMetadata("agent-loop-2");
      metadata2.createdAt = baseTime + 1000;

      await storage.save("agent-loop-1", createTestData(100), metadata1);
      await storage.save("agent-loop-2", createTestData(100), metadata2);
      
      const list = await storage.list({ createdAfter: baseTime });
      expect(list).toHaveLength(1);
      expect(list).toContain("agent-loop-2");
    });

    it("should filter by createdBefore", async () => {
      const baseTime = Date.now();
      const metadata1 = createTestMetadata("agent-loop-1");
      metadata1.createdAt = baseTime - 2000;
      const metadata2 = createTestMetadata("agent-loop-2");
      metadata2.createdAt = baseTime + 1000;

      await storage.save("agent-loop-1", createTestData(100), metadata1);
      await storage.save("agent-loop-2", createTestData(100), metadata2);
      
      const list = await storage.list({ createdBefore: baseTime });
      expect(list).toHaveLength(1);
      expect(list).toContain("agent-loop-1");
    });

    it("should support pagination with limit", async () => {
      for (let i = 0; i < 5; i++) {
        await storage.save(`agent-loop-${i}`, createTestData(100), createTestMetadata(`agent-loop-${i}`));
      }
      
      const list = await storage.list({ limit: 3 });
      expect(list).toHaveLength(3);
    });

    it("should support pagination with offset", async () => {
      for (let i = 0; i < 5; i++) {
        await storage.save(`agent-loop-${i}`, createTestData(100), createTestMetadata(`agent-loop-${i}`));
      }
      
      const list1 = await storage.list({ limit: 2, offset: 0 });
      const list2 = await storage.list({ limit: 2, offset: 2 });
      
      expect(list1).toHaveLength(2);
      expect(list2).toHaveLength(2);
      expect(list1[0]).not.toBe(list2[0]);
    });

    it("should order by created_at DESC", async () => {
      const baseTime = Date.now();
      await storage.save("agent-loop-1", createTestData(100), { ...createTestMetadata("agent-loop-1"), createdAt: baseTime - 2000 });
      await storage.save("agent-loop-2", createTestData(100), { ...createTestMetadata("agent-loop-2"), createdAt: baseTime - 1000 });
      await storage.save("agent-loop-3", createTestData(100), { ...createTestMetadata("agent-loop-3"), createdAt: baseTime });
      
      const list = await storage.list();
      expect(list).toEqual(["agent-loop-3", "agent-loop-2", "agent-loop-1"]);
    });
  });

  describe("getMetadata", () => {
    it("should get agent loop metadata", async () => {
      const agentLoopId = "agent-loop-1";
      const metadata = createTestMetadata(agentLoopId);

      await storage.save(agentLoopId, createTestData(100), metadata);
      
      const retrieved = await storage.getMetadata(agentLoopId);
      expect(retrieved).toBeDefined();
      expect(retrieved!.agentLoopId).toBe(metadata.agentLoopId);
      expect(retrieved!.status).toBe(metadata.status);
      expect(retrieved!.createdAt).toBe(metadata.createdAt);
      expect(retrieved!.updatedAt).toBe(metadata.updatedAt);
      expect(retrieved!.profileId).toBe(metadata.profileId);
      expect(retrieved!.tags).toEqual(metadata.tags);
      expect(retrieved!.customFields).toEqual(metadata.customFields);
    });

    it("should return null for non-existent agent loop", async () => {
      const retrieved = await storage.getMetadata("non-existent");
      expect(retrieved).toBeNull();
    });
  });

  describe("updateAgentLoopStatus", () => {
    it("should update agent loop status", async () => {
      const agentLoopId = "agent-loop-1";
      const metadata = createTestMetadata(agentLoopId, AgentLoopStatus.RUNNING);

      await storage.save(agentLoopId, createTestData(100), metadata);
      await storage.updateAgentLoopStatus(agentLoopId, AgentLoopStatus.COMPLETED);
      
      const retrieved = await storage.getMetadata(agentLoopId);
      expect(retrieved!.status).toBe(AgentLoopStatus.COMPLETED);
      expect(retrieved!.completedAt).toBeDefined();
    });

    it("should set completedAt for terminal states", async () => {
      const agentLoopId = "agent-loop-1";
      const metadata = createTestMetadata(agentLoopId, AgentLoopStatus.RUNNING);

      await storage.save(agentLoopId, createTestData(100), metadata);
      
      const beforeUpdate = await storage.getMetadata(agentLoopId);
      expect(beforeUpdate!.completedAt).toBeUndefined();
      
      await storage.updateAgentLoopStatus(agentLoopId, AgentLoopStatus.FAILED);
      
      const afterUpdate = await storage.getMetadata(agentLoopId);
      expect(afterUpdate!.status).toBe(AgentLoopStatus.FAILED);
      expect(afterUpdate!.completedAt).toBeDefined();
    });

    it("should not set completedAt for non-terminal states", async () => {
      const agentLoopId = "agent-loop-1";
      const metadata = createTestMetadata(agentLoopId, AgentLoopStatus.RUNNING);

      await storage.save(agentLoopId, createTestData(100), metadata);
      await storage.updateAgentLoopStatus(agentLoopId, AgentLoopStatus.RUNNING);
      
      const retrieved = await storage.getMetadata(agentLoopId);
      expect(retrieved!.status).toBe(AgentLoopStatus.RUNNING);
      expect(retrieved!.completedAt).toBeUndefined();
    });

    it("should throw error when updating non-existent agent loop", async () => {
      await expect(storage.updateAgentLoopStatus("non-existent", AgentLoopStatus.COMPLETED)).rejects.toThrow(
        "Agent loop not found"
      );
    });

    it("should update updatedAt timestamp", async () => {
      const agentLoopId = "agent-loop-1";
      const metadata = createTestMetadata(agentLoopId);

      await storage.save(agentLoopId, createTestData(100), metadata);
      
      const beforeUpdate = await storage.getMetadata(agentLoopId);
      const originalUpdatedAt = beforeUpdate!.updatedAt;
      
      // Wait a bit to ensure timestamp changes
      await new Promise(resolve => setTimeout(resolve, 10));
      
      await storage.updateAgentLoopStatus(agentLoopId, AgentLoopStatus.RUNNING);
      
      const afterUpdate = await storage.getMetadata(agentLoopId);
      expect(afterUpdate!.updatedAt).toBeGreaterThan(originalUpdatedAt!);
    });
  });

  describe("listByStatus", () => {
    it("should list agent loops by status", async () => {
      const metadata1 = createTestMetadata("agent-loop-1", AgentLoopStatus.RUNNING);
      const metadata2 = createTestMetadata("agent-loop-2", AgentLoopStatus.COMPLETED);
      const metadata3 = createTestMetadata("agent-loop-3", AgentLoopStatus.RUNNING);

      await storage.save("agent-loop-1", createTestData(100), metadata1);
      await storage.save("agent-loop-2", createTestData(100), metadata2);
      await storage.save("agent-loop-3", createTestData(100), metadata3);
      
      const running = await storage.listByStatus(AgentLoopStatus.RUNNING);
      expect(running).toHaveLength(2);
      expect(running).toContain("agent-loop-1");
      expect(running).toContain("agent-loop-3");
      
      const completed = await storage.listByStatus(AgentLoopStatus.COMPLETED);
      expect(completed).toHaveLength(1);
      expect(completed).toContain("agent-loop-2");
    });

    it("should return empty array when no agent loops match status", async () => {
      const list = await storage.listByStatus(AgentLoopStatus.COMPLETED);
      expect(list).toHaveLength(0);
    });
  });

  describe("getAgentLoopStats", () => {
    it("should return correct statistics", async () => {
      const metadata1 = createTestMetadata("agent-loop-1", AgentLoopStatus.RUNNING);
      const metadata2 = createTestMetadata("agent-loop-2", AgentLoopStatus.COMPLETED);
      const metadata3 = createTestMetadata("agent-loop-3", AgentLoopStatus.RUNNING);

      await storage.save("agent-loop-1", createTestData(100), metadata1);
      await storage.save("agent-loop-2", createTestData(100), metadata2);
      await storage.save("agent-loop-3", createTestData(100), metadata3);
      
      const stats = await storage.getAgentLoopStats();
      
      expect(stats.total).toBe(3);
      expect(stats.byStatus["RUNNING"]).toBe(2);
      expect(stats.byStatus["COMPLETED"]).toBe(1);
    });

    it("should return zero stats when no agent loops exist", async () => {
      const stats = await storage.getAgentLoopStats();
      
      expect(stats.total).toBe(0);
      expect(Object.keys(stats.byStatus)).toHaveLength(0);
    });
  });

  describe("clear", () => {
    it("should clear all agent loops", async () => {
      await storage.save("agent-loop-1", createTestData(100), createTestMetadata("agent-loop-1"));
      await storage.save("agent-loop-2", createTestData(100), createTestMetadata("agent-loop-2"));
      
      await storage.clear();
      
      const list = await storage.list();
      expect(list).toHaveLength(0);
    });
  });

  describe("compression", () => {
    it("should compress and decompress data correctly", async () => {
      const agentLoopId = "agent-loop-compress";
      // Create highly compressible data
      const data = new Uint8Array(5000);
      for (let i = 0; i < data.length; i++) {
        data[i] = 65; // All 'A' characters
      }
      const metadata = createTestMetadata(agentLoopId);

      await storage.save(agentLoopId, data, metadata);
      
      const loaded = await storage.load(agentLoopId);
      expect(loaded).toBeDefined();
      expect(loaded!.length).toBe(data.length);
      expect(Buffer.from(loaded!).equals(Buffer.from(data))).toBe(true);
    });

    it("should handle small data without compression", async () => {
      const agentLoopId = "agent-loop-small";
      const data = createTestData(50); // Below compression threshold
      const metadata = createTestMetadata(agentLoopId);

      await storage.save(agentLoopId, data, metadata);
      
      const loaded = await storage.load(agentLoopId);
      expect(loaded).toBeDefined();
      expect(Buffer.from(loaded!).equals(Buffer.from(data))).toBe(true);
    });
  });

  describe("error handling", () => {
    it("should handle database errors gracefully", async () => {
      // Close storage to simulate error state
      await storage.close();
      
      await expect(storage.save("agent-loop-1", createTestData(100), createTestMetadata("agent-loop-1"))).rejects.toThrow();
    });
  });
});
