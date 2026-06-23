import { describe, it, expect, beforeEach } from "vitest";
import { TriggerStateManager, type TriggerState } from "../trigger-state-manager.js";
import type { BaseTriggerDefinition } from "../types.js";

describe("TriggerStateManager", () => {
  let manager: TriggerStateManager;

  beforeEach(() => {
    manager = new TriggerStateManager();
  });

  describe("Basic State Operations", () => {
    it("should initialize with no state", () => {
      expect(manager.getState("trigger-1")).toBeUndefined();
    });

    it("should set and get trigger state", () => {
      const state: TriggerState = {
        triggerId: "trigger-1",
        fireCount: 3,
        lastFiredAt: Date.now(),
      };

      manager.setState("trigger-1", state);
      expect(manager.getState("trigger-1")).toEqual(state);
    });

    it("should update existing state", () => {
      const state1: TriggerState = {
        triggerId: "trigger-1",
        fireCount: 1,
      };

      manager.setState("trigger-1", state1);
      const state2: TriggerState = {
        triggerId: "trigger-1",
        fireCount: 2,
        lastFiredAt: Date.now(),
      };

      manager.setState("trigger-1", state2);
      expect(manager.getState("trigger-1")).toEqual(state2);
    });
  });

  describe("Fire Count Tracking", () => {
    it("should increment fire count", () => {
      const count = manager.incrementFireCount("trigger-1");
      expect(count).toBe(1);

      const count2 = manager.incrementFireCount("trigger-1");
      expect(count2).toBe(2);
    });

    it("should update lastFiredAt timestamp", () => {
      const before = Date.now();
      manager.incrementFireCount("trigger-1");
      const after = Date.now();

      const state = manager.getState("trigger-1");
      expect(state?.lastFiredAt).toBeGreaterThanOrEqual(before);
      expect(state?.lastFiredAt).toBeLessThanOrEqual(after);
    });

    it("should track multiple triggers independently", () => {
      manager.incrementFireCount("trigger-1");
      manager.incrementFireCount("trigger-1");
      manager.incrementFireCount("trigger-2");

      expect(manager.getState("trigger-1")?.fireCount).toBe(2);
      expect(manager.getState("trigger-2")?.fireCount).toBe(1);
    });
  });

  describe("Trigger Limit Checking", () => {
    it("should return false when no limit is set", () => {
      const trigger: BaseTriggerDefinition = {
        id: "trigger-1",
        condition: { eventType: "test" },
        action: { type: "log", parameters: {} },
      };

      expect(manager.hasReachedLimit(trigger)).toBe(false);
    });

    it("should return false when limit is not reached", () => {
      const trigger: BaseTriggerDefinition = {
        id: "trigger-1",
        condition: { eventType: "test" },
        action: { type: "log", parameters: {} },
        maxTriggers: 5,
      };

      manager.incrementFireCount("trigger-1");
      manager.incrementFireCount("trigger-1");

      expect(manager.hasReachedLimit(trigger)).toBe(false);
    });

    it("should return true when limit is reached", () => {
      const trigger: BaseTriggerDefinition = {
        id: "trigger-1",
        condition: { eventType: "test" },
        action: { type: "log", parameters: {} },
        maxTriggers: 3,
      };

      manager.incrementFireCount("trigger-1");
      manager.incrementFireCount("trigger-1");
      manager.incrementFireCount("trigger-1");

      expect(manager.hasReachedLimit(trigger)).toBe(true);
    });

    it("should return true when limit is exceeded", () => {
      const trigger: BaseTriggerDefinition = {
        id: "trigger-1",
        condition: { eventType: "test" },
        action: { type: "log", parameters: {} },
        maxTriggers: 2,
      };

      manager.incrementFireCount("trigger-1");
      manager.incrementFireCount("trigger-1");
      manager.incrementFireCount("trigger-1");

      expect(manager.hasReachedLimit(trigger)).toBe(true);
    });
  });

  describe("Remaining Triggers", () => {
    it("should return -1 when no limit is set", () => {
      const trigger: BaseTriggerDefinition = {
        id: "trigger-1",
        condition: { eventType: "test" },
        action: { type: "log", parameters: {} },
      };

      expect(manager.getRemainingTriggers(trigger)).toBe(-1);
    });

    it("should return correct remaining count", () => {
      const trigger: BaseTriggerDefinition = {
        id: "trigger-1",
        condition: { eventType: "test" },
        action: { type: "log", parameters: {} },
        maxTriggers: 5,
      };

      manager.incrementFireCount("trigger-1");
      expect(manager.getRemainingTriggers(trigger)).toBe(4);

      manager.incrementFireCount("trigger-1");
      expect(manager.getRemainingTriggers(trigger)).toBe(3);
    });

    it("should return 0 when limit is reached", () => {
      const trigger: BaseTriggerDefinition = {
        id: "trigger-1",
        condition: { eventType: "test" },
        action: { type: "log", parameters: {} },
        maxTriggers: 2,
      };

      manager.incrementFireCount("trigger-1");
      manager.incrementFireCount("trigger-1");

      expect(manager.getRemainingTriggers(trigger)).toBe(0);
    });

    it("should return 0 when limit is exceeded", () => {
      const trigger: BaseTriggerDefinition = {
        id: "trigger-1",
        condition: { eventType: "test" },
        action: { type: "log", parameters: {} },
        maxTriggers: 1,
      };

      manager.incrementFireCount("trigger-1");
      manager.incrementFireCount("trigger-1");

      expect(manager.getRemainingTriggers(trigger)).toBe(0);
    });
  });

  describe("Reset Operations", () => {
    it("should reset individual trigger state", () => {
      manager.incrementFireCount("trigger-1");
      manager.incrementFireCount("trigger-2");

      manager.reset("trigger-1");

      expect(manager.getState("trigger-1")).toBeUndefined();
      expect(manager.getState("trigger-2")).toBeDefined();
    });

    it("should reset all trigger state", () => {
      manager.incrementFireCount("trigger-1");
      manager.incrementFireCount("trigger-2");
      manager.incrementFireCount("trigger-3");

      manager.resetAll();

      expect(manager.getState("trigger-1")).toBeUndefined();
      expect(manager.getState("trigger-2")).toBeUndefined();
      expect(manager.getState("trigger-3")).toBeUndefined();
    });

    it("should allow state increment after reset", () => {
      manager.incrementFireCount("trigger-1");
      manager.reset("trigger-1");

      const count = manager.incrementFireCount("trigger-1");
      expect(count).toBe(1);
    });
  });

  describe("Serialization", () => {
    it("should serialize to JSON", () => {
      manager.incrementFireCount("trigger-1");
      manager.incrementFireCount("trigger-1");
      manager.incrementFireCount("trigger-2");

      const json = manager.toJSON();

      expect(json["trigger-1"]?.fireCount).toBe(2);
      expect(json["trigger-2"]?.fireCount).toBe(1);
      expect(json["trigger-1"]?.lastFiredAt).toBeDefined();
      expect(json["trigger-2"]?.lastFiredAt).toBeDefined();
    });

    it("should restore from JSON", () => {
      manager.incrementFireCount("trigger-1");
      manager.incrementFireCount("trigger-1");

      const json = manager.toJSON();
      const restored = TriggerStateManager.fromJSON(json);

      expect(restored.getState("trigger-1")?.fireCount).toBe(2);
      expect(restored.getState("trigger-1")?.lastFiredAt).toBe(
        manager.getState("trigger-1")?.lastFiredAt,
      );
    });

    it("should persist metadata during serialization", () => {
      const state: TriggerState = {
        triggerId: "trigger-1",
        fireCount: 1,
        metadata: {
          lastError: "Connection timeout",
          retryCount: 3,
        },
      };

      manager.setState("trigger-1", state);
      const json = manager.toJSON();
      const restored = TriggerStateManager.fromJSON(json);

      expect(restored.getState("trigger-1")?.metadata).toEqual(
        state.metadata,
      );
    });

    it("should handle empty state serialization", () => {
      const json = manager.toJSON();
      expect(json).toEqual({});

      const restored = TriggerStateManager.fromJSON(json);
      expect(restored.getState("trigger-1")).toBeUndefined();
    });
  });

  describe("Checkpoint Integration", () => {
    it("should support checkpoint save/restore cycle", () => {
      // Initial execution
      manager.incrementFireCount("trigger-1");
      manager.incrementFireCount("trigger-1");
      manager.incrementFireCount("trigger-2");

      // Simulate checkpoint save
      const checkpoint = manager.toJSON();

      // Simulate new execution restored from checkpoint
      const restored = TriggerStateManager.fromJSON(checkpoint);

      // Verify state was restored
      expect(restored.getState("trigger-1")?.fireCount).toBe(2);
      expect(restored.getState("trigger-2")?.fireCount).toBe(1);

      // Verify we can continue incrementing
      const count = restored.incrementFireCount("trigger-1");
      expect(count).toBe(3);
    });

    it("should preserve fire history across checkpoints", () => {
      // First execution phase
      manager.incrementFireCount("trigger-1");
      manager.incrementFireCount("trigger-1");
      const state1 = manager.getState("trigger-1")?.lastFiredAt;

      // Checkpoint and restore
      const checkpoint = manager.toJSON();
      const restored = TriggerStateManager.fromJSON(checkpoint);

      // Second execution phase
      restored.incrementFireCount("trigger-1");

      // Verify fire count accumulation
      expect(restored.getState("trigger-1")?.fireCount).toBe(3);
      // lastFiredAt should be updated
      expect(restored.getState("trigger-1")?.lastFiredAt).toBeGreaterThanOrEqual(
        state1!,
      );
    });
  });
});
