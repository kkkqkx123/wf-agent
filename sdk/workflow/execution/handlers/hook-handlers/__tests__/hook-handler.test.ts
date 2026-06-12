/**
 * Graph Hook Processor Unit Tests
 *
 * Tests for the executeHook function and its internal handler creation.
 */

import { describe, it, expect, beforeEach, vi, type Mock } from "vitest";
import type { NodeHook, StaticNode, HookType } from "@wf-agent/types";
import type { HookExecutionContext } from "../hook-handler.js";
import type { WorkflowExecutionEntity } from "../../../../entities/workflow-execution-entity.js";

// ---------------------------------------------------------------------------
// Mock all external dependencies
// IMPORTANT: vi.mock paths are resolved relative to the test file, NOT the
// source file. Since the test file is in __tests__/ (one level deeper than
// the source file), all relative paths need an extra ../ compared to the
// source file's imports.
// ---------------------------------------------------------------------------

// Create mock function references using vi.hoisted() for type-safe mock access
const mockFilterAndSortHooks = vi.hoisted(() => vi.fn());
const mockExecuteHooks = vi.hoisted(() => vi.fn());

// Source imports from: ../../../../core/hooks/index.js (from hook-handlers/)
// Test needs: ../../../../../core/hooks/index.js (from __tests__/)
vi.mock("../../../../../core/hooks/index.js", () => ({
  filterAndSortHooks: mockFilterAndSortHooks,
  executeHooks: mockExecuteHooks,
}));

// Source imports from: ../../../checkpoint/checkpoint-coordinator.js (from hook-handlers/)
// Test needs: ../../../../checkpoint/checkpoint-coordinator.js (from __tests__/)
vi.mock("../../../../checkpoint/checkpoint-coordinator.js", () => ({
  CheckpointCoordinator: {
    createNodeCheckpoint: vi.fn(),
  },
}));

vi.mock("@wf-agent/common-utils", async (importOriginal) => {
  const actual = await importOriginal() as Record<string, unknown>;
  return {
    ...actual,
    getErrorOrNew: vi.fn((e: unknown) => e),
  };
});

const mockExecuteWithInterruptionHandling = vi.hoisted(() => vi.fn());

// Source imports from: ../../../../core/utils/interruption/index.js (from hook-handlers/)
// Test needs: ../../../../../core/utils/interruption/index.js (from __tests__/)
vi.mock("../../../../../core/utils/interruption/index.js", () => ({
  executeWithInterruptionHandling: mockExecuteWithInterruptionHandling,
}));

const mockGetWorkflowInterruptionDescription = vi.hoisted(() => vi.fn());
const mockToWorkflowInterruptionResult = vi.hoisted(() => vi.fn());

// Source imports from: ../../utils/workflow-interruption-utils.js (from hook-handlers/, 2 up)
// Test needs: ../../../utils/workflow-interruption-utils.js (from __tests__/, 3 up)
vi.mock("../../../utils/workflow-interruption-utils.js", () => ({
  getWorkflowInterruptionDescription: mockGetWorkflowInterruptionDescription,
  toWorkflowInterruptionResult: mockToWorkflowInterruptionResult,
}));

// Source imports from: ../../../../utils/contextual-logger.js (from hook-handlers/)
// Test needs: ../../../../../utils/contextual-logger.js (from __tests__/)
// Shared logger mock - created before vi.mock so the source module captures it at import time
const sharedLogger = vi.hoisted(() => ({ info: vi.fn(), warn: vi.fn(), error: vi.fn() }));

vi.mock("../../../../../utils/contextual-logger.js", () => ({
  createContextualLogger: vi.fn(() => sharedLogger),
}));

// Same-directory imports from source: ./context-builder.js, ./event-emitter.js
// From test (__tests__/): ../context-builder.js, ../event-emitter.js
vi.mock("../context-builder.js", () => ({
  buildHookEvaluationContext: vi.fn(),
  convertToEvaluationContext: vi.fn(),
}));

vi.mock("../event-emitter.js", () => ({
  emitHookEvent: vi.fn(),
}));

// ---------------------------------------------------------------------------
// Subject under test
// ---------------------------------------------------------------------------

import { executeHook } from "../hook-handler.js";

