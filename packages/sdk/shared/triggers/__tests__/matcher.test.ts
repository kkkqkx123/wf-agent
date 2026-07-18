import { describe, it, expect, vi, beforeEach } from "vitest";
import type { BaseTriggerCondition, BaseEventData } from "../types.js";

const { mockConditionEvaluatorEvaluate } = vi.hoisted(() => {
  const evaluate = vi.fn();
  return { mockConditionEvaluatorEvaluate: evaluate };
});

vi.mock("../../../services/evaluation/index.js", () => ({
  conditionEvaluator: { evaluate: mockConditionEvaluatorEvaluate },
  cacheManager: { clear: vi.fn() },
}));

vi.mock("@wf-agent/common-utils", () => ({
  getGlobalLogger: () => ({
    child: () => ({
      debug: vi.fn(),
      warn: vi.fn(),
      info: vi.fn(),
      error: vi.fn(),
    }),
  }),
}));

import {
  defaultTriggerMatcher,
  matchTriggers,
  createTriggerMatcher,
  clearConditionCache,
} from "../matcher.js";
import type { BaseTriggerDefinition } from "../types.js";

function createCondition(overrides: Partial<BaseTriggerCondition> = {}): BaseTriggerCondition {
  return {
    eventType: "test.event",
    ...overrides,
  };
}

function createEvent(overrides: Partial<BaseEventData> = {}): BaseEventData {
  return {
    type: "test.event",
    timestamp: Date.now(),
    ...overrides,
  };
}

function createTrigger(overrides: Partial<BaseTriggerDefinition> = {}): BaseTriggerDefinition {
  return {
    id: "trigger-1",
    name: "test-trigger",
    condition: createCondition(),
    action: { type: "test", parameters: {} },
    ...overrides,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  clearConditionCache();
  mockConditionEvaluatorEvaluate.mockReset();
  mockConditionEvaluatorEvaluate.mockReturnValue(true);
});

describe("defaultTriggerMatcher", () => {
  it("should return true when eventType matches", () => {
    const condition = createCondition({ eventType: "test.event" });
    const event = createEvent({ type: "test.event" });
    expect(defaultTriggerMatcher(condition, event)).toBe(true);
  });

  it("should return false when eventType does not match", () => {
    const condition = createCondition({ eventType: "test.event" });
    const event = createEvent({ type: "other.event" });
    expect(defaultTriggerMatcher(condition, event)).toBe(false);
  });

  it("should return true when eventType matches and eventName matches", () => {
    const condition = createCondition({ eventType: "test.event", eventName: "create" });
    const event = createEvent({ type: "test.event", eventName: "create" });
    expect(defaultTriggerMatcher(condition, event)).toBe(true);
  });

  it("should return false when eventType matches but eventName does not", () => {
    const condition = createCondition({ eventType: "test.event", eventName: "create" });
    const event = createEvent({ type: "test.event", eventName: "update" });
    expect(defaultTriggerMatcher(condition, event)).toBe(false);
  });

  it("should skip eventName check when condition has no eventName", () => {
    const condition = createCondition({ eventType: "test.event", eventName: undefined });
    const event = createEvent({ type: "test.event", eventName: "anything" });
    expect(defaultTriggerMatcher(condition, event)).toBe(true);
  });

  it("should evaluate condition expression when present", () => {
    const condition = createCondition({
      eventType: "test.event",
      condition: { expression: "data.status == 'completed'" },
    });
    const event = createEvent({ type: "test.event", data: { status: "completed" } });
    mockConditionEvaluatorEvaluate.mockReturnValue(true);
    expect(defaultTriggerMatcher(condition, event)).toBe(true);
  });

  it("should return false when condition expression evaluates to false", () => {
    const condition = createCondition({
      eventType: "test.event",
      condition: { expression: "data.status == 'failed'" },
    });
    const event = createEvent({ type: "test.event", data: { status: "completed" } });
    mockConditionEvaluatorEvaluate.mockReturnValue(false);
    expect(defaultTriggerMatcher(condition, event)).toBe(false);
  });

  it("should return false when condition expression evaluation throws", () => {
    const condition = createCondition({
      eventType: "test.event",
      condition: { expression: "invalid syntax !@#" },
    });
    const event = createEvent({ type: "test.event" });
    mockConditionEvaluatorEvaluate.mockImplementation(() => {
      throw new Error("evaluation error");
    });
    expect(defaultTriggerMatcher(condition, event)).toBe(false);
  });

  it("should skip expression evaluation when condition has no expression", () => {
    const condition = createCondition({ eventType: "test.event", condition: undefined });
    const event = createEvent({ type: "test.event" });
    expect(defaultTriggerMatcher(condition, event)).toBe(true);
    expect(mockConditionEvaluatorEvaluate).not.toHaveBeenCalled();
  });
});

