/**
 * Integration tests for AgentLoopResourceAPI
 */

import { describe, it, expect, beforeEach } from "vitest";
import { AgentLoopResourceAPI } from "../../api/agent/resources/agent-loop-resource-api.js";
import { AgentLoopFactory } from "../../agent/execution/factories/index.js";
import type { AgentLoopEntity } from "../../agent/entities/agent-loop-entity.js";
import type { AgentLoopStatus } from "@wf-agent/types";
import { isSuccess, getData } from "../../api/shared/types/execution-result.js";

describe("AgentLoopResourceAPI", () => {
  let api: AgentLoopResourceAPI;
  let testEntity: AgentLoopEntity;

  beforeEach(async () => {
    // Create a fresh API instance for each test
    api = new AgentLoopResourceAPI();
    
    // Create a test agent loop entity
    testEntity = await AgentLoopFactory.create({
      profileId: "test-profile",
      maxIterations: 10,
    });
  });

  describe("CRUD Operations", () => {
    it("should create an agent loop entity", async () => {
      const result = await api.create(testEntity);
      
      expect(isSuccess(result)).toBe(true);
      expect(getData(result)).toBeDefined();
    });

    it("should get an agent loop entity by ID", async () => {
      await api.create(testEntity);
      
      const result = await api.get(testEntity.id);
      
      expect(isSuccess(result)).toBe(true);
      expect(getData(result)).toBeDefined();
      expect(getData(result)?.id).toBe(testEntity.id);
    });

    it("should return null for non-existent entity", async () => {
      const result = await api.get("non-existent-id");
      
      expect(isSuccess(result)).toBe(true);
      expect(getData(result)).toBeNull();
    });

    it("should list all agent loop entities", async () => {
      const entity2 = await AgentLoopFactory.create({
        profileId: "test-profile-2",
        maxIterations: 5,
      });
      
      await api.create(testEntity);
      await api.create(entity2);
      
      const result = await api.getAll();
      
      expect(isSuccess(result)).toBe(true);
      expect(getData(result)).toBeDefined();
      expect(getData(result)!.length).toBeGreaterThanOrEqual(2);
    });

    it("should delete an agent loop entity", async () => {
      await api.create(testEntity);
      
      const deleteResult = await api.delete(testEntity.id);
      expect(isSuccess(deleteResult)).toBe(true);
      
      const getResult = await api.get(testEntity.id);
      expect(getData(getResult)).toBeNull();
    });

    it("should throw error when updating entity directly", async () => {
      await api.create(testEntity);
      
      await expect(api.update(testEntity.id, {} as any)).rejects.toThrow(
        "Direct entity update is not supported"
      );
    });
  });

  describe("Filtering", () => {
    it("should filter entities by status", async () => {
      const entity1 = await AgentLoopFactory.create({ profileId: "profile-1" });
      const entity2 = await AgentLoopFactory.create({ profileId: "profile-2" });
      
      await api.create(entity1);
      await api.create(entity2);
      
      const result = await api.getAll({ status: entity1.getStatus() });
      
      expect(isSuccess(result)).toBe(true);
      expect(getData(result)).toBeDefined();
      expect(getData(result)!.length).toBeGreaterThanOrEqual(2);
    });

    it("should filter entities by profile ID", async () => {
      const entity1 = await AgentLoopFactory.create({ profileId: "specific-profile" });
      const entity2 = await AgentLoopFactory.create({ profileId: "other-profile" });
      
      await api.create(entity1);
      await api.create(entity2);
      
      const result = await api.getAll({ profileId: "specific-profile" });
      
      expect(isSuccess(result)).toBe(true);
      expect(getData(result)).toBeDefined();
      expect(getData(result)!.every((e: AgentLoopEntity) => e.config.profileId === "specific-profile")).toBe(true);
    });

    it("should filter entities by creation time range", async () => {
      const entity1 = await AgentLoopFactory.create({ profileId: "profile-1" });
      
      await api.create(entity1);
      
      const now = Date.now();
      const result = await api.getAll({ 
        createdAfter: now - 1000,
        createdBefore: now + 1000 
      });
      
      expect(isSuccess(result)).toBe(true);
      expect(getData(result)).toBeDefined();
      expect(getData(result)!.length).toBeGreaterThanOrEqual(1);
    });
  });

  describe("Status Management", () => {
    it("should update agent loop status", async () => {
      await api.create(testEntity);
      
      await expect(api.updateStatus(testEntity.id, "PAUSED" as AgentLoopStatus)).resolves.not.toThrow();
    });

    it("should list entities by status", async () => {
      await api.create(testEntity);
      
      const result = await api.listByStatus(testEntity.getStatus());
      
      expect(result.length).toBeGreaterThanOrEqual(1);
      expect(result.some(e => e.id === testEntity.id)).toBe(true);
    });
  });

  describe("Summary and Statistics", () => {
    it("should get entity summary", async () => {
      await api.create(testEntity);
      
      const summary = await api.getSummary(testEntity.id);
      
      expect(summary).not.toBeNull();
      expect(summary?.id).toBe(testEntity.id);
      expect(summary?.status).toBe(testEntity.getStatus());
      expect(summary?.profileId).toBe(testEntity.config.profileId);
    });

    it("should return null summary for non-existent entity", async () => {
      const summary = await api.getSummary("non-existent-id");
      
      expect(summary).toBeNull();
    });

    it("should list entity summaries", async () => {
      const entity2 = await AgentLoopFactory.create({ profileId: "profile-2" });
      
      await api.create(testEntity);
      await api.create(entity2);
      
      const summaries = await api.listSummaries();
      
      expect(summaries.length).toBeGreaterThanOrEqual(2);
      expect(summaries[0]).toHaveProperty("id");
      expect(summaries[0]).toHaveProperty("status");
      expect(summaries[0]).toHaveProperty("currentIteration");
    });

    it("should get statistics", async () => {
      await api.create(testEntity);
      
      const stats = await api.getStatistics();
      
      expect(stats.total).toBeGreaterThanOrEqual(1);
      expect(stats.byStatus).toBeDefined();
      expect(typeof stats.byStatus).toBe("object");
    });
  });

  describe("Clear Operations", () => {
    it("should clear all entities", async () => {
      const entity2 = await AgentLoopFactory.create({ profileId: "profile-2" });
      
      await api.create(testEntity);
      await api.create(entity2);
      
      await api.clear();
      
      const result = await api.getAll();
      expect(getData(result)).toEqual([]);
    });
  });
});
