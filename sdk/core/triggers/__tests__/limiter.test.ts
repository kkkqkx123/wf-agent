import { describe, it, expect, beforeEach } from "vitest";
import {
  canTrigger,
  getTriggerStatus,
  incrementTriggerCount,
  resetTriggerCount,
  isTriggerExpired,
  getRemainingTriggers,
} from "../limiter.js";
import type { BaseTriggerDefinition } from "../types.js";

function createTrigger(overrides: Partial<BaseTriggerDefinition> = {}): BaseTriggerDefinition {
  return {
    id: "trigger-1",
    name: "test-trigger",
    condition: { eventType: "test.event" },
    action: { type: "test", parameters: {} },
    ...overrides,
  };
}

describe("canTrigger", () => {
  it("should return true for a default trigger", () => {
    const trigger = createTrigger();
    expect(canTrigger(trigger)).toBe(true);
  });

  it("should return false when trigger is disabled", () => {
    const trigger = createTrigger({ enabled: false });
    expect(canTrigger(trigger)).toBe(false);
  });

  it("should return false when maxTriggers is reached", () => {
    const trigger = createTrigger({ maxTriggers: 5, triggerCount: 5 });
    expect(canTrigger(trigger)).toBe(false);
  });

  it("should return false when triggerCount exceeds maxTriggers", () => {
    const trigger = createTrigger({ maxTriggers: 3, triggerCount: 10 });
    expect(canTrigger(trigger)).toBe(false);
  });

  it("should return true when triggerCount is below maxTriggers", () => {
    const trigger = createTrigger({ maxTriggers: 5, triggerCount: 3 });
    expect(canTrigger(trigger)).toBe(true);
  });

  it("should return true when maxTriggers is 0 (no limit)", () => {
    const trigger = createTrigger({ maxTriggers: 0, triggerCount: 100 });
    expect(canTrigger(trigger)).toBe(true);
  });

  it("should return true when maxTriggers is undefined (no limit)", () => {
    const trigger = createTrigger({ maxTriggers: undefined, triggerCount: 100 });
    expect(canTrigger(trigger)).toBe(true);
  });

  it("should treat undefined triggerCount as 0", () => {
    const trigger = createTrigger({ maxTriggers: 5, triggerCount: undefined });
    expect(canTrigger(trigger)).toBe(true);
  });
});

describe("getTriggerStatus", () => {
  it("should return 'idle' for a fresh trigger", () => {
    const trigger = createTrigger();
    expect(getTriggerStatus(trigger)).toBe("idle");
  });

  it("should return 'disabled' when trigger is disabled", () => {
    const trigger = createTrigger({ enabled: false });
    expect(getTriggerStatus(trigger)).toBe("disabled");
  });

  it("should return 'expired' when maxTriggers is reached", () => {
    const trigger = createTrigger({ maxTriggers: 3, triggerCount: 3 });
    expect(getTriggerStatus(trigger)).toBe("expired");
  });

  it("should return 'triggered' when trigger has been triggered before", () => {
    const trigger = createTrigger({ triggerCount: 1 });
    expect(getTriggerStatus(trigger)).toBe("triggered");
  });

  it("should return 'triggered' when triggerCount > 1", () => {
    const trigger = createTrigger({ triggerCount: 5 });
    expect(getTriggerStatus(trigger)).toBe("triggered");
  });

  it("should return 'expired' even when triggerCount > maxTriggers", () => {
    const trigger = createTrigger({ maxTriggers: 3, triggerCount: 5 });
    expect(getTriggerStatus(trigger)).toBe("expired");
  });

  it("should prioritize disabled over expired", () => {
    const trigger = createTrigger({ enabled: false, maxTriggers: 3, triggerCount: 3 });
    expect(getTriggerStatus(trigger)).toBe("disabled");
  });
});

describe("incrementTriggerCount", () => {
  it("should increment from 0 to 1 when triggerCount is undefined", () => {
    const trigger = createTrigger({ triggerCount: undefined });
    const result = incrementTriggerCount(trigger);
    expect(result).toBe(1);
    expect(trigger.triggerCount).toBe(1);
  });

  it("should increment by 1", () => {
    const trigger = createTrigger({ triggerCount: 5 });
    const result = incrementTriggerCount(trigger);
    expect(result).toBe(6);
    expect(trigger.triggerCount).toBe(6);
  });

  it("should mutate the trigger object", () => {
    const trigger = createTrigger({ triggerCount: 0 });
    incrementTriggerCount(trigger);
    expect(trigger.triggerCount).toBe(1);
  });
});

describe("resetTriggerCount", () => {
  it("should reset triggerCount to 0", () => {
    const trigger = createTrigger({ triggerCount: 10 });
    resetTriggerCount(trigger);
    expect(trigger.triggerCount).toBe(0);
  });

  it("should set undefined triggerCount to 0", () => {
    const trigger = createTrigger({ triggerCount: undefined });
    resetTriggerCount(trigger);
    expect(trigger.triggerCount).toBe(0);
  });
});

describe("isTriggerExpired", () => {
  it("should return false when maxTriggers is undefined", () => {
    const trigger = createTrigger({ maxTriggers: undefined, triggerCount: 100 });
    expect(isTriggerExpired(trigger)).toBe(false);
  });

  it("should return false when maxTriggers is 0 (unlimited)", () => {
    const trigger = createTrigger({ maxTriggers: 0, triggerCount: 100 });
    expect(isTriggerExpired(trigger)).toBe(false);
  });

  it("should return false when below maxTriggers", () => {
    const trigger = createTrigger({ maxTriggers: 10, triggerCount: 5 });
    expect(isTriggerExpired(trigger)).toBe(false);
  });

  it("should return true when triggerCount equals maxTriggers", () => {
    const trigger = createTrigger({ maxTriggers: 5, triggerCount: 5 });
    expect(isTriggerExpired(trigger)).toBe(true);
  });

  it("should return true when triggerCount exceeds maxTriggers", () => {
    const trigger = createTrigger({ maxTriggers: 3, triggerCount: 7 });
    expect(isTriggerExpired(trigger)).toBe(true);
  });
});

describe("getRemainingTriggers", () => {
  it("should return -1 when maxTriggers is undefined (unlimited)", () => {
    const trigger = createTrigger({ maxTriggers: undefined });
    expect(getRemainingTriggers(trigger)).toBe(-1);
  });

  it("should return -1 when maxTriggers is 0 (unlimited)", () => {
    const trigger = createTrigger({ maxTriggers: 0 });
    expect(getRemainingTriggers(trigger)).toBe(-1);
  });

  it("should return remaining count", () => {
    const trigger = createTrigger({ maxTriggers: 10, triggerCount: 4 });
    expect(getRemainingTriggers(trigger)).toBe(6);
  });

  it("should return 0 when no triggers remaining", () => {
    const trigger = createTrigger({ maxTriggers: 5, triggerCount: 5 });
    expect(getRemainingTriggers(trigger)).toBe(0);
  });

  it("should return 0 when triggerCount exceeds maxTriggers (no negative)", () => {
    const trigger = createTrigger({ maxTriggers: 3, triggerCount: 10 });
    expect(getRemainingTriggers(trigger)).toBe(0);
  });

  it("should treat undefined triggerCount as 0", () => {
    const trigger = createTrigger({ maxTriggers: 5, triggerCount: undefined });
    expect(getRemainingTriggers(trigger)).toBe(5);
  });
});