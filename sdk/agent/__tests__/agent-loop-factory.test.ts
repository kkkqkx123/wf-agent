/**
 * Agent Loop Factory Integration Tests
 */

import { describe, it, expect, beforeEach } from "vitest";
import { AgentLoopFactory } from "../execution/factories/agent-loop-factory.js";
import type { AgentLoopRuntimeConfig, LLMMessage } from "@wf-agent/types";

describe("Agent Loop Factory", () => {
  const basicConfig: AgentLoopRuntimeConfig = {
    maxIterations: 10,
    profileId: "test-profile",
  };

  describe("Create New Instance", () => {
    it("should create basic agent loop entity", async () => {
      const entity = await AgentLoopFactory.create(basicConfig);

      expect(entity).toBeDefined();
      expect(entity.id).toBeDefined();
      expect(entity.config).toEqual(basicConfig);
    });

    it("should initialize message history", async () => {
      const initialMessages: LLMMessage[] = [{ role: "user", content: "Hello" }];

      const entity = await AgentLoopFactory.create(basicConfig, { initialMessages });

      expect(entity.getMessages().length).toBe(1);
    });

    it("should initialize variable state", async () => {
      const initialVariables = { key1: "value1", key2: "value2" };

      const entity = await AgentLoopFactory.create(basicConfig, { initialVariables });

      expect(entity.getVariable("key1")).toBe("value1");
      expect(entity.getVariable("key2")).toBe("value2");
    });

    it("should set parent context", async () => {
      const entity = await AgentLoopFactory.create(basicConfig, {
        parentExecutionId: "wfexec-123",
        nodeId: "node-456",
      });

      expect(entity.parentExecutionId).toBe("wfexec-123");
      expect(entity.nodeId).toBe("node-456");
    });
  });

  describe("From Checkpoint", () => {
    it("should restore from checkpoint", async () => {
      const mockDependencies = {
        saveCheckpoint: async () => "checkpoint-id",
        getCheckpoint: async () => ({
          id: "checkpoint-id",
          agentLoopId: "test",
          timestamp: Date.now(),
          type: "FULL",
          snapshot: {
            status: "CREATED",
            currentIteration: 0,
            toolCallCount: 0,
            messages: [],
            variables: {},
            config: basicConfig,
          },
        }),
        listCheckpoints: async () => [],
      };

      const entity = await AgentLoopFactory.fromCheckpoint("checkpoint-id", mockDependencies);

      expect(entity).toBeDefined();
    });
  });
});