describe("Graph Hook Processor - executeHook", () => {
  let mockEntity: WorkflowExecutionEntity;
  let mockNode: StaticNode;
  let mockEmitEvent: Mock;
  let baseContext: HookExecutionContext;
  let mockAbortSignal: AbortSignal;

  beforeEach(() => {
    vi.clearAllMocks();

    mockAbortSignal = new AbortController().signal;

    mockEntity = {
      id: "exec-1",
      getExecution: vi.fn().mockReturnValue({
        id: "exec-1",
        workflowId: "wf-1",
        output: { final: "result" },
      }),
      getInput: vi.fn().mockReturnValue({ inputKey: "inputVal" }),
      getAbortSignal: vi.fn().mockReturnValue(mockAbortSignal),
      messageHistoryManager: {
        getMessages: vi.fn().mockReturnValue([]),
      },
      variableStateManager: {
        getAllVariables: vi.fn().mockReturnValue({ var1: "val1" }),
      },
    } as unknown as WorkflowExecutionEntity;

    mockNode = {
      id: "node-1",
      type: "SCRIPT",
      name: "Test Script",
      config: {},
      metadata: {},
      hooks: undefined,
    } as StaticNode;

    mockEmitEvent = vi.fn().mockResolvedValue(undefined);

    baseContext = {
      workflowExecutionEntity: mockEntity,
      node: mockNode,
    } as HookExecutionContext;

    // Default: simulate successful execution
    mockExecuteWithInterruptionHandling.mockImplementation(
      async (operation: (signal: AbortSignal) => Promise<void>, signal?: AbortSignal) => {
        await operation(signal ?? new AbortController().signal);
        return { success: true as const, result: undefined };
      },
    );

    mockGetWorkflowInterruptionDescription.mockReturnValue(
      "Workflow execution paused at node: node-1",
    );
    mockToWorkflowInterruptionResult.mockImplementation(
      (result: unknown, nodeId: string) => ({ ...(result as object), nodeId }),
    );
  });

  // -----------------------------------------------------------------------
  // Early return scenarios
  // -----------------------------------------------------------------------

  it("should return early when node.hooks is undefined", async () => {
    mockNode.hooks = undefined;

    await executeHook(baseContext, "AFTER_EXECUTE" as any, mockEmitEvent);

    expect(mockFilterAndSortHooks).not.toHaveBeenCalled();
    expect(mockExecuteWithInterruptionHandling).not.toHaveBeenCalled();
  });

  it("should return early when node.hooks is empty array", async () => {
    mockNode.hooks = [];

    await executeHook(baseContext, "AFTER_EXECUTE" as any, mockEmitEvent);

    expect(mockFilterAndSortHooks).not.toHaveBeenCalled();
    expect(mockExecuteWithInterruptionHandling).not.toHaveBeenCalled();
  });

  it("should return early when filtered hooks are empty", async () => {
    mockNode.hooks = [
      { hookType: "BEFORE_EXECUTE", eventName: "hook.before" } as NodeHook,
    ];
    mockFilterAndSortHooks.mockReturnValue([]);

    await executeHook(baseContext, "AFTER_EXECUTE" as any, mockEmitEvent);

    expect(mockFilterAndSortHooks).toHaveBeenCalledWith(
      mockNode.hooks,
      "AFTER_EXECUTE",
    );
    expect(mockExecuteWithInterruptionHandling).not.toHaveBeenCalled();
  });

  // -----------------------------------------------------------------------
  // Successful execution
  // -----------------------------------------------------------------------

  it("should execute hooks with correct parameters on success", async () => {
    const hooks: NodeHook[] = [
      { hookType: "AFTER_EXECUTE" as HookType, eventName: "hook.after", enabled: true },
    ];
    mockNode.hooks = hooks;
    mockFilterAndSortHooks.mockReturnValue(hooks);

    await executeHook(baseContext, "AFTER_EXECUTE" as HookType, mockEmitEvent);

    expect(mockFilterAndSortHooks).toHaveBeenCalledWith(hooks, "AFTER_EXECUTE" as HookType);
    expect(mockExecuteWithInterruptionHandling).toHaveBeenCalledWith(
      expect.any(Function),
      mockAbortSignal,
    );
    expect(mockExecuteHooks).toHaveBeenCalledWith(
      hooks,
      baseContext,
      expect.any(Function),
      expect.any(Array),
      expect.any(Function),
      expect.objectContaining({
        parallel: true,
        continueOnError: true,
        warnOnConditionFailure: true,
        abortSignal: mockAbortSignal,
      }),
    );
  });

  it("should pass the abort signal from entity to executeWithInterruptionHandling", async () => {
    const hooks: NodeHook[] = [
      { hookType: "AFTER_EXECUTE" as HookType, eventName: "hook.after", enabled: true },
    ];
    mockNode.hooks = hooks;
    mockFilterAndSortHooks.mockReturnValue(hooks);

    await executeHook(baseContext, "AFTER_EXECUTE" as HookType, mockEmitEvent);

    expect(mockExecuteWithInterruptionHandling).toHaveBeenCalledWith(
      expect.any(Function),
      mockAbortSignal,
    );
  });

  // -----------------------------------------------------------------------
  // Interruption handling
  // -----------------------------------------------------------------------

  it("should throw InterruptionError when execution is interrupted (paused)", async () => {
    const hooks: NodeHook[] = [
      { hookType: "AFTER_EXECUTE" as HookType, eventName: "hook.after", enabled: true },
    ];
    mockNode.hooks = hooks;
    mockFilterAndSortHooks.mockReturnValue(hooks);
    mockExecuteWithInterruptionHandling.mockResolvedValue({
      success: false as const,
      interruption: { type: "paused" },
    });

    await expect(
      executeHook(baseContext, "AFTER_EXECUTE" as HookType, mockEmitEvent),
    ).rejects.toThrow(
      "Hook execution interrupted: Workflow execution paused at node: node-1",
    );

    expect(mockToWorkflowInterruptionResult).toHaveBeenCalledWith(
      { type: "paused" },
      "node-1",
    );
    expect(mockGetWorkflowInterruptionDescription).toHaveBeenCalledWith(
      expect.objectContaining({ type: "paused", nodeId: "node-1" }),
    );
  });

  it("should log interruption info when execution is interrupted (stopped)", async () => {
    const hooks: NodeHook[] = [
      { hookType: "BEFORE_EXECUTE" as HookType, eventName: "hook.before", enabled: true },
    ];
    mockNode.hooks = hooks;
    mockFilterAndSortHooks.mockReturnValue(hooks);
    mockExecuteWithInterruptionHandling.mockResolvedValue({
      success: false as const,
      interruption: { type: "stopped" },
    });

    await expect(
      executeHook(baseContext, "BEFORE_EXECUTE" as HookType, mockEmitEvent),
    ).rejects.toThrow();

    expect(sharedLogger.info).toHaveBeenCalledWith(
      "Hook execution interrupted",
      expect.objectContaining({
        executionId: "exec-1",
        nodeId: "node-1",
        hookType: "BEFORE_EXECUTE" as HookType,
        interruptionType: "stopped",
      }),
    );
  });

  // -----------------------------------------------------------------------
  // Handler integration (verify the handlers array is passed)
  // -----------------------------------------------------------------------

  it("should pass handlers array with 2 entries to executeHooks", async () => {
    const hooks: NodeHook[] = [
      { hookType: "AFTER_EXECUTE" as HookType, eventName: "hook.after", enabled: true },
    ];
    mockNode.hooks = hooks;
    mockFilterAndSortHooks.mockReturnValue(hooks);

    await executeHook(baseContext, "AFTER_EXECUTE" as HookType, mockEmitEvent);

    const calls = mockExecuteHooks.mock.calls;
    expect(calls.length).toBeGreaterThan(0);
    const handlersArg = calls[0]![3];
    expect(handlersArg).toBeInstanceOf(Array);
    expect(handlersArg).toHaveLength(2);
  });

  it("should pass context builder as 3rd argument to executeHooks", async () => {
    const hooks: NodeHook[] = [
      { hookType: "AFTER_EXECUTE" as HookType, eventName: "hook.after", enabled: true },
    ];
    mockNode.hooks = hooks;
    mockFilterAndSortHooks.mockReturnValue(hooks);

    await executeHook(baseContext, "AFTER_EXECUTE" as HookType, mockEmitEvent);

    const calls = mockExecuteHooks.mock.calls;
    expect(calls.length).toBeGreaterThan(0);
    const contextBuilderArg = calls[0]![2];
    expect(typeof contextBuilderArg).toBe("function");
  });
});