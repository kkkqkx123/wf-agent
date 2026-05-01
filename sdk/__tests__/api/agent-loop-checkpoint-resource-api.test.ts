/**
 * Integration tests for AgentLoopCheckpointResourceAPI
 */

import { describe, it, expect, beforeEach } from "vitest";
import { AgentLoopCheckpointResourceAPI } from "../../api/agent/resources/checkpoint-resource-api.js";
import { AgentLoopFactory } from "../../agent/execution/factories/index.js";
import type { AgentLoopEntity } from "../../agent/entities/agent-loop-entity.js";
import type { AgentLoopCheckpoint } from "@wf-agent/types";
import { isSuccess, getData } from "../../api/shared/types/execution-result.js";

describe("AgentLoopCheckpointResourceAPI", () => {
  let api: AgentLoopCheckpointResourceAPI;
  let testEntity: AgentLoopEntity;

  beforeEach(async () => {
    // Create a fresh API instance for each test
    api = new AgentLoopCheckpointResourceAPI();
    
    // Create a test agent loop entity
    testEntity = await AgentLoopFactory.create({
      profileId: "test-profile",
      maxIterations: 10,
    });
  });

  describe("CRUD Operations", () => {
    it("should create a checkpoint", async () => {
      const checkpointId = await api.createCheckpoint(testEntity);
      
      expect(checkpointId).toBeDefined();
      expect(typeof checkpointId).toBe("string");
    });

    it("should get a checkpoint by ID", async () => {
      const checkpointId = await api.createCheckpoint(testEntity);
      
      const result = await api.get(checkpointId);
      
      expect(isSuccess(result)).toBe(true);
      expect(getData(result)).toBeDefined();
      expect(getData(result)?.id).toBe(checkpointId);
    });

    it("should return null for non-existent checkpoint", async () => {
      const result = await api.get("non-existent-id");
      
      expect(isSuccess(result)).toBe(true);
      expect(getData(result)).toBeNull();
    });

    it("should list all checkpoints", async () => {
      await api.createCheckpoint(testEntity);
      await api.createCheckpoint(testEntity);
      
      const result = await api.getAll();
      
      expect(isSuccess(result)).toBe(true);
      expect(getData(result)).toBeDefined();
      expect(getData(result)!.length).toBeGreaterThanOrEqual(2);
    });

    it("should delete a checkpoint", async () => {
      const checkpointId = await api.createCheckpoint(testEntity);
      
      const deleteResult = await api.delete(checkpointId);
      expect(isSuccess(deleteResult)).toBe(true);
      
      const getResult = await api.get(checkpointId);
      expect(getData(getResult)).toBeNull();
    });

    it("should throw error when updating checkpoint", async () => {
      const checkpointId = await api.createCheckpoint(testEntity);
      
      await expect(api.update(checkpointId, {} as any)).rejects.toThrow(
        "Checkpoint update via API is not supported"
      );
    });
  });

  describe("Filtering", () => {
    it("should filter checkpoints by agent loop ID", async () => {
      const checkpointId = await api.createCheckpoint(testEntity);
      
      const result = await api.getAll({ agentLoopId: testEntity.id });
      
      expect(isSuccess(result)).toBe(true);
      expect(getData(result)).toBeDefined();
      expect(getData(result)!.length).toBeGreaterThanOrEqual(1);
      expect(getData(result)!.every((cp: AgentLoopCheckpoint) => cp.agentLoopId === testEntity.id)).toBe(true);
    });

    it("should filter checkpoints by type", async () => {
      await api.createCheckpoint(testEntity);
      
      const result = await api.getAll({ type: "FULL" });
      
      expect(isSuccess(result)).toBe(true);
      expect(getData(result)).toBeDefined();
      // All checkpoints created should be FULL type by default
      expect(getData(result)!.every((cp: AgentLoopCheckpoint) => cp.type === "FULL")).toBe(true);
    });

    it("should filter checkpoints by timestamp range", async () => {
      await api.createCheckpoint(testEntity);
      
      const now = Date.now();
      const result = await api.getAll({ 
        timestampRange: { 
          start: now - 1000,
          end: now + 1000 
        } 
      });
      
      expect(isSuccess(result)).toBe(true);
      expect(getData(result)).toBeDefined();
      expect(getData(result)!.length).toBeGreaterThanOrEqual(1);
    });
  });

  describe("Agent Loop Checkpoint Operations", () => {
    it("should get all checkpoints for an agent loop", async () => {
      await api.createCheckpoint(testEntity);
      await api.createCheckpoint(testEntity);
      
      const checkpoints = await api.getAgentLoopCheckpoints(testEntity.id);
      
      expect(checkpoints.length).toBeGreaterThanOrEqual(2);
      expect(checkpoints.every(cp => cp.agentLoopId === testEntity.id)).toBe(true);
    });

    it("should get the latest checkpoint for an agent loop", async () => {
      const checkpointId1 = await api.createCheckpoint(testEntity);
      await new Promise(resolve => setTimeout(resolve, 10));
      const checkpointId2 = await api.createCheckpoint(testEntity);
      
      const latest = await api.getLatestCheckpoint(testEntity.id);
      
      expect(latest).not.toBeNull();
      expect(latest?.id).toBe(checkpointId2);
    });

    it("should return null for latest checkpoint when none exist", async () => {
      const latest = await api.getLatestCheckpoint("non-existent-id");
      
      expect(latest).toBeNull();
    });

    it("should delete all checkpoints for an agent loop", async () => {
      await api.createCheckpoint(testEntity);
      await api.createCheckpoint(testEntity);
      
      const count = await api.deleteAgentLoopCheckpoints(testEntity.id);
      
      expect(count).toBeGreaterThanOrEqual(2);
      
      const remaining = await api.getAgentLoopCheckpoints(testEntity.id);
      expect(remaining.length).toBe(0);
    });
  });

  describe("Checkpoint Chain", () => {
    it("should get checkpoint chain", async () => {
      const checkpointId = await api.createCheckpoint(testEntity);
      
      const chain = await api.getCheckpointChain(checkpointId);
      
      expect(chain.length).toBeGreaterThanOrEqual(1);
      expect(chain[0]?.id).toBe(checkpointId);
    });

    it("should handle empty chain for non-existent checkpoint", async () => {
      const chain = await api.getCheckpointChain("non-existent-id");
      
      expect(chain.length).toBe(0);
    });
  });

  describe("Statistics", () => {
    it("should get checkpoint statistics", async () => {
      await api.createCheckpoint(testEntity);
      await api.createCheckpoint(testEntity);
      
      const stats = await api.getCheckpointStatistics();
      
      expect(stats.total).toBeGreaterThanOrEqual(2);
      expect(stats.byAgentLoop).toBeDefined();
      expect(stats.byType).toBeDefined();
      expect(stats.byAgentLoop[testEntity.id]).toBeGreaterThanOrEqual(2);
    });
  });

  describe("Restore from Checkpoint", () => {
    it("should restore agent loop from checkpoint", async () => {
      // Add some messages to the entity before creating checkpoint
      testEntity.addMessage({ role: "user", content: "Test message" });
      
      const checkpointId = await api.createCheckpoint(testEntity);
      
      const restoredEntity = await api.restoreFromCheckpoint(checkpointId);
      
      expect(restoredEntity).toBeDefined();
      expect(restoredEntity.id).toBeDefined();
      // Note: The restored entity will have a new ID
      expect(restoredEntity.getMessages().length).toBeGreaterThanOrEqual(1);
    });
  });

  describe("Clear Operations", () => {
    it("should clear all checkpoints", async () => {
      await api.createCheckpoint(testEntity);
      await api.createCheckpoint(testEntity);
      
      await api.clear();
      
      const result = await api.getAll();
      expect(getData(result)).toEqual([]);
    });
  });
});
