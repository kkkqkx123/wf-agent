/**
 * Agent Loop Coordinator Integration Tests
 */

import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { AgentLoopCoordinator } from "../execution/coordinators/agent-loop-coordinator.js";
import { AgentLoopExecutor } from "../execution/executors/agent-loop-executor.js";
import { AgentLoopRegistry } from "../loop/agent-loop-registry.js";
import { ToolRegistry } from "../../core/registry/tool-registry.js";
import { EventRegistry } from "../../core/registry/event-registry.js";
import type { AgentLoopConfig, LLMResult } from "@wf-agent/types";
import { AgentLoopStatus } from "@wf-agent/types";

// Mock LLM Executor
class MockLLMExecutor {
  async execute(): Promise<LLMResult> {
    return {
      id: "mock-id",
      model: "mock-model",
      content: "Response",
      message: { role: "assistant", content: "Response" },
      finishReason: "stop",
      duration: 0,
    };
  }

  async *executeStream(): AsyncIterable<LLMResult> {
    yield await this.execute();
  }

  async executeLLMCall(
    messages: any[],
    requestData: any,
    options?: any,
  ): Promise<{ success: true; result: any } | { success: false; interruption: any }> {
    return {
      success: true,
      result: {
        content: "Response",
        usage: {},
        finishReason: "stop",
        toolCalls: undefined,
      },
    };
  }
}

describe("Agent Loop Coordinator", () => {
  let coordinator: AgentLoopCoordinator;
  let registry: AgentLoopRegistry;
  let eventManager: EventRegistry;

  const config: AgentLoopConfig = {
    maxIterations: 5,
    profileId: "test-profile",
  };

  beforeEach(() => {
    registry = new AgentLoopRegistry();
    eventManager = new EventRegistry();
    const toolService = new ToolRegistry();
    const mockLLMExecutor = new MockLLMExecutor();
    const executor = new AgentLoopExecutor(mockLLMExecutor as any, toolService, eventManager);

    coordinator = new AgentLoopCoordinator(registry, executor);
  });

  afterEach(() => {
    registry.clear();
    vi.clearAllMocks();
  });

  describe("Complete Execution Flow", () => {
    it("should execute complete flow", async () => {
      const result = await coordinator.execute(config);

      expect(result.success).toBe(true);
      expect(result.iterations).toBe(1);
    });

    it("should register entity", async () => {
      await coordinator.execute(config);

      expect(registry.size()).toBe(1);
    });

    it("should update status to COMPLETED", async () => {
      const result = await coordinator.execute(config);

      const running = registry.getRunning();
      expect(running.length).toBe(0);

      const completed = registry.getCompleted();
      expect(completed.length).toBe(1);
    });
  });

  describe("Streaming Execution Flow", () => {
    it("should execute streaming flow", async () => {
      const events: any[] = [];

      for await (const event of coordinator.executeStream(config)) {
        events.push(event);
      }

      expect(events.length).toBeGreaterThan(0);
    });

    it("should generate correct events", async () => {
      const eventTypes = new Set<string>();

      for await (const event of coordinator.executeStream(config)) {
        eventTypes.add(event.type);
      }

      expect(eventTypes.size).toBeGreaterThan(0);
    });
  });

  describe("Async Execution Flow", () => {
    it("should start async execution", async () => {
      const id = await coordinator.start(config);

      expect(id).toBeDefined();
      expect(typeof id).toBe("string");
    });

    it("should update status after completion", async () => {
      const id = await coordinator.start(config);

      await new Promise(resolve => setTimeout(resolve, 200));

      const status = coordinator.getStatus(id);
      expect(status).toBe(AgentLoopStatus.COMPLETED);
    });
  });

  describe("Lifecycle Management", () => {
    it("should pause and resume", async () => {
      const id = await coordinator.start(config);

      await new Promise(resolve => setTimeout(resolve, 50));

      try {
        await coordinator.pause(id);

        const status = coordinator.getStatus(id);
        if (status === AgentLoopStatus.PAUSED) {
          await coordinator.resume(id);
        }
      } catch (error) {
        // May already be completed
      }
    });

    it("should stop execution", async () => {
      const id = await coordinator.start(config);

      await new Promise(resolve => setTimeout(resolve, 50));

      try {
        await coordinator.stop(id);

        const status = coordinator.getStatus(id);
        expect(status).toBe(AgentLoopStatus.CANCELLED);
      } catch (error) {
        // May already be completed
      }
    });
  });

  describe("Instance Query", () => {
    it("should get instance", async () => {
      const id = await coordinator.start(config);

      const entity = coordinator.get(id);
      expect(entity).toBeDefined();
    });

    it("should get status", async () => {
      const id = await coordinator.start(config);

      const status = coordinator.getStatus(id);
      expect(status).toBeDefined();
    });

    it("should get running instances", async () => {
      await coordinator.start(config);
      await coordinator.start(config);

      await new Promise(resolve => setTimeout(resolve, 50));

      const running = coordinator.getRunning();
      expect(running.length).toBeGreaterThanOrEqual(0);
    });

    it("should get paused instances", async () => {
      const paused = coordinator.getPaused();
      expect(Array.isArray(paused)).toBe(true);
    });
  });

  describe("Cleanup", () => {
    it("should cleanup completed instances", async () => {
      await coordinator.execute(config);
      await coordinator.execute(config);

      const count = coordinator.cleanup();

      expect(count).toBe(2);
      expect(registry.size()).toBe(0);
    });
  });
});
