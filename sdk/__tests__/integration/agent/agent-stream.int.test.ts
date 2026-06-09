/**
 * Integration Test: Agent Loop Streaming Execution
 *
 * Tests the streaming execution mode of the agent loop.
 * Covers:
 * - Stream events yield during execution
 * - Stream completion with final answer
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import {
  createFullAgentLoopFixture,
  createBasicAgentConfig,
} from "./__shared/fixtures.js";
import type { FullAgentLoopTestFixture } from "./__shared/fixtures.js";
import type { AgentLoopStreamEvent } from "@/agent/execution/coordinators/agent-loop-coordinator.js";

describe("Agent Loop Streaming", () => {
  let fixture: FullAgentLoopTestFixture;

  beforeEach(async () => {
    fixture = await createFullAgentLoopFixture(true);
  });

  afterEach(async () => {
    fixture.coordinator.destroy();
    await fixture.storage.clear();
    fixture.mockLLMWrapper.reset();
  });

  it("should yield stream events during execution", async () => {
    fixture.mockLLMWrapper.setDefaultResponse("Streaming final answer.");

    const config = createBasicAgentConfig({
      maxIterations: 1,
      stream: true,
    });

    const events: AgentLoopStreamEvent[] = [];
    for await (const event of fixture.coordinator.executeStream(config)) {
      events.push(event);
    }

    // Should have yielded some events during the streaming execution
    expect(events.length).toBeGreaterThan(0);
  });

  it("should complete streaming execution successfully", async () => {
    fixture.mockLLMWrapper.setDefaultResponse("Streaming complete.");

    const config = createBasicAgentConfig({
      maxIterations: 1,
      stream: true,
    });

    let eventCount = 0;
    for await (const _event of fixture.coordinator.executeStream(config)) {
      eventCount++;
    }

    // Verify events were yielded
    expect(eventCount).toBeGreaterThan(0);

    // Verify entity completed successfully
    const entities = fixture.registry.getAll();
    if (entities.length > 0) {
      const entity = await fixture.registry.get(entities[0]!.id);
      expect(entity?.getStatus()).toBe("COMPLETED");
    }
  });
});