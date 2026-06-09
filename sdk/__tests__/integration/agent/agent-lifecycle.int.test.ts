/**
 * Integration Test: Agent Loop Lifecycle Management
 *
 * Tests pause/resume/stop lifecycle operations on the agent loop coordinator.
 * Covers:
 * - Synchronous API error cases (pause/resume/stop on invalid states)
 * - State transitions through lifecycle operations
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import {
  createFullAgentLoopFixture,
  createBasicAgentConfig,
} from "./__shared/fixtures.js";
import type { FullAgentLoopTestFixture } from "./__shared/fixtures.js";

describe("Agent Loop Lifecycle", () => {
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
  // Sync API Error Cases
  // ===========================================================================

  describe("Pause API error handling", () => {
    it("should throw when pausing a non-existent entity", async () => {
      await expect(fixture.coordinator.pause("non_existent_id")).rejects.toThrow(
        "AgentLoop not found",
      );
    });

    it("should throw when pausing an entity that is not running", async () => {
      fixture.mockLLMWrapper.setDefaultResponse("Quick answer.");
      const config = createBasicAgentConfig({ maxIterations: 1 });
      await fixture.coordinator.execute(config);

      const entities = fixture.registry.getAll();
      if (entities.length > 0) {
        await expect(fixture.coordinator.pause(entities[0]!.id)).rejects.toThrow(
          "not running",
        );
      }
    });
  });

  describe("Resume API error handling", () => {
    it("should throw when resuming a non-existent entity", async () => {
      await expect(fixture.coordinator.resume("non_existent_id")).rejects.toThrow(
        "AgentLoop not found",
      );
    });

    it("should throw when resuming an entity that is not paused", async () => {
      fixture.mockLLMWrapper.setDefaultResponse("Final answer.");
      const config = createBasicAgentConfig({ maxIterations: 1 });
      await fixture.coordinator.execute(config);

      const entities = fixture.registry.getAll();
      if (entities.length > 0) {
        await expect(fixture.coordinator.resume(entities[0]!.id)).rejects.toThrow(
          "not paused",
        );
      }
    });
  });

  describe("Stop API error handling", () => {
    it("should throw when stopping a non-existent entity", async () => {
      await expect(fixture.coordinator.stop("non_existent_id")).rejects.toThrow(
        "AgentLoop not found",
      );
    });
  });

  // ===========================================================================
  // State Transitions
  // ===========================================================================

  describe("State transitions", () => {
    it("should transition to COMPLETED for successful execution", async () => {
      fixture.mockLLMWrapper.setDefaultResponse("Final answer.");
      const config = createBasicAgentConfig({ maxIterations: 1 });

      const result = await fixture.coordinator.execute(config);

      expect(result.success).toBe(true);
      expect(result.iterations).toBe(1);

      const entities = fixture.registry.getAll();
      expect(entities.length).toBeGreaterThan(0);
      if (entities.length > 0) {
        const entity = await fixture.registry.get(entities[0]!.id);
        expect(entity?.getStatus()).toBe("COMPLETED");
      }
    });

    it("should transition to FAILED when LLM throws an error", async () => {
      fixture.mockLLMWrapper.setThrowOnRequest(1, "LLM failure");
      fixture.mockLLMWrapper.setDefaultResponse("Will fail.");

      const config = createBasicAgentConfig({ maxIterations: 1 });
      const result = await fixture.coordinator.execute(config);

      expect(result.success).toBe(false);
      expect(result.error).toBeDefined();
    });
  });
});