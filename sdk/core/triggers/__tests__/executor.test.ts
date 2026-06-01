import { describe, it, expect, vi, beforeEach } from "vitest";
import type { BaseTriggerDefinition, TriggerExecutionResult } from "../types.js";

const { mockMatchTriggers } = vi.hoisted(() => {
  const matchTriggers = vi.fn();
  return { mockMatchTriggers: matchTriggers };
});

vi.mock("../matcher.js", () => ({
  matchTriggers: mockMatchTriggers,
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

import { executeTriggers } from "../executor.js";
import type { BaseEventData, TriggerHandler } from "../types.js";

function createTrigger(overrides: Partial<BaseTriggerDefinition> = {}): BaseTriggerDefinition {
  return {
    id: "trigger-1",
    name: "test-trigger",
    condition: { eventType: "test.event" },
    action: { type: "test", parameters: {} },
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

beforeEach(() => {
  vi.clearAllMocks();
});

describe("executeTriggers", () => {
  it("should execute matched triggers and return results", async () => {
    const triggers = [
      createTrigger({ id: "t1", action: { type: "webhook", parameters: { url: "http://example.com" } } }),
    ];
    const event = createEvent();
    const handler: TriggerHandler = vi.fn().mockResolvedValue({
      triggerId: "t1",
      success: true,
      action: triggers[0]!.action,
      executionTime: 10,
      result: { status: "ok" },
    } as TriggerExecutionResult);

    mockMatchTriggers.mockReturnValue(triggers);

    const results = await executeTriggers(triggers, event, handler);

    expect(results).toHaveLength(1);
    expect(results[0]!.triggerId).toBe("t1");
    expect(results[0]!.success).toBe(true);
    expect(results[0]!.result).toEqual({ status: "ok" });
    expect(handler).toHaveBeenCalledTimes(1);
  });

  it("should increment triggerCount after successful execution", async () => {
    const triggers = [
      createTrigger({ id: "t1", action: { type: "test", parameters: {} }, triggerCount: 0 }),
    ];
    const event = createEvent();
    const handler: TriggerHandler = vi.fn().mockResolvedValue({
      triggerId: "t1",
      success: true,
      action: triggers[0]!.action,
      executionTime: 5,
    } as TriggerExecutionResult);

    mockMatchTriggers.mockReturnValue(triggers);

    await executeTriggers(triggers, event, handler);

    expect(triggers[0]!.triggerCount).toBe(1);
  });

  it("should increment triggerCount cumulatively across multiple calls", async () => {
    const triggers = [
      createTrigger({ id: "t1", action: { type: "test", parameters: {} }, maxTriggers: 3 }),
    ];
    const event = createEvent();
    const handler: TriggerHandler = vi.fn().mockResolvedValue({
      triggerId: "t1",
      success: true,
      action: triggers[0]!.action,
      executionTime: 5,
    } as TriggerExecutionResult);

    mockMatchTriggers.mockReturnValue(triggers);

    await executeTriggers(triggers, event, handler);
    expect(triggers[0]!.triggerCount).toBe(1);

    // Second call: matchTriggers must still return the trigger
    mockMatchTriggers.mockReturnValue(triggers);
    await executeTriggers(triggers, event, handler);
    expect(triggers[0]!.triggerCount).toBe(2);

    mockMatchTriggers.mockReturnValue(triggers);
    await executeTriggers(triggers, event, handler);
    expect(triggers[0]!.triggerCount).toBe(3);
  });

  it("should NOT increment triggerCount when handler fails", async () => {
    const triggers = [
      createTrigger({ id: "t1", action: { type: "test", parameters: {} }, triggerCount: 5 }),
    ];
    const event = createEvent();
    const handler: TriggerHandler = vi.fn().mockRejectedValue(new Error("execution failed"));

    mockMatchTriggers.mockReturnValue(triggers);

    await executeTriggers(triggers, event, handler, { errorHandling: "silent" });

    expect(triggers[0]!.triggerCount).toBe(5);
  });

  it("should execute multiple matched triggers in order", async () => {
    const triggers = [
      createTrigger({ id: "t1", action: { type: "test", parameters: {} } }),
      createTrigger({ id: "t2", action: { type: "test", parameters: {} } }),
    ];
    const event = createEvent();
    const executionOrder: string[] = [];
    const handler: TriggerHandler = vi.fn().mockImplementation(async (trigger) => {
      executionOrder.push(trigger.id);
      return {
        triggerId: trigger.id,
        success: true,
        action: trigger.action,
        executionTime: 5,
      } as TriggerExecutionResult;
    });

    mockMatchTriggers.mockReturnValue(triggers);

    const results = await executeTriggers(triggers, event, handler);

    expect(results).toHaveLength(2);
    expect(executionOrder).toEqual(["t1", "t2"]);
  });

  it("should return empty results when no triggers match", async () => {
    const triggers = [createTrigger({ id: "t1" })];
    const event = createEvent();
    const handler: TriggerHandler = vi.fn();

    mockMatchTriggers.mockReturnValue([]);

    const results = await executeTriggers(triggers, event, handler);

    expect(results).toHaveLength(0);
    expect(handler).not.toHaveBeenCalled();
  });

  it("should handle handler error with 'log' strategy (default)", async () => {
    const triggers = [
      createTrigger({ id: "t1", action: { type: "test", parameters: {} } }),
      createTrigger({ id: "t2", action: { type: "test", parameters: {} } }),
    ];
    const event = createEvent();
    const handler: TriggerHandler = vi.fn().mockImplementation(async (trigger) => {
      if (trigger.id === "t1") {
        throw new Error("handler error");
      }
      return {
        triggerId: trigger.id,
        success: true,
        action: trigger.action,
        executionTime: 5,
      } as TriggerExecutionResult;
    });

    mockMatchTriggers.mockReturnValue(triggers);

    const results = await executeTriggers(triggers, event, handler);

    expect(results).toHaveLength(2);
    expect(results[0]!.success).toBe(false);
    expect(results[0]!.error).toBeDefined();
    expect(results[0]!.error!.toString()).toContain("handler error");
    expect(results[1]!.success).toBe(true);
  });

  it("should handle handler error with 'silent' strategy", async () => {
    const triggers = [createTrigger({ id: "t1", action: { type: "test", parameters: {} } })];
    const event = createEvent();
    const handler: TriggerHandler = vi.fn().mockRejectedValue(new Error("silent error"));

    mockMatchTriggers.mockReturnValue(triggers);

    const results = await executeTriggers(triggers, event, handler, { errorHandling: "silent" });

    expect(results).toHaveLength(1);
    expect(results[0]!.success).toBe(false);
    expect(results[0]!.error).toBeDefined();
  });

  it("should throw on handler error with 'throw' strategy", async () => {
    const triggers = [createTrigger({ id: "t1", action: { type: "test", parameters: {} } })];
    const event = createEvent();
    const handler: TriggerHandler = vi.fn().mockRejectedValue(new Error("fatal error"));

    mockMatchTriggers.mockReturnValue(triggers);

    await expect(
      executeTriggers(triggers, event, handler, { errorHandling: "throw" }),
    ).rejects.toThrow("fatal error");
  });

  it("should set executionTime to 0 for failed results", async () => {
    const triggers = [createTrigger({ id: "t1", action: { type: "test", parameters: {} } })];
    const event = createEvent();
    const handler: TriggerHandler = vi.fn().mockRejectedValue(new Error("error"));

    mockMatchTriggers.mockReturnValue(triggers);

    const results = await executeTriggers(triggers, event, handler, { errorHandling: "silent" });

    expect(results[0]!.executionTime).toBe(0);
  });

  it("should wrap non-Error thrown values in Error objects", async () => {
    const triggers = [createTrigger({ id: "t1", action: { type: "test", parameters: {} } })];
    const event = createEvent();
    const handler: TriggerHandler = vi.fn().mockRejectedValue("string error");

    mockMatchTriggers.mockReturnValue(triggers);

    const results = await executeTriggers(triggers, event, handler, { errorHandling: "silent" });

    expect(results[0]!.error).toBeInstanceOf(Error);
    expect(results[0]!.error!.toString()).toContain("string error");
  });

  it("should forward event data to the handler", async () => {
    const triggers = [createTrigger({ id: "t1", action: { type: "test", parameters: {} } })];
    const event = createEvent({ data: { key: "value" } });
    const handler: TriggerHandler = vi.fn().mockResolvedValue({
      triggerId: "t1",
      success: true,
      action: triggers[0]!.action,
      executionTime: 5,
    } as TriggerExecutionResult);

    mockMatchTriggers.mockReturnValue(triggers);

    await executeTriggers(triggers, event, handler);

    expect(handler).toHaveBeenCalledWith(triggers[0], event);
  });
});