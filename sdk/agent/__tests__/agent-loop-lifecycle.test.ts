/**
 * Agent Loop Lifecycle Integration Tests
 */

import { describe, it, expect, beforeEach } from "vitest";
import { AgentLoopEntity } from "../entities/agent-loop-entity.js";
import {
  createAgentLoopCheckpoint,
  cleanupAgentLoop,
  cloneAgentLoop,
} from "../execution/handlers/agent-loop-lifecycle.js";
import type { AgentLoopConfig, AgentLoopCheckpoint } from "@wf-agent/types";

describe("Agent Loop Lifecycle", () => {
  let entity: AgentLoopEntity;
  const config: AgentLoopConfig = {
    maxIterations: 10,
    profileId: "test-profile",
  };

  const mockDependencies = {
    saveCheckpoint: async (checkpoint: unknown) => "mock-checkpoint-id",
    getCheckpoint: async () => null,
    listCheckpoints: async () => [],
  };

  beforeEach(() => {
    entity = new AgentLoopEntity("test-id", config);
  });

  describe("Checkpoint Creation", () => {
    it("should create checkpoint", async () => {
      entity.state.start();

      const checkpointId = await createAgentLoopCheckpoint(entity, mockDependencies);

      expect(checkpointId).toBeDefined();
    });

    it("should return checkpoint ID", async () => {
      entity.state.start();

      const checkpointId = await createAgentLoopCheckpoint(entity, mockDependencies);

      expect(typeof checkpointId).toBe("string");
    });

    it("should handle errors", async () => {
      const failingDependencies = {
        ...mockDependencies,
        saveCheckpoint: async () => {
          throw new Error("Save failed");
        },
      };

      entity.state.start();

      await expect(createAgentLoopCheckpoint(entity, failingDependencies)).rejects.toThrow();
    });
  });

  describe("Resource Cleanup", () => {
    it("should cleanup state", () => {
      entity.state.start();
      entity.state.startIteration();

      cleanupAgentLoop(entity);

      expect(entity.state.iterationHistory.length).toBe(0);
    });

    it("should cleanup message history", () => {
      entity.addMessage({ role: "user", content: "Test" });

      cleanupAgentLoop(entity);

      expect(entity.getMessages().length).toBe(0);
    });

    it("should cleanup variable state", () => {
      entity.setVariable("key1", "value1");

      cleanupAgentLoop(entity);

      expect(entity.getVariable("key1")).toBeUndefined();
    });

    it("should clear abort controller", () => {
      entity.abortController = new AbortController();

      cleanupAgentLoop(entity);

      expect(entity.abortController).toBeUndefined();
    });
  });

  describe("Instance Cloning", () => {
    it("should clone entity", () => {
      entity.state.start();

      const cloned = cloneAgentLoop(entity);

      expect(cloned).toBeDefined();
      expect(cloned.id).toBe(entity.id);
    });

    it("should create independent state", () => {
      entity.state.start();
      entity.state.startIteration();

      const cloned = cloneAgentLoop(entity);

      cloned.state.startIteration();

      expect(entity.state.currentIteration).toBe(1);
      expect(cloned.state.currentIteration).toBe(2);
    });

    it("should clone message history", () => {
      entity.addMessage({ role: "user", content: "Test" });

      const cloned = cloneAgentLoop(entity);

      cloned.addMessage({ role: "assistant", content: "Response" });

      expect(entity.getMessages().length).toBe(1);
      expect(cloned.getMessages().length).toBe(2);
    });

    it("should clone variable state", () => {
      entity.setVariable("key1", "value1");

      const cloned = cloneAgentLoop(entity);

      cloned.setVariable("key1", "value2");

      expect(entity.getVariable("key1")).toBe("value1");
      expect(cloned.getVariable("key1")).toBe("value2");
    });

    it("should preserve parent context", () => {
      entity.parentExecutionId = "wfexec-123";
      entity.nodeId = "node-456";

      const cloned = cloneAgentLoop(entity);

      expect(cloned.parentExecutionId).toBe("wfexec-123");
      expect(cloned.nodeId).toBe("node-456");
    });
  });
});
