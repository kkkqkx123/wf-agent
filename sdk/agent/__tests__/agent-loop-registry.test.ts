/**
 * Agent Loop Registry Integration Tests
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { AgentLoopRegistry } from "../stores/agent-loop-registry.js";
import { AgentLoopEntity } from "../entities/agent-loop-entity.js";
import type { AgentLoopRuntimeConfig } from "@wf-agent/types";
import { AgentLoopStatus } from "@wf-agent/types";

describe("Agent Loop Registry", () => {
  let registry: AgentLoopRegistry;
  let entity1: AgentLoopEntity;
  let entity2: AgentLoopEntity;

  const config: AgentLoopRuntimeConfig = {
    maxIterations: 10,
    profileId: "test-profile",
  };

  beforeEach(() => {
    registry = new AgentLoopRegistry();
    entity1 = new AgentLoopEntity("entity-1", config);
    entity2 = new AgentLoopEntity("entity-2", config);
  });

  afterEach(() => {
    registry.clear();
  });

  describe("Instance Management", () => {
    it("should register entity", () => {
      registry.register(entity1);
      expect(registry.has("entity-1")).toBe(true);
    });

    it("should unregister entity", () => {
      registry.register(entity1);
      const result = registry.unregister("entity-1");
      expect(result).toBe(true);
      expect(registry.has("entity-1")).toBe(false);
    });

    it("should get entity", () => {
      registry.register(entity1);
      const entity = registry.get("entity-1");
      expect(entity).toBe(entity1);
    });

    it("should return undefined for non-existent entity", () => {
      const entity = registry.get("non-existent");
      expect(entity).toBeUndefined();
    });

    it("should check if entity exists", () => {
      registry.register(entity1);
      expect(registry.has("entity-1")).toBe(true);
      expect(registry.has("entity-2")).toBe(false);
    });

    it("should get all entities", () => {
      registry.register(entity1);
      registry.register(entity2);
      const all = registry.getAll();
      expect(all.length).toBe(2);
    });

    it("should get all IDs", () => {
      registry.register(entity1);
      registry.register(entity2);
      const ids = registry.getAllIds();
      expect(ids.length).toBe(2);
      expect(ids).toContain("entity-1");
      expect(ids).toContain("entity-2");
    });

    it("should return correct size", () => {
      expect(registry.size()).toBe(0);
      registry.register(entity1);
      expect(registry.size()).toBe(1);
      registry.register(entity2);
      expect(registry.size()).toBe(2);
    });
  });

  describe("Status Filtering", () => {
    it("should get entities by status", () => {
      entity1.state.start();
      entity2.state.start();
      entity2.state.pause();

      registry.register(entity1);
      registry.register(entity2);

      const running = registry.getByStatus(AgentLoopStatus.RUNNING);
      expect(running.length).toBe(1);
      expect(running[0]).toBe(entity1);
    });

    it("should get running entities", () => {
      entity1.state.start();
      entity2.state.start();

      registry.register(entity1);
      registry.register(entity2);

      const running = registry.getRunning();
      expect(running.length).toBe(2);
    });

    it("should get paused entities", () => {
      entity1.state.start();
      entity1.state.pause();
      entity2.state.start();

      registry.register(entity1);
      registry.register(entity2);

      const paused = registry.getPaused();
      expect(paused.length).toBe(1);
      expect(paused[0]).toBe(entity1);
    });

    it("should get completed entities", () => {
      entity1.state.start();
      entity1.state.complete();
      entity2.state.start();

      registry.register(entity1);
      registry.register(entity2);

      const completed = registry.getCompleted();
      expect(completed.length).toBe(1);
    });

    it("should get failed entities", () => {
      entity1.state.start();
      entity1.state.fail(new Error("Test"));
      entity2.state.start();

      registry.register(entity1);
      registry.register(entity2);

      const failed = registry.getFailed();
      expect(failed.length).toBe(1);
    });
  });

  describe("Cleanup", () => {
    it("should cleanup completed entities", () => {
      entity1.state.start();
      entity1.state.complete();
      entity2.state.start();

      registry.register(entity1);
      registry.register(entity2);

      const count = registry.cleanupCompleted();
      expect(count).toBe(1);
      expect(registry.size()).toBe(1);
    });

    it("should clear all entities", () => {
      registry.register(entity1);
      registry.register(entity2);

      registry.clear();
      expect(registry.size()).toBe(0);
    });

    it("should call cleanup on entities", () => {
      registry.register(entity1);
      registry.register(entity2);

      registry.cleanup();
      expect(registry.size()).toBe(0);
    });
  });
});
