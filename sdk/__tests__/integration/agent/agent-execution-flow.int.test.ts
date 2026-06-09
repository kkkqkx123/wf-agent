/**
 * Integration Test: Agent Loop Execution Flow
 *
 * Tests the complete agent loop execution flow through the full coordinator chain
 * (AgentLoopCoordinator -> AgentLoopExecutor -> AgentExecutionCoordinator ->
 * AgentIterationCoordinator -> CoreLLMExecutionCoordinator -> LLMExecutor -> MockLLMWrapper).
 *
 * Covers:
 * - Single-iteration execution (no tools)
 * - Multi-iteration execution with tool calls
 * - Maximum iterations reached behavior
 * - Tool call result flow through the agent loop
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { createFullAgentLoopFixture, createBasicAgentConfig } from "./__shared/fixtures.js";
import type { FullAgentLoopTestFixture } from "./__shared/fixtures.js";

// =============================================================================
// Mock Data
// =============================================================================

const echoToolCall = {
  id: "call_echo_1",
  name: "mock_echo_tool",
  arguments: JSON.stringify({ message: "Hello from agent loop" }),
};

const toolResponseConfig = {
  toolCalls: [echoToolCall],
};

describe("Agent Loop Execution Flow", () => {
  let fixture: FullAgentLoopTestFixture;

  beforeEach(async () => {
    fixture = await createFullAgentLoopFixture(true);
  });

  afterEach(async () => {
    fixture.coordinator.destroy();
    await fixture.storage.clear();
    fixture.mockLLMWrapper.reset();
  });

  // ===========================================================================
  // Single Iteration (No Tools)
  // ===========================================================================

  describe("Single Iteration (no tools)", () => {
    it("should complete a single iteration with a final answer", async () => {
      fixture.mockLLMWrapper.setDefaultResponse(
        "This is the final answer from the mock LLM.",
      );

      const config = createBasicAgentConfig({ maxIterations: 1 });

      const result = await fixture.coordinator.execute(config);

      expect(result.success).toBe(true);
      expect(result.content).toBeDefined();
      expect(result.content).toContain("final answer");
      expect(result.iterations).toBe(1);
      expect(result.toolCallCount).toBe(0);
    });

    it("should respect maxIterations=1 even when LLM returns tool calls", async () => {
      // LLM returns tool calls, but maxIterations=1 means the loop stops after
      // one iteration regardless. This verifies the agent loop properly limits iterations.
      fixture.mockLLMWrapper.setResponseSequence([
        {
          content: "I need to use a tool.",
          toolCalls: [echoToolCall],
        },
      ]);

      const config = createBasicAgentConfig({ maxIterations: 1 });

      const result = await fixture.coordinator.execute(config);

      // With maxIterations=1 and a tool call in the first response, the tool gets
      // executed but there is no second iteration for final answer.
      expect(result.success).toBe(true);
      // Iteration should complete (tool was executed in iteration 1)
      expect(result.iterations).toBe(1);
    });

    it("should return empty content when LLM returns empty response", async () => {
      fixture.mockLLMWrapper.setDefaultResponse("");

      const config = createBasicAgentConfig({ maxIterations: 1 });

      const result = await fixture.coordinator.execute(config);

      expect(result.success).toBe(true);
      expect(result.content).toBe("");
      expect(result.iterations).toBe(1);
    });
  });

  // ===========================================================================
  // Multi-Iteration with Tool Execution
  // ===========================================================================

  describe("Multi-Iteration with Tool Execution", () => {
    it("should execute tool call and produce final answer in second iteration", async () => {
      // Response sequence:
      // Iteration 1: LLM returns tool call for echo tool
      // Iteration 2: LLM returns final answer
      fixture.mockLLMWrapper.setResponseSequence([
        {
          content: "I need to look up information.",
          toolCalls: [
            {
              id: "call_echo_1",
              name: "mock_echo_tool",
              arguments: JSON.stringify({ message: "test data" }),
            },
          ],
        },
        {
          content: "The tool returned: test data. This is the final answer.",
        },
      ]);

      const config = createBasicAgentConfig({ maxIterations: 5 });

      const result = await fixture.coordinator.execute(config);

      expect(result.success).toBe(true);
      expect(result.content).toBeDefined();
      expect(result.content).toContain("final answer");
      expect(result.iterations).toBeGreaterThanOrEqual(2);
      expect(result.toolCallCount).toBeGreaterThanOrEqual(1);
      expect(fixture.mockLLMWrapper.getCallCount()).toBeGreaterThanOrEqual(2);
    });

    it("should execute multiple tool calls across iterations", async () => {
      // Response sequence:
      // Iteration 1: LLM calls tool
      // Iteration 2: LLM calls tool again
      // Iteration 3: LLM produces final answer
      fixture.mockLLMWrapper.setResponseSequence([
        {
          content: "First tool call.",
          toolCalls: [
            {
              id: "call_echo_1",
              name: "mock_echo_tool",
              arguments: JSON.stringify({ message: "first call" }),
            },
          ],
        },
        {
          content: "Second tool call.",
          toolCalls: [
            {
              id: "call_echo_2",
              name: "mock_echo_tool",
              arguments: JSON.stringify({ message: "second call" }),
            },
          ],
        },
        {
          content: "Final answer after two tool calls.",
        },
      ]);

      const config = createBasicAgentConfig({ maxIterations: 5 });

      const result = await fixture.coordinator.execute(config);

      expect(result.success).toBe(true);
      expect(result.content).toContain("Final answer");
      expect(result.iterations).toBeGreaterThanOrEqual(3);
      expect(result.toolCallCount).toBeGreaterThanOrEqual(2);
      expect(fixture.mockLLMWrapper.getCallCount()).toBeGreaterThanOrEqual(3);
    });
  });

  // ===========================================================================
  // Maximum Iterations
  // ===========================================================================

  describe("Maximum Iterations Handling", () => {
    it("should stop when maxIterations is reached without final answer", async () => {
      // LLM keeps returning tool calls forever; the loop should stop at maxIterations
      fixture.mockLLMWrapper.setResponseSequence([
        {
          content: "Calling tool again.",
          toolCalls: [
            {
              id: "call_echo_loop",
              name: "mock_echo_tool",
              arguments: JSON.stringify({ message: "loop" }),
            },
          ],
        },
      ]);

      const config = createBasicAgentConfig({ maxIterations: 3 });

      const result = await fixture.coordinator.execute(config);

      expect(result.success).toBe(true);
      expect(result.iterations).toBe(3);
      expect(result.toolCallCount).toBeGreaterThanOrEqual(3);
      expect(fixture.mockLLMWrapper.getCallCount()).toBe(3);
    });

    it("should handle maxIterations=2 with tool call pattern", async () => {
      // Each iteration triggers a tool call, should stop after 2 iterations
      fixture.mockLLMWrapper.setResponseSequence([
        {
          content: "Calling tool 1.",
          toolCalls: [
            {
              id: "call_t1",
              name: "mock_echo_tool",
              arguments: JSON.stringify({ message: "iteration 1" }),
            },
          ],
        },
        {
          content: "Calling tool 2.",
          toolCalls: [
            {
              id: "call_t2",
              name: "mock_echo_tool",
              arguments: JSON.stringify({ message: "iteration 2" }),
            },
          ],
        },
      ]);

      const config = createBasicAgentConfig({ maxIterations: 2 });

      const result = await fixture.coordinator.execute(config);

      expect(result.success).toBe(true);
      expect(result.iterations).toBe(2);
      expect(result.toolCallCount).toBe(2);
    });
  });
});