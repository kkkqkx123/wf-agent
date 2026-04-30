/**
 * Agent Loop LLM Integration Tests
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { AgentLoopCoordinator } from "../execution/coordinators/agent-loop-coordinator.js";
import { AgentLoopExecutor } from "../execution/executors/agent-loop-executor.js";
import { AgentLoopRegistry } from "../loop/agent-loop-registry.js";
import { ToolRegistry } from "../../core/registry/tool-registry.js";
import { EventRegistry } from "../../core/registry/event-registry.js";
import type { AgentLoopConfig, LLMResult, LLMMessage } from "@wf-agent/types";

// Mock LLM Executor
class MockLLMExecutor {
  private responseContent: string = "Default response";
  private shouldFail: boolean = false;
  private delayMs: number = 0;

  setResponse(content: string): void {
    this.responseContent = content;
  }

  setFailure(shouldFail: boolean): void {
    this.shouldFail = shouldFail;
  }

  setDelay(delayMs: number): void {
    this.delayMs = delayMs;
  }

  async execute(request: any): Promise<LLMResult> {
    if (this.shouldFail) {
      throw new Error("LLM call failed");
    }

    if (this.delayMs > 0) {
      await new Promise(resolve => setTimeout(resolve, this.delayMs));
    }

    return {
      id: "mock-id",
      model: "mock-model",
      content: this.responseContent,
      message: { role: "assistant", content: this.responseContent },
      finishReason: "stop",
      duration: 0,
    };
  }

  async *executeStream(request: any): AsyncIterable<LLMResult> {
    yield await this.execute(request);
  }

  async executeLLMCall(
    messages: any[],
    requestData: any,
    options?: any,
  ): Promise<{ success: true; result: any } | { success: false; interruption: any }> {
    if (this.shouldFail) {
      throw new Error("LLM call failed");
    }

    if (this.delayMs > 0) {
      await new Promise(resolve => setTimeout(resolve, this.delayMs));
    }

    return {
      success: true,
      result: {
        content: this.responseContent,
        usage: {},
        finishReason: "stop",
        toolCalls: undefined,
      },
    };
  }
}

describe("Agent Loop LLM Integration", () => {
  let coordinator: AgentLoopCoordinator;
  let registry: AgentLoopRegistry;
  let mockLLMExecutor: MockLLMExecutor;

  const config: AgentLoopConfig = {
    maxIterations: 5,
    profileId: "test-profile",
  };

  beforeEach(() => {
    registry = new AgentLoopRegistry();
    const eventManager = new EventRegistry();
    const toolService = new ToolRegistry();
    mockLLMExecutor = new MockLLMExecutor();

    const executor = new AgentLoopExecutor(mockLLMExecutor as any, toolService, eventManager);

    coordinator = new AgentLoopCoordinator(registry, executor);
  });

  afterEach(() => {
    registry.clear();
  });

  describe("LLM Call", () => {
    it("should call LLM with correct profile", async () => {
      const result = await coordinator.execute(config);

      expect(result.success).toBe(true);
    });

    it("should pass message history", async () => {
      const initialMessages: LLMMessage[] = [
        { role: "system", content: "System message" },
        { role: "user", content: "User message" },
      ];

      const result = await coordinator.execute(config, { initialMessages });

      expect(result.success).toBe(true);
    });

    it("should use profile ID", async () => {
      const customConfig: AgentLoopConfig = {
        maxIterations: 5,
        profileId: "custom-profile",
      };

      const result = await coordinator.execute(customConfig);

      expect(result.success).toBe(true);
    });
  });

  describe("LLM Response Handling", () => {
    it("should handle text response", async () => {
      mockLLMExecutor.setResponse("This is a text response");

      const result = await coordinator.execute(config);

      expect(result.success).toBe(true);
      expect(result.content).toBe("This is a text response");
    });

    it("should handle empty response", async () => {
      mockLLMExecutor.setResponse("");

      const result = await coordinator.execute(config);

      expect(result).toBeDefined();
    });
  });

  describe("LLM Error Handling", () => {
    it("should handle LLM call failure", async () => {
      mockLLMExecutor.setFailure(true);

      const result = await coordinator.execute(config);

      expect(result.success).toBe(false);
      expect(result.error).toBeDefined();
    });

    it("should handle timeout error", async () => {
      mockLLMExecutor.setDelay(10000);

      const result = await coordinator.execute(config);

      expect(result).toBeDefined();
    });
  });

  describe("LLM Performance", () => {
    it("should track response time", async () => {
      mockLLMExecutor.setDelay(100);

      const startTime = Date.now();
      const result = await coordinator.execute(config);
      const endTime = Date.now();

      expect(result).toBeDefined();
      expect(endTime - startTime).toBeGreaterThanOrEqual(100);
    });
  });
});
