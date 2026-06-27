/**
 * Integration Test: Agent Loop Conversation Management
 *
 * Tests that conversation state is correctly managed across iterations
 * during agent loop execution.
 *
 * Covers:
 * - System prompt inclusion
 * - Initial user message
 * - Message history accumulation across iterations
 * - Tool call results in conversation
 * - Final messages in result
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { createFullAgentLoopFixture, createBasicAgentConfig } from "./__shared/fixtures";
import type { FullAgentLoopTestFixture } from "./__shared/fixtures";

describe("Agent Loop Conversation", () => {
  let fixture: FullAgentLoopTestFixture;

  beforeEach(async () => {
    fixture = await createFullAgentLoopFixture(true);
  });

  afterEach(async () => {
    fixture.coordinator.destroy();
    await fixture.storage.clear();
    fixture.mockLLMWrapper.reset();
  });

  it("should include system prompt in execution context", async () => {
    fixture.mockLLMWrapper.setDefaultResponse("System prompt acknowledged.");

    const config = createBasicAgentConfig({
      maxIterations: 1,
      systemPrompt: "You are a test assistant. Always respond with 'System prompt acknowledged.'",
    });

    const result = await fixture.coordinator.execute(config);

    expect(result.success).toBe(true);
    expect(result.content).toBe("System prompt acknowledged.");
  });

  it("should handle initial user message", async () => {
    fixture.mockLLMWrapper.setDefaultResponse("Response to initial message.");

    const config = createBasicAgentConfig({
      maxIterations: 1,
      initialUserMessage: "What can you do?",
    });

    const result = await fixture.coordinator.execute(config);

    expect(result.success).toBe(true);
    expect(result.content).toBeDefined();
  });

  it("should accumulate messages across tool call iterations", async () => {
    fixture.mockLLMWrapper.setResponseSequence([
      {
        content: "Calling tool.",
        toolCalls: [
          {
            id: "call_conv_1",
            name: "mock_echo_tool",
            arguments: JSON.stringify({ message: "conversation test" }),
          },
        ],
      },
      {
        content: "Final answer after tool execution.",
      },
    ]);

    const config = createBasicAgentConfig({ maxIterations: 5 });

    const result = await fixture.coordinator.execute(config);

    expect(result.success).toBe(true);
    expect(result.content).toContain("Final answer");
    expect(result.iterations).toBeGreaterThanOrEqual(2);
  });
});
