/**
 * Hook Executor Tests
 *
 * Tests for the universal hook execution logic.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import type {
  BaseHookDefinition,
  BaseHookContext,
  HookHandler,
  EventEmitter,
  ContextBuilder,
} from "../executor.js";
import {
  filterAndSortHooks,
  evaluateHookCondition,
  executeSingleHook,
  executeHooks,
  resolvePayloadTemplate,
} from "../executor.js";

// ── Hoisted Mocks ─────────────────────────────────────────────────────────
// vi.mock factories are hoisted, so we define mock functions via vi.hoisted.

const { mockConditionEvaluatorEvaluate, mockExpressionEvaluatorEvaluate, mockNow, mockGetErrorMessage, mockBuildHookExecutedEvent, mockLoggerWarn, mockChildLogger, mockCheckExecutionInterruption, mockShouldContinueExecution } = vi.hoisted(() => {
  const condEval = vi.fn();
  const exprEval = vi.fn();
  const nowFn = vi.fn();
  const getErrMsg = vi.fn((e: unknown) =>
    e instanceof Error ? e.message : String(e),
  );
  const buildEvent = vi.fn((params: unknown) => ({
    type: "HOOK_EXECUTED",
    timestamp: Date.now(),
    ...(params as Record<string, unknown>),
  }));
  const loggerWarn = vi.fn();
  const childLog = {
    warn: loggerWarn,
    info: vi.fn(),
    error: vi.fn(),
    debug: vi.fn(),
  };
  const checkInterrupt = vi.fn();
  const shouldContinue = vi.fn();
  return {
    mockConditionEvaluatorEvaluate: condEval,
    mockExpressionEvaluatorEvaluate: exprEval,
    mockNow: nowFn,
    mockGetErrorMessage: getErrMsg,
    mockBuildHookExecutedEvent: buildEvent,
    mockLoggerWarn: loggerWarn,
    mockChildLogger: childLog,
    mockCheckExecutionInterruption: checkInterrupt,
    mockShouldContinueExecution: shouldContinue,
  };
});

vi.mock("../../../workflow/evaluation/index.js", () => ({
  conditionEvaluator: { evaluate: mockConditionEvaluatorEvaluate },
  expressionEvaluator: { evaluate: mockExpressionEvaluatorEvaluate },
}));

vi.mock("@wf-agent/common-utils", () => ({
  getGlobalLogger: () => ({ child: () => mockChildLogger }),
  getErrorMessage: (...args: unknown[]) => mockGetErrorMessage(...args),
  now: (...args: unknown[]) => mockNow(...args),
  createPackageLogger: vi.fn(),
  registerLogger: vi.fn(),
  getLogLevelFromEnv: vi.fn(() => "silent"),
}));

vi.mock("../../utils/event/builders/index.js", () => ({
  buildHookExecutedEvent: (...args: unknown[]) =>
    mockBuildHookExecutedEvent(...args),
}));

vi.mock("../../../utils/logger.js", () => ({
  sdkLogger: { warn: mockLoggerWarn } as Record<string, unknown>,
  configureSDKLogger: vi.fn(),
  initializeSDKLogger: vi.fn(),
  createSDKModuleLogger: vi.fn(),
}));

vi.mock("../../utils/interruption/index.js", () => ({
  checkExecutionInterruption: (...args: unknown[]) =>
    mockCheckExecutionInterruption(...args),
  shouldContinueExecution: (...args: unknown[]) =>
    mockShouldContinueExecution(...args),
}));

// ── Test Helpers ───────────────────────────────────────────────────────────

interface TestHookContext extends BaseHookContext {
  variables?: Record<string, unknown>;
  input?: Record<string, unknown>;
  output?: Record<string, unknown>;
}

const buildEvalContext: ContextBuilder<TestHookContext> = (
  context: TestHookContext,
): Record<string, unknown> => ({
  variables: context.variables || {},
  input: context.input || {},
  output: context.output || {},
});

function createHandlerMock(): HookHandler<TestHookContext> {
  return vi.fn().mockResolvedValue(undefined);
}

function createEventEmitterMock(): EventEmitter {
  return vi.fn().mockResolvedValue(undefined);
}

// ── Tests ──────────────────────────────────────────────────────────────────

beforeEach(() => {
  vi.clearAllMocks();
  mockNow.mockReturnValue(100);
  mockConditionEvaluatorEvaluate.mockReturnValue(true);
  mockExpressionEvaluatorEvaluate.mockImplementation(
    (path: string) => `resolved:${path}`,
  );
  mockCheckExecutionInterruption.mockReturnValue({ type: "continue" });
  mockShouldContinueExecution.mockReturnValue(true);
});

// ── filterAndSortHooks ────────────────────────────────────────────────────

describe("filterAndSortHooks", () => {
  it("should filter hooks by hookType", () => {
    const hooks: BaseHookDefinition[] = [
      { hookType: "before", eventName: "a", enabled: true },
      { hookType: "after", eventName: "b", enabled: true },
      { hookType: "before", eventName: "c", enabled: true },
    ];
    const result = filterAndSortHooks(hooks, "before");
    expect(result).toHaveLength(2);
    expect(result[0]!.eventName).toBe("a");
    expect(result[1]!.eventName).toBe("c");
  });

  it("should exclude disabled hooks", () => {
    const hooks: BaseHookDefinition[] = [
      { hookType: "before", eventName: "a", enabled: true },
      { hookType: "before", eventName: "b", enabled: false },
      { hookType: "before", eventName: "c" },
    ];
    const result = filterAndSortHooks(hooks, "before");
    expect(result).toHaveLength(2);
    expect(result.map((h) => h.eventName)).toEqual(["a", "c"]);
  });

  it("should sort by weight descending", () => {
    const hooks: BaseHookDefinition[] = [
      { hookType: "before", eventName: "low", weight: 10 },
      { hookType: "before", eventName: "high", weight: 100 },
      { hookType: "before", eventName: "mid", weight: 50 },
    ];
    const result = filterAndSortHooks(hooks, "before");
    expect(result.map((h) => h.eventName)).toEqual(["high", "mid", "low"]);
  });

  it("should treat hooks without weight as 0", () => {
    const hooks: BaseHookDefinition[] = [
      { hookType: "before", eventName: "a", weight: 5 },
      { hookType: "before", eventName: "b" },
      { hookType: "before", eventName: "c", weight: -1 },
    ];
    const result = filterAndSortHooks(hooks, "before");
    expect(result.map((h) => h.eventName)).toEqual(["a", "b", "c"]);
  });

  it("should return empty array when no hooks match", () => {
    const hooks: BaseHookDefinition[] = [
      { hookType: "after", eventName: "a" },
    ];
    const result = filterAndSortHooks(hooks, "before");
    expect(result).toEqual([]);
  });
});

// ── evaluateHookCondition ─────────────────────────────────────────────────

describe("evaluateHookCondition", () => {
  it("should return true when hook has no condition", () => {
    const hook: BaseHookDefinition = { hookType: "test", eventName: "e1" };
    const result = evaluateHookCondition(hook, {});
    expect(result).toBe(true);
  });

  it("should evaluate condition via conditionEvaluator", () => {
    const condition = { expression: "output.result > 0" };
    const hook: BaseHookDefinition = {
      hookType: "test",
      eventName: "e1",
      condition,
    };
    mockConditionEvaluatorEvaluate.mockReturnValue(true);

    const result = evaluateHookCondition(hook, {
      output: { result: 42 },
    });

    expect(result).toBe(true);
    expect(mockConditionEvaluatorEvaluate).toHaveBeenCalledWith(condition, {
      variables: {},
      input: {},
      output: { result: 42 },
    });
  });

  it("should return false when condition evaluation fails", () => {
    const hook: BaseHookDefinition = {
      hookType: "test",
      eventName: "e1",
      condition: { expression: "broken" },
    };
    mockConditionEvaluatorEvaluate.mockImplementation(() => {
      throw new Error("evaluation error");
    });

    const result = evaluateHookCondition(hook, {});
    expect(result).toBe(false);
    expect(mockLoggerWarn).toHaveBeenCalled();
  });

  it("should skip warning when warnOnFailure is false", () => {
    const hook: BaseHookDefinition = {
      hookType: "test",
      eventName: "e1",
      condition: { expression: "broken" },
    };
    mockConditionEvaluatorEvaluate.mockImplementation(() => {
      throw new Error("evaluation error");
    });

    const result = evaluateHookCondition(hook, {}, false);
    expect(result).toBe(false);
    expect(mockLoggerWarn).not.toHaveBeenCalled();
  });
});

// ── executeSingleHook ──────────────────────────────────────────────────────

describe("executeSingleHook", () => {
  it("should skip execution when condition is not met", async () => {
    const hook: BaseHookDefinition = {
      hookType: "test",
      eventName: "e1",
      condition: { expression: "false" },
    };
    mockConditionEvaluatorEvaluate.mockReturnValue(false);

    const handler = createHandlerMock();
    const emitEvent = createEventEmitterMock();

    const result = await executeSingleHook(
      hook,
      { variables: {} },
      buildEvalContext,
      [handler],
      emitEvent,
    );

    expect(result.success).toBe(true);
    expect(result.data).toEqual({ skipped: true, reason: "condition_not_met" });
    expect(handler).not.toHaveBeenCalled();
    expect(emitEvent).not.toHaveBeenCalled();
  });

  it("should execute handlers and emit event on success", async () => {
    const hook: BaseHookDefinition = {
      hookType: "test",
      eventName: "e1",
    };
    const context: TestHookContext = { variables: { foo: "bar" } };
    const handler = createHandlerMock();
    const emitEvent = createEventEmitterMock();

    const result = await executeSingleHook(
      hook,
      context,
      buildEvalContext,
      [handler],
      emitEvent,
    );

    expect(result.success).toBe(true);
    expect(result.eventName).toBe("e1");
    expect(handler).toHaveBeenCalledTimes(1);
    expect(emitEvent).toHaveBeenCalledTimes(1);
    expect(mockBuildHookExecutedEvent).toHaveBeenCalled();
  });

  it("should inject abortSignal from config if not in context", async () => {
    const abortSignal = new AbortController().signal;
    const hook: BaseHookDefinition = {
      hookType: "test",
      eventName: "e1",
    };
    const handler = vi.fn().mockResolvedValue(undefined);
    const emitEvent = createEventEmitterMock();

    await executeSingleHook(
      hook,
      {},
      buildEvalContext,
      [handler],
      emitEvent,
      { abortSignal },
    );

    expect(handler).toHaveBeenCalledWith(
      expect.objectContaining({ abortSignal }),
      hook,
      expect.any(Object),
    );
  });

  it("should return error result when handler throws", async () => {
    mockNow.mockReturnValueOnce(100).mockReturnValueOnce(150);
    const hook: BaseHookDefinition = {
      hookType: "test",
      eventName: "e1",
    };
    const handler = vi.fn().mockRejectedValue(new Error("handler failed"));
    const emitEvent = createEventEmitterMock();

    const result = await executeSingleHook(
      hook,
      {},
      buildEvalContext,
      [handler],
      emitEvent,
    );

    expect(result.success).toBe(false);
    expect(result.eventName).toBe("e1");
    expect(result.error).toBeInstanceOf(Error);
    expect(result.error!.message).toBe("handler failed");
    expect(emitEvent).not.toHaveBeenCalled();
  });

  it("should resolve payload template and pass to handler", async () => {
    const hook: BaseHookDefinition = {
      hookType: "test",
      eventName: "e1",
      eventPayload: { custom: "{{variables.foo}}" },
    };
    mockExpressionEvaluatorEvaluate.mockReturnValue("resolved_value");

    const handler = vi.fn().mockResolvedValue(undefined);
    const emitEvent = createEventEmitterMock();

    await executeSingleHook(
      hook,
      { variables: { foo: "bar" } },
      buildEvalContext,
      [handler],
      emitEvent,
    );

    expect(handler).toHaveBeenCalledWith(
      expect.anything(),
      hook,
      expect.objectContaining({ custom: "resolved_value" }),
    );
  });
});

// ── executeHooks (Parallel Mode) ──────────────────────────────────────────

describe("executeHooks - parallel mode", () => {
  it("should execute all hooks in parallel and return results", async () => {
    const hooks: BaseHookDefinition[] = [
      { hookType: "test", eventName: "e1" },
      { hookType: "test", eventName: "e2" },
    ];
    const handler = createHandlerMock();
    const emitEvent = createEventEmitterMock();

    const results = await executeHooks(
      hooks,
      { variables: {} },
      buildEvalContext,
      [handler],
      emitEvent,
      { parallel: true },
    );

    expect(results).toHaveLength(2);
    expect(results[0]!.success).toBe(true);
    expect(results[1]!.success).toBe(true);
  });

  it("should throw error if already aborted before start", async () => {
    const abortSignal = { aborted: true } as AbortSignal;
    mockCheckExecutionInterruption.mockReturnValue({ type: "stopped" });
    mockShouldContinueExecution.mockReturnValue(false);

    const hooks: BaseHookDefinition[] = [
      { hookType: "test", eventName: "e1" },
    ];

    await expect(
      executeHooks(hooks, {}, buildEvalContext, [], () => Promise.resolve(), {
        parallel: true,
        abortSignal,
      }),
    ).rejects.toThrow("interrupted before start");
  });

  it("should mark results as interrupted when abort happens after execution", async () => {
    const abortSignal = { aborted: true } as AbortSignal;
    mockCheckExecutionInterruption.mockReturnValue({ type: "stopped" });
    // First call (pre-check): allow through; second call (post-check): interrupt
    mockShouldContinueExecution
      .mockReturnValueOnce(true)
      .mockReturnValueOnce(false);

    const handler = vi.fn().mockResolvedValue(undefined);

    const hooks: BaseHookDefinition[] = [
      { hookType: "test", eventName: "e1" },
      { hookType: "test", eventName: "e2" },
    ];

    const results = await executeHooks(
      hooks,
      { variables: {} },
      buildEvalContext,
      [handler],
      () => Promise.resolve(),
      { parallel: true, abortSignal },
    );

    expect(results).toHaveLength(2);
    for (const r of results) {
      expect(r.success).toBe(false);
      expect(r.error!.message).toContain("interrupted");
    }
  });

  it("should handle rejected promises from hooks", async () => {
    mockNow.mockReturnValue(100);

    const hooks: BaseHookDefinition[] = [
      { hookType: "test", eventName: "e1" },
      { hookType: "test", eventName: "e2" },
    ];

    const handler = vi
      .fn()
      .mockResolvedValueOnce(undefined)
      .mockRejectedValueOnce(new Error("oops"));

    const results = await executeHooks(
      hooks,
      {},
      buildEvalContext,
      [handler],
      () => Promise.resolve(),
      { parallel: true },
    );

    expect(results).toHaveLength(2);
    expect(results[0]!.success).toBe(true);
    expect(results[1]!.success).toBe(false);
    expect(results[1]!.error?.message).toBe("oops");
  });
});

// ── executeHooks (Serial Mode) ────────────────────────────────────────────

describe("executeHooks - serial mode", () => {
  it("should execute hooks sequentially", async () => {
    const executionOrder: string[] = [];
    const handler: HookHandler<TestHookContext> = vi
      .fn()
      .mockImplementation(async (_ctx, hook) => {
        executionOrder.push(hook.eventName);
      });

    const hooks: BaseHookDefinition[] = [
      { hookType: "test", eventName: "e1", weight: 100 },
      { hookType: "test", eventName: "e2", weight: 50 },
    ];

    const results = await executeHooks(
      hooks,
      {},
      buildEvalContext,
      [handler],
      () => Promise.resolve(),
      { parallel: false },
    );

    expect(executionOrder).toEqual(["e1", "e2"]);
    expect(results).toHaveLength(2);
  });

  it("should stop on first failure when continueOnError is false", async () => {
    mockNow.mockReturnValue(100);
    const handler = vi
      .fn()
      .mockResolvedValueOnce(undefined)
      .mockRejectedValueOnce(new Error("fail"))
      .mockResolvedValueOnce(undefined);

    const hooks: BaseHookDefinition[] = [
      { hookType: "test", eventName: "e1" },
      { hookType: "test", eventName: "e2" },
      { hookType: "test", eventName: "e3" },
    ];

    const results = await executeHooks(
      hooks,
      {},
      buildEvalContext,
      [handler],
      () => Promise.resolve(),
      { parallel: false, continueOnError: false },
    );

    expect(results).toHaveLength(2);
    expect(results[0]!.success).toBe(true);
    expect(results[1]!.success).toBe(false);
  });

  it("should continue on error in serial mode by default", async () => {
    mockNow.mockReturnValue(100);
    const handler = vi
      .fn()
      .mockResolvedValueOnce(undefined)
      .mockRejectedValueOnce(new Error("fail"))
      .mockResolvedValueOnce(undefined);

    const hooks: BaseHookDefinition[] = [
      { hookType: "test", eventName: "e1" },
      { hookType: "test", eventName: "e2" },
      { hookType: "test", eventName: "e3" },
    ];

    const results = await executeHooks(
      hooks,
      {},
      buildEvalContext,
      [handler],
      () => Promise.resolve(),
      { parallel: false },
    );

    expect(results).toHaveLength(3);
    expect(results[0]!.success).toBe(true);
    expect(results[1]!.success).toBe(false);
    expect(results[2]!.success).toBe(true);
  });

  it("should throw when aborted before serial hook", async () => {
    const abortSignal = { aborted: true } as AbortSignal;
    mockCheckExecutionInterruption.mockReturnValue({ type: "stopped" });
    mockShouldContinueExecution.mockReturnValue(false);

    const hooks: BaseHookDefinition[] = [
      { hookType: "test", eventName: "e1" },
    ];

    await expect(
      executeHooks(hooks, {}, buildEvalContext, [], () => Promise.resolve(), {
        parallel: false,
        abortSignal,
      }),
    ).rejects.toThrow("interrupted");
  });
});

// ── resolvePayloadTemplate ────────────────────────────────────────────────

describe("resolvePayloadTemplate", () => {
  it("should skip 'handler' key", () => {
    const payload = { handler: "something", foo: "bar" };
    const result = resolvePayloadTemplate(payload, {});
    expect(result).not.toHaveProperty("handler");
    expect(result).toHaveProperty("foo");
  });

  it("should resolve string templates via expressionEvaluator", () => {
    mockExpressionEvaluatorEvaluate.mockImplementation(
      (path: string) => `value:${path}`,
    );
    const payload = { msg: "{{output.result}}" };
    const result = resolvePayloadTemplate(payload, {
      output: { result: "hello" },
    });
    expect(result).toEqual({ msg: "value:output.result" });
  });

  it("should pass through non-string non-object values", () => {
    const payload = { num: 42, flag: true, nil: null };
    const result = resolvePayloadTemplate(payload, {});
    expect(result).toEqual({ num: 42, flag: true, nil: null });
  });

  it("should recursively resolve nested objects", () => {
    mockExpressionEvaluatorEvaluate.mockImplementation(
      (path: string) => `v:${path}`,
    );
    const payload = {
      nested: {
        deep: "{{variables.x}}",
        keep: 123,
      },
    };
    const result = resolvePayloadTemplate(payload, { variables: { x: "y" } });
    expect(result).toEqual({
      nested: {
        deep: "v:variables.x",
        keep: 123,
      },
    });
  });

  it("should skip handler key in nested objects", () => {
    const payload = {
      config: {
        handler: "fn",
        value: "{{variables.x}}",
      },
    };
    mockExpressionEvaluatorEvaluate.mockReturnValue("resolved");
    const result = resolvePayloadTemplate(payload, { variables: { x: "y" } });
    expect(result).toEqual({
      config: {
        value: "resolved",
      },
    });
    expect(
      (result.config as Record<string, unknown>),
    ).not.toHaveProperty("handler");
  });

  it("should return empty record for empty payload", () => {
    const result = resolvePayloadTemplate({}, {});
    expect(result).toEqual({});
  });
});
