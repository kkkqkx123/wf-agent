/**
 * Temporary debug test to diagnose the "Cannot read properties of undefined (reading 'name')" error.
 */
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { createFullAgentLoopFixture, createBasicAgentConfig } from "./fixtures.js";

describe("Debug Tool Call Error", () => {
  let fixture: Awaited<ReturnType<typeof createFullAgentLoopFixture>>;

  beforeEach(async () => {
    fixture = await createFullAgentLoopFixture(true);
  });

  afterEach(async () => {
    fixture.coordinator.destroy();
    await fixture.storage.clear();
    fixture.mockLLMWrapper.reset();
  });

  it("should capture stack trace", async () => {
    fixture.mockLLMWrapper.setResponseSequence([
      {
        content: "Calling tool.",
        toolCalls: [
          {
            id: "call_echo_1",
            name: "mock_echo_tool",
            arguments: JSON.stringify({ message: "test" }),
          },
        ],
      },
      {
        content: "Final answer.",
      },
    ]);

    const config = createBasicAgentConfig({ maxIterations: 5 });

    try {
      const result = await fixture.coordinator.execute(config);
      process.stdout.write(JSON.stringify({ result }, null, 2) + "\n");
      expect(result.success).toBe(true);
    } catch (err: any) {
      process.stdout.write("CAUGHT ERROR: " + (err?.stack || err?.message || String(err)) + "\n");
      throw err;
    }
  });
});
