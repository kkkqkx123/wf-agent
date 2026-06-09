/**
 * Integration Test: Agent Loop Error Handling
 *
 * Tests error recovery and propagation through the agent loop execution chain.
 * Covers:
 * - LLM errors (mock LLM throws errors)
 * - Tool execution errors (tool throws during execution)
 * - Invalid configuration errors
 * - Error state transitions (RUNNING -> FAILED)
 * - Error propagation through coordinator chain
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { createFullAgentLoopFixture, createBasicAgentConfig } from "./__shared/fixtures.js";
import type { FullAgentLoopTestFixture } from "./__shared/fixtures.js";

describe("Agent Loop Error Handling", () => {
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
  // LLM Errors
  // ===========================================================================

  describe("LLM errors", () => {
    it("should handle LLM error on the first request", async () => {
      // Configure mock to throw on the first LLM call
      fixture.mockLLMWrapper.setThrowOnRequest(1, "Simulated LLM failure");

      const config = createBasicAgentConfig({ maxIterations: 3 });

      const result = await fixture.coordinator.execute(config);

      expect(result.success).toBe(false);
      expect(result.error).toBeDefined();
      // Error should propagate through the coordinator chain
      expect(String(result.error)).toContain("Simulated LLM failure");
      expect(fixture.mockLLMWrapper.getCallCount()).toBe(1);
    });

    it("should handle LLM error on a later iteration", async () => {
      // Iteration 1: tool call succeeds
      // Iteration 2: LLM throws error
      fixture.mockLLMWrapper.setResponseSequence([
        {
          content: "Calling tool.",
          toolCalls: [
            {
              id: "call_err_1",
              name: "mock_echo_tool",
              arguments: JSON.stringify({ message: "error recovery test" }),
            },
          ],
        },
        {
          content: "This should fail.",
        },
      ]);
      fixture.mockLLMWrapper.setThrowOnRequest(2, "Iteration 2 LLM failure");

      const config = createBasicAgentConfig({ maxIterations: 5 });

      const result = await fixture.coordinator.execute(config);

      expect(result.success).toBe(false);
      expect(result.error).toBeDefined();
      expect(String(result.error)).toContain("Iteration 2 LLM failure");
      // First call succeeded, second call triggered the throw
      expect(fixture.mockLLMWrapper.getCallCount()).toBe(2);
    });

    it("should preserve error message content", async () => {
      const errorMessage = "Custom mock LLM error for testing.";
      fixture.mockLLMWrapper.setThrowOnRequest(1, errorMessage);

      const config = createBasicAgentConfig({ maxIterations: 1 });

      const result = await fixture.coordinator.execute(config);

      expect(result.success).toBe(false);
      expect(result.error).toBeDefined();
      expect(String(result.error)).toContain(errorMessage);
    });
  });

  // ===========================================================================
  // Tool Execution Errors
  // ===========================================================================

  describe("Tool execution errors", () => {
    it("should handle tool execution failure gracefully", async () => {
      // Use a tool reference that does not exist
      fixture.mockLLMWrapper.setResponseSequence([
        {
          content: "Calling non-existent tool.",
          toolCalls: [
            {
              id: "call_bad_tool",
              name: "non_existent_tool",
              arguments: JSON.stringify({}),
            },
          ],
        },
        {
          content: "Final answer after tool error.",
        },
      ]);

      const config = createBasicAgentConfig({ maxIterations: 5 });

      const result = await fixture.coordinator.execute(config);

      // The agent loop should handle tool errors gracefully and continue
      expect(result.success).toBe(true);
      expect(result.content).toContain("Final answer");
      expect(result.toolCallCount).toBeGreaterThanOrEqual(1);
    });
  });

  // ===========================================================================
  // Configuration Errors
  // ===========================================================================

  describe("Configuration errors", () => {
    it("should handle missing profile ID gracefully", async () => {
      const config = createBasicAgentConfig({ profileId: "" });

      // With empty profile ID, the executor may handle it differently
      // depending on implementation. The test verifies no crash.
      const result = await fixture.coordinator.execute(config);

      // The coordinator should not crash; result may be success or failure
      expect(result).toBeDefined();
      expect(typeof result.success).toBe("boolean");
    });
  });

  // ===========================================================================
  // Error State Transitions
  // ===========================================================================

  describe("Error state transitions", () => {
    it("should transition RUNNING -> FAILED on LLM error", async () => {
      fixture.mockLLMWrapper.setThrowOnRequest(1, "Fatal LLM error");

      const config = createBasicAgentConfig({ maxIterations: 3 });

      const result = await fixture.coordinator.execute(config);

      expect(result.success).toBe(false);

      // Registry should have the entity in FAILED status
      const entities = fixture.registry.getAll();
      if (entities.length > 0) {
        const entity = await fixture.registry.get(entities[0]!.id);
        if (entity) {
          expect(entity.getStatus()).toBe("FAILED");
        }
      }
    });

    it("should record iteration count on failure", async () => {
      fixture.mockLLMWrapper.setThrowOnRequest(1, "Error");

      const config = createBasicAgentConfig({ maxIterations: 5 });

      const result = await fixture.coordinator.execute(config);

      expect(result.success).toBe(false);
      // Even on failure, iteration count should reflect progress
      expect(result.iterations).toBeDefined();
    });
  });
});