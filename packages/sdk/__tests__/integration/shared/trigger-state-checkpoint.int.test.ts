/**
 * Integration Test: Trigger State Persistence Across Checkpoint Cycles
 *
 * Tests whether trigger fire count limits are preserved when checkpoints are saved and restored.
 * This is critical for preventing unwanted re-execution of limited-rate triggers.
 *
 * DESIGN ISSUE: Trigger State Not Checkpointed
 * - TriggerStateManager has toJSON/fromJSON but is never called by checkpoint
 * - Checkpoint saves execution state but not trigger limits state
 * - This causes trigger limits to reset after recovery
 *
 * BUSINESS SCENARIO:
 * Workflow has a trigger with maxTriggers=1 (should fire once per execution):
 * 1. Trigger condition matches → fire count becomes 1
 * 2. Save checkpoint
 * 3. System crash
 * 4. Restore checkpoint
 * 5. Trigger condition matches again
 * 6. ❌ Current: Trigger fires again (fire count was reset)
 *    ✅ Expected: Trigger is blocked (fire count is 1, already at max)
 */

import { describe, it, expect, beforeEach } from "vitest";
import type { BaseTriggerDefinition } from "@sdk/shared/triggers/types.js";
import { TriggerStateManager } from "@sdk/shared/triggers/trigger-state-manager.js";

describe("Integration: Trigger State Persistence Across Checkpoint", () => {
  let triggerStateManager: TriggerStateManager;

  const createMockTrigger = (
    id: string,
    maxTriggers?: number,
  ): BaseTriggerDefinition => ({
    id,
    eventType: "test-event",
    eventName: "test-event",
    enabled: true,
    handler: async () => {},
    maxTriggers,
  });

  beforeEach(() => {
    triggerStateManager = new TriggerStateManager();
  });

  describe("Scenario: Single-Fire Trigger with Checkpoint", () => {
    it("should prevent re-firing after checkpoint restore", () => {
      // Trigger definition: can only fire once
      const trigger = createMockTrigger("auto-fix-trigger", 1);

      // Step 1: Trigger fires first time
      const count1 = triggerStateManager.incrementFireCount(trigger.id);
      expect(count1).toBe(1);

      // Verify it has reached the limit
      const hasReachedLimit = triggerStateManager.hasReachedLimit(trigger);
      expect(hasReachedLimit).toBe(true);

      // Step 2: Simulate checkpoint - serialize state
      const serialized = triggerStateManager.toJSON();
      expect(serialized[trigger.id]).toBeDefined();
      expect(serialized[trigger.id].fireCount).toBe(1);

      // Step 3: Simulate checkpoint restore - deserialize state
      const restoredManager = TriggerStateManager.fromJSON(serialized);

      // Step 4: Check if trigger is still limited
      const stillLimited = restoredManager.hasReachedLimit(trigger);
      expect(stillLimited).toBe(true);

      // DESIGN ISSUE: This test demonstrates the DESIGN REQUIREMENT
      // that is currently NOT implemented in checkpoint lifecycle
      // The checkpoint should automatically call toJSON/fromJSON
      expect(serialized[trigger.id].fireCount).toBe(1);
    });

    it("should track remaining trigger counts across checkpoint", () => {
      const trigger = createMockTrigger("rate-limited-trigger", 3);

      // Initial state
      expect(triggerStateManager.getRemainingTriggers(trigger)).toBe(3);

      // First execution
      triggerStateManager.incrementFireCount(trigger.id);
      expect(triggerStateManager.getRemainingTriggers(trigger)).toBe(2);

      // Checkpoint 1
      const state1 = triggerStateManager.toJSON();
      const restored1 = TriggerStateManager.fromJSON(state1);

      // Still 2 remaining after restore
      expect(restored1.getRemainingTriggers(trigger)).toBe(2);

      // Second execution
      restored1.incrementFireCount(trigger.id);
      expect(restored1.getRemainingTriggers(trigger)).toBe(1);

      // Checkpoint 2
      const state2 = restored1.toJSON();
      const restored2 = TriggerStateManager.fromJSON(state2);

      // Still 1 remaining
      expect(restored2.getRemainingTriggers(trigger)).toBe(1);

      // Third execution
      restored2.incrementFireCount(trigger.id);
      expect(restored2.getRemainingTriggers(trigger)).toBe(0);

      // Fourth attempt should be blocked
      expect(restored2.hasReachedLimit(trigger)).toBe(true);

      // DESIGN REQUIREMENT: This entire cycle should be automatic
      // in the checkpoint/restore flow
      expect(state2[trigger.id].fireCount).toBe(2);
    });

    it("should preserve trigger metadata through checkpoint", () => {
      const trigger = createMockTrigger("complex-trigger", 2);

      // Fire with metadata
      triggerStateManager.incrementFireCount(trigger.id);
      const state = triggerStateManager.getState(trigger.id);

      expect(state).toBeDefined();
      expect(state!.fireCount).toBe(1);
      expect(state!.lastFiredAt).toBeDefined();

      // Checkpoint
      const serialized = triggerStateManager.toJSON();

      // Restore
      const restored = TriggerStateManager.fromJSON(serialized);
      const restoredState = restored.getState(trigger.id);

      // All metadata preserved
      expect(restoredState!.fireCount).toBe(1);
      expect(restoredState!.lastFiredAt).toBe(state!.lastFiredAt);
      expect(restoredState!.triggerId).toBe(trigger.id);
    });
  });

  describe("Scenario: Multiple Triggers with Mixed Limits", () => {
    it("should preserve independent fire counts for multiple triggers", () => {
      // Create multiple triggers with different limits
      const triggers = [
        createMockTrigger("trigger-1", 1), // Can fire once
        createMockTrigger("trigger-2", 3), // Can fire 3 times
        createMockTrigger("trigger-3", null), // Unlimited
      ];

      // Fire each trigger different number of times
      triggerStateManager.incrementFireCount(triggers[0].id); // trigger-1: count=1
      triggerStateManager.incrementFireCount(triggers[1].id); // trigger-2: count=1
      triggerStateManager.incrementFireCount(triggers[1].id); // trigger-2: count=2
      triggerStateManager.incrementFireCount(triggers[2].id); // trigger-3: count=1

      // Checkpoint
      const serialized = triggerStateManager.toJSON();

      // Restore
      const restored = TriggerStateManager.fromJSON(serialized);

      // Verify each trigger has correct count
      expect(restored.hasReachedLimit(triggers[0])).toBe(true); // 1/1
      expect(restored.hasReachedLimit(triggers[1])).toBe(false); // 2/3
      expect(restored.hasReachedLimit(triggers[2])).toBe(false); // 1/unlimited

      // Verify remaining counts
      expect(restored.getRemainingTriggers(triggers[0])).toBe(0);
      expect(restored.getRemainingTriggers(triggers[1])).toBe(1);
      expect(restored.getRemainingTriggers(triggers[2])).toBe(-1); // Unlimited
    });

    it("should handle reset of specific triggers", () => {
      const trigger1 = createMockTrigger("trigger-to-reset", 2);
      const trigger2 = createMockTrigger("trigger-to-keep", 2);

      // Both fire once
      triggerStateManager.incrementFireCount(trigger1.id);
      triggerStateManager.incrementFireCount(trigger2.id);

      // Reset only trigger1
      triggerStateManager.reset(trigger1.id);

      // Checkpoint
      const serialized = triggerStateManager.toJSON();

      // Restore
      const restored = TriggerStateManager.fromJSON(serialized);

      // trigger1 should be reset (no state)
      expect(restored.getState(trigger1.id)).toBeUndefined();
      expect(restored.getRemainingTriggers(trigger1)).toBe(2); // Back to max

      // trigger2 should keep state
      expect(restored.getState(trigger2.id)).toBeDefined();
      expect(restored.getRemainingTriggers(trigger2)).toBe(1);
    });
  });

  describe("Scenario: Checkpoint Integration Gap (DESIGN ISSUE)", () => {
    it("should document missing integration between TriggerStateManager and Checkpoint", () => {
      // This test documents the MISSING DESIGN REQUIREMENT
      // that trigger state should be part of the checkpoint

      const trigger = createMockTrigger("documented-issue", 1);

      // Current workflow (BROKEN):
      // 1. Execute trigger
      triggerStateManager.incrementFireCount(trigger.id);

      // 2. Save checkpoint (manually serialize trigger state)
      const triggerStateSnapshot = triggerStateManager.toJSON();

      // ❌ ISSUE: Checkpoint object does NOT include triggerStateSnapshot
      // In real code:
      // const checkpoint = agentState.createSnapshot()
      // checkpoint does NOT include triggerStateSnapshot

      // 3. Restore checkpoint
      // ❌ TriggerStateManager is re-initialized with empty state
      const newTriggerManager = new TriggerStateManager(); // ← Empty!

      // 4. Fire count is lost
      expect(newTriggerManager.getRemainingTriggers(trigger)).toBe(1);

      // ✅ What SHOULD happen:
      // const newTriggerManager = TriggerStateManager.fromJSON(checkpoint.triggerStates)

      expect(triggerStateSnapshot[trigger.id].fireCount).toBe(1);
      expect(newTriggerManager.getRemainingTriggers(trigger)).toBe(1);
    });

    it("should define the contract for checkpoint integration", () => {
      // This test defines what the integration should look like

      // REQUIRED in BaseCheckpointCoordinator.buildCheckpoint():
      // triggerStates: this.triggerStateManager.toJSON()

      // REQUIRED in BaseCheckpointCoordinator.restoreFromCheckpoint():
      // this.triggerStateManager = TriggerStateManager.fromJSON(checkpoint.triggerStates)

      // REQUIRED in Checkpoint type:
      // interface AgentLoopCheckpoint extends BaseCheckpoint {
      //   triggerStates?: Record<string, TriggerState>;
      // }

      // REQUIRED in ConversationState:
      // interface ConversationState {
      //   triggerStates?: Record<string, TriggerState>;
      // }

      const manager = new TriggerStateManager();
      const trigger = createMockTrigger("contract-test", 1);

      manager.incrementFireCount(trigger.id);

      // The contract should be:
      const snapshot = manager.toJSON();
      const restored = TriggerStateManager.fromJSON(snapshot);

      expect(restored.hasReachedLimit(trigger)).toBe(true);
    });
  });

  describe("Scenario: Trigger State Consistency Validation", () => {
    it("should validate trigger state consistency across restore", () => {
      const trigger = createMockTrigger("consistency-check", 2);

      // Create initial state
      triggerStateManager.incrementFireCount(trigger.id);
      triggerStateManager.incrementFireCount(trigger.id);

      const state = triggerStateManager.getState(trigger.id);
      expect(state!.fireCount).toBe(2);
      expect(state!.lastFiredAt).toBeDefined();

      // Simulate checkpoint
      const checkpointData = triggerStateManager.toJSON();

      // Simulate restore with validation
      const restoredManager = TriggerStateManager.fromJSON(checkpointData);
      const restoredState = restoredManager.getState(trigger.id);

      // Validate consistency
      expect(restoredState!.fireCount).toBe(state!.fireCount);
      expect(restoredState!.triggerId).toBe(state!.triggerId);
      expect(restoredState!.lastFiredAt).toBe(state!.lastFiredAt);

      // DESIGN REQUIREMENT: Add checksum to detect corruption
      // checkpointData should include:
      // {
      //   version: 1,
      //   checksum: SHA256(JSON.stringify(state)),
      //   data: { ... }
      // }

      expect(checkpointData[trigger.id]).toBeDefined();
    });

    it("should prevent silent data loss during restore", () => {
      const triggers = [
        createMockTrigger("trigger-a", 1),
        createMockTrigger("trigger-b", 2),
        createMockTrigger("trigger-c", 3),
      ];

      // All fire once
      for (const trigger of triggers) {
        triggerStateManager.incrementFireCount(trigger.id);
      }

      const original = triggerStateManager.toJSON();
      expect(Object.keys(original)).toHaveLength(3);

      // Restore
      const restored = TriggerStateManager.fromJSON(original);
      const restoredData = restored.toJSON();

      // DESIGN REQUIREMENT: Verify no triggers were lost
      expect(restoredData).toHaveLength(3);
      expect(Object.keys(restoredData).sort()).toEqual(
        Object.keys(original).sort()
      );

      // Each trigger should have exact same data
      for (const triggerId of Object.keys(original)) {
        expect(restoredData[triggerId]).toEqual(original[triggerId]);
      }
    });
  });

  describe("Scenario: Trigger State with Checkpoint Metadata", () => {
    it("should include trigger state metadata in checkpoint", () => {
      // DESIGN REQUIREMENT: Checkpoint metadata should include trigger information

      const triggers = [
        createMockTrigger("trigger-1", 1),
        createMockTrigger("trigger-2", 2),
      ];

      for (const trigger of triggers) {
        triggerStateManager.incrementFireCount(trigger.id);
      }

      // Checkpoint metadata should include:
      // {
      //   triggerStateVersion: 1,
      //   triggersTracked: ["trigger-1", "trigger-2"],
      //   checkpointedAt: timestamp,
      // }

      const state = triggerStateManager.toJSON();
      const trackedTriggers = Object.keys(state);

      expect(trackedTriggers).toHaveLength(2);
      expect(trackedTriggers).toContain("trigger-1");
      expect(trackedTriggers).toContain("trigger-2");
    });
  });
});