describe("matchTriggers", () => {
  it("should return matching triggers", () => {
    const triggers = [
      createTrigger({ id: "t1", condition: createCondition({ eventType: "test.event" }) }),
      createTrigger({ id: "t2", condition: createCondition({ eventType: "other.event" }) }),
      createTrigger({ id: "t3", condition: createCondition({ eventType: "test.event" }) }),
    ];
    const event = createEvent({ type: "test.event" });
    const result = matchTriggers(triggers, event);
    expect(result).toHaveLength(2);
    expect(result[0]!.id).toBe("t1");
    expect(result[1]!.id).toBe("t3");
  });

  it("should skip disabled triggers", () => {
    const triggers = [
      createTrigger({ id: "t1", condition: createCondition({ eventType: "test.event" }) }),
      createTrigger({
        id: "t2",
        condition: createCondition({ eventType: "test.event" }),
        enabled: false,
      }),
    ];
    const event = createEvent({ type: "test.event" });
    const result = matchTriggers(triggers, event);
    expect(result).toHaveLength(1);
    expect(result[0]!.id).toBe("t1");
  });

  it("should skip expired triggers (maxTriggers reached)", () => {
    const triggers = [
      createTrigger({ id: "t1", condition: createCondition({ eventType: "test.event" }) }),
      createTrigger({
        id: "t2",
        condition: createCondition({ eventType: "test.event" }),
        maxTriggers: 3,
        triggerCount: 3,
      }),
    ];
    const event = createEvent({ type: "test.event" });
    const result = matchTriggers(triggers, event);
    expect(result).toHaveLength(1);
    expect(result[0]!.id).toBe("t1");
  });

  it("should return empty array when no triggers match", () => {
    const triggers = [
      createTrigger({ id: "t1", condition: createCondition({ eventType: "other.event" }) }),
    ];
    const event = createEvent({ type: "test.event" });
    const result = matchTriggers(triggers, event);
    expect(result).toHaveLength(0);
  });

  it("should return empty array for empty triggers list", () => {
    const event = createEvent({ type: "test.event" });
    const result = matchTriggers([], event);
    expect(result).toHaveLength(0);
  });

  it("should use custom matcher when provided", () => {
    const customMatcher = vi.fn().mockReturnValue(true);
    const triggers = [
      createTrigger({ id: "t1", condition: createCondition({ eventType: "test.event" }) }),
    ];
    const event = createEvent({ type: "test.event" });
    const result = matchTriggers(triggers, event, customMatcher);
    expect(result).toHaveLength(1);
    expect(customMatcher).toHaveBeenCalledTimes(1);
  });
});

describe("createTriggerMatcher", () => {
  it("should use default-first order by default", () => {
    const customMatcher = vi.fn().mockReturnValue(true);
    const matcher = createTriggerMatcher(customMatcher);
    const condition = createCondition({ eventType: "test.event" });
    const event = createEvent({ type: "test.event" });
    expect(matcher(condition, event)).toBe(true);
  });

  it("should stop if default matcher fails in default-first mode", () => {
    const customMatcher = vi.fn().mockReturnValue(true);
    const matcher = createTriggerMatcher(customMatcher);
    const condition = createCondition({ eventType: "test.event" });
    const event = createEvent({ type: "other.event" });
    expect(matcher(condition, event)).toBe(false);
    expect(customMatcher).not.toHaveBeenCalled();
  });

  it("should run custom matcher first in custom-first mode", () => {
    const customMatcher = vi.fn().mockReturnValue(true);
    const matcher = createTriggerMatcher(customMatcher, { order: "custom-first" });
    const condition = createCondition({ eventType: "test.event" });
    const event = createEvent({ type: "test.event" });
    expect(matcher(condition, event)).toBe(true);
    expect(customMatcher).toHaveBeenCalledTimes(1);
  });

  it("should stop if custom matcher fails in custom-first mode", () => {
    const customMatcher = vi.fn().mockReturnValue(false);
    const matcher = createTriggerMatcher(customMatcher, { order: "custom-first" });
    const condition = createCondition({ eventType: "test.event" });
    const event = createEvent({ type: "test.event" });
    expect(matcher(condition, event)).toBe(false);
  });

  it("should use only custom matcher in custom-only mode", () => {
    const customMatcher = vi.fn().mockReturnValue(true);
    const matcher = createTriggerMatcher(customMatcher, { order: "custom-only" });
    const condition = createCondition({ eventType: "test.event" });
    const event = createEvent({ type: "other.event" });
    expect(matcher(condition, event)).toBe(true);
    expect(customMatcher).toHaveBeenCalledTimes(1);
  });
});

describe("clearConditionCache", () => {
  it("should clear the dependency manager cache without throwing", () => {
    expect(() => clearConditionCache()).not.toThrow();
  });
});
