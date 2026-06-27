/**
 * Integration Test: Agent Loop Checkpoint Integration
 *
 * Tests basic checkpoint-related configuration and behavior during
 * agent loop execution.
 *
 * Covers:
 * - createCheckpointOnEnd flag configuration
 * - createCheckpointOnError flag configuration
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { createFullAgentLoopFixture, createBasicAgentConfig } from "./__shared/fixtures";
import type { FullAgentLoopTestFixture } from "./__shared/fixtures";

describe("Agent Loop Checkpoint", () => {
  let fixture: FullAgentLoopTestFixture;

  beforeEach(async () => {
    fixture = await createFullAgentLoopFixture(true);
  });

  afterEach(async () => {
    fixture.coordinator.destroy();
    await fixture.storage.clear();
    fixture.mockLLMWrapper.reset();
  });

  it("should execute with createCheckpointOnEnd=false", async () => {
    fixture.mockLLMWrapper.setDefaultResponse("Checkpoint test answer.");

    const config = createBasicAgentConfig({
      maxIterations: 1,
      createCheckpointOnEnd: false,
    });

    const result = await fixture.coordinator.execute(config);

    expect(result.success).toBe(true);
    expect(result.content).toBeDefined();
  });

  it("should execute with createCheckpointOnError=false", async () => {
    fixture.mockLLMWrapper.setDefaultResponse("Checkpoint error test answer.");

    const config = createBasicAgentConfig({
      maxIterations: 1,
      createCheckpointOnError: false,
    });

    const result = await fixture.coordinator.execute(config);

    expect(result.success).toBe(true);
  });

  it("should handle both checkpoint flags together", async () => {
    fixture.mockLLMWrapper.setDefaultResponse("Both flags test.");

    const config = createBasicAgentConfig({
      maxIterations: 1,
      createCheckpointOnEnd: false,
      createCheckpointOnError: false,
    });

    const result = await fixture.coordinator.execute(config);

    expect(result.success).toBe(true);
  });
});
