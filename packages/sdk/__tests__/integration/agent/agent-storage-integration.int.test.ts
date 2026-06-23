/**
 * Integration Test: Agent Loop Storage Integration
 *
 * Verifies that agent loop entities are properly persisted to storage
 * and can be retrieved in subsequent sessions.
 *
 * Architecture:
 * - Uses MemoryAgentLoopStorage for isolated testing
 * - Exercises the real AgentLoopRegistry with storage adapter
 * - Verifies storage contents directly via storage adapter API
 * - Uses fixture.registry.getAllIds() to get entity IDs after execution
 *   (result.agentLoopId may not be populated by all coordinator paths)
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { createFullAgentLoopFixture, createBasicAgentConfig } from "./__shared/fixtures.js";
import type { FullAgentLoopTestFixture } from "./__shared/fixtures.js";

describe("Agent Loop Storage Integration", () => {
  let fixture: FullAgentLoopTestFixture;

  beforeEach(async () => {
    fixture = await createFullAgentLoopFixture(true);
  });

  afterEach(async () => {
    fixture.coordinator.destroy();
    await fixture.storage.clear();
    fixture.mockLLMWrapper.reset();
  });

  /** Helper: gets the single stored entity ID from registry */
  function getSingleEntityId(): string {
    const ids = fixture.registry.getAllIds();
    expect(ids.length).toBe(1);
    return ids[0]!;
  }

  // ===========================================================================
  // AS-INT-01: Agent loop entity is persisted to storage after execution
  // ===========================================================================

  describe("Persistence After Execution (AS-INT-01)", () => {
    it("should persist agent loop entity to storage after successful execution", async () => {
      fixture.mockLLMWrapper.setDefaultResponse("Final answer from agent.");

      const config = createBasicAgentConfig({ maxIterations: 1 });
      await fixture.coordinator.execute(config);

      // Verify entity exists in registry
      const entityId = getSingleEntityId();
      expect(entityId).toBeDefined();

      // Verify data exists in storage
      const storedIds = await fixture.storage.list();
      expect(storedIds.length).toBe(1);
      expect(storedIds[0]).toBe(entityId);
    });

    it("should persist agent loop entity to storage after multiple iterations", async () => {
      fixture.mockLLMWrapper.setResponseSequence([
        {
          content: "Tool result.",
          toolCalls: [
            {
              id: "call_1",
              name: "mock_echo_tool",
              arguments: JSON.stringify({ message: "echo" }),
            },
          ],
        },
        { content: "Final answer after tools." },
      ]);

      const config = createBasicAgentConfig({ maxIterations: 2 });
      await fixture.coordinator.execute(config);

      // Verify data exists in storage
      const entityId = getSingleEntityId();
      const storedIds = await fixture.storage.list();
      expect(storedIds.length).toBe(1);
      expect(storedIds[0]).toBe(entityId);
    });
  });

  // ===========================================================================
  // AS-INT-02: Persisted data contains correct information
  // ===========================================================================

  describe("Persisted Data Content (AS-INT-02)", () => {
    it("should persist raw data that can be decoded back", async () => {
      fixture.mockLLMWrapper.setDefaultResponse("Done.");

      const config = createBasicAgentConfig({ maxIterations: 1 });
      await fixture.coordinator.execute(config);

      const entityId = getSingleEntityId();

      // Load raw data from storage
      const rawData = await fixture.storage.load(entityId);
      expect(rawData).not.toBeNull();

      const decoded = JSON.parse(new TextDecoder().decode(rawData!));
      expect(decoded.id).toBe(entityId);
      expect(decoded.status).toBeDefined();
      // currentIteration may be 0 at registration time (before first iteration increment)
      expect(typeof decoded.currentIteration).toBe("number");
    });

    it("should return metadata for stored agent loop", async () => {
      fixture.mockLLMWrapper.setDefaultResponse("Final.");
      const config = createBasicAgentConfig({ maxIterations: 1 });
      await fixture.coordinator.execute(config);

      const entityId = getSingleEntityId();

      // Check metadata directly from storage
      const metadata = await fixture.storage.getMetadata(entityId);
      expect(metadata).not.toBeNull();
      expect(metadata!.agentLoopId).toBe(entityId);
    });
  });

  // ===========================================================================
  // AS-INT-03: Multiple agent loops are independently persisted
  // ===========================================================================

  describe("Multiple Agent Loops (AS-INT-03)", () => {
    it("should persist multiple agent loop entities independently", async () => {
      fixture.mockLLMWrapper.setDefaultResponse("Answer.");

      // Execute first agent loop
      await fixture.coordinator.execute(createBasicAgentConfig({ maxIterations: 1 }));
      fixture.registry.getAllIds();

      // Execute second agent loop
      await fixture.coordinator.execute(createBasicAgentConfig({ maxIterations: 1 }));
      const secondIds = fixture.registry.getAllIds();

      // Two different entities
      expect(secondIds.length).toBe(2);

      // Both should be stored independently in storage
      const storedIds = await fixture.storage.list();
      expect(storedIds.length).toBe(2);
      expect(storedIds.sort()).toEqual(secondIds.sort());
    });
  });

  // ===========================================================================
  // AS-INT-04: Storage metadata operations
  // ===========================================================================

  describe("Storage Metadata Operations (AS-INT-04)", () => {
    it("should allow status update via storage adapter", async () => {
      fixture.mockLLMWrapper.setDefaultResponse("Done.");

      await fixture.coordinator.execute(createBasicAgentConfig({ maxIterations: 1 }));

      const entityId = getSingleEntityId();

      // The entity should exist in storage after execution
      const storedIds = await fixture.storage.list();
      expect(storedIds.length).toBe(1);

      // Update status via storage adapter
      await fixture.storage.updateAgentLoopStatus(entityId, "COMPLETED");

      // Verify status change via metadata
      const metadata = await fixture.storage.getMetadata(entityId);
      expect(metadata).not.toBeNull();
      expect(metadata!.status).toBe("COMPLETED");
    });

    it("should list agent loops by status after manual update", async () => {
      fixture.mockLLMWrapper.setDefaultResponse("Done.");

      await fixture.coordinator.execute(createBasicAgentConfig({ maxIterations: 1 }));

      const entityId = getSingleEntityId();
      await fixture.storage.updateAgentLoopStatus(entityId, "COMPLETED");

      const completedIds = await fixture.storage.listByStatus("COMPLETED");
      expect(completedIds.length).toBe(1);
      expect(completedIds[0]).toBe(entityId);

      // Stats should reflect status change
      const stats = await fixture.storage.getAgentLoopStats();
      expect(stats.total).toBe(1);
      expect(stats.byStatus["COMPLETED"]).toBe(1);
    });
  });

  // ===========================================================================
  // AS-INT-05: Entity cleanup from storage
  // ===========================================================================

  describe("Entity Cleanup (AS-INT-05)", () => {
    it("should clear all stored data via storage adapter", async () => {
      fixture.mockLLMWrapper.setDefaultResponse("Clear.");

      await fixture.coordinator.execute(createBasicAgentConfig({ maxIterations: 1 }));

      expect((await fixture.storage.list()).length).toBe(1);

      await fixture.storage.clear();

      expect((await fixture.storage.list()).length).toBe(0);
    });
  });
});
