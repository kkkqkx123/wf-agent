/**
 * Unit tests for hook-handler.ts
 *
 * Tests the executeAgentHook function that filters, evaluates,
 * and executes agent hooks through the generic hook framework.
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import type { AgentLoopEntity } from "../../../../entities/agent-loop-entity.js";
import type { AgentHookTriggeredEvent } from "@wf-agent/types";

// Mock external dependencies
vi.mock("../../../../../core/hooks/index.js", () => ({
  filterAndSortHooks: vi.fn(),
  executeHooks: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("../../../../../utils/contextual-logger.js", () => ({
  createContextualLogger: () => ({
    debug: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
  }),
}));

vi.mock("../context-builder.js", () => ({
  buildAgentHookEvaluationContext: vi.fn(() => ({
    iteration: 3,
    maxIterations: 10,
    toolCallCount: 5,
    status: "RUNNING",
    error: undefined,
    config: { profileId: "test-profile", tools: [] },
    tools: {
      isAvailable: vi.fn(() => true),
      getAll: vi.fn(() => new Set()),
    },
    conversationManager: {
      getAllMessages: vi.fn(() => []),
      getMessages: vi.fn(() => []),
    },
  })),
  convertToEvaluationContext: vi.fn(() => ({
    input: { iteration: 3, maxIterations: 10, toolCallCount: 5, messages: [], lastMessage: null },
    output: { status: "RUNNING", error: undefined },
    variables: {},
  })),
}));

vi.mock("../event-emitter.js", () => ({
  emitAgentHookEvent: vi.fn().mockResolvedValue(undefined),
}));

import { executeAgentHook } from "../hook-handler.js";
import { filterAndSortHooks, executeHooks } from "../../../../../core/hooks/index.js";
import { ConversationSession } from "../../../../../core/messaging/conversation-session.js";
import { AgentStateCoordinator } from "../../../../state-managers/agent-state-coordinator.js";

describe("AgentHookHandler", () => {
  let mockEntity: AgentLoopEntity;
  let mockEmitEvent: (event: AgentHookTriggeredEvent) => Promise<void>;
  let mockStateCoordinator: AgentStateCoordinator;

  beforeEach(() => {
    vi.clearAllMocks();

    mockEntity = {
      id: "agent-loop-1",
      nodeId: "node-1",
      config: {
        profileId: "test-profile",
        hooks: [
          { hookType: "BEFORE_ITERATION", eventName: "iteration.start", enabled: true },
          { hookType: "AFTER_TOOL_CALL", eventName: "tool.after", enabled: true, weight: 5 },
        ],
      },
      state: {
        currentIteration: 3,
        toolCallCount: 5,
        status: "RUNNING",
        error: undefined,
      },
      isToolAvailable: vi.fn(() => true),
      getAvailableTools: vi.fn(() => new Set(["tool-1"])),
    } as unknown as AgentLoopEntity;

    const conversationSession = new ConversationSession();
    mockStateCoordinator = new AgentStateCoordinator({
      conversationManager: conversationSession,
    });

    mockEmitEvent = vi.fn() as unknown as (event: AgentHookTriggeredEvent) => Promise<void>;
  });

  it("should return early when config has no hooks", async () => {
    (mockEntity.config as Record<string, unknown>)["hooks"] = undefined;

    await executeAgentHook(mockEntity, "BEFORE_ITERATION", mockEmitEvent, mockStateCoordinator);

    expect(filterAndSortHooks).not.toHaveBeenCalled();
    expect(executeHooks).not.toHaveBeenCalled();
  });

  it("should return early when config has empty hooks", async () => {
    (mockEntity.config as Record<string, unknown>)["hooks"] = [];

    await executeAgentHook(mockEntity, "BEFORE_ITERATION", mockEmitEvent, mockStateCoordinator);

    expect(filterAndSortHooks).not.toHaveBeenCalled();
    expect(executeHooks).not.toHaveBeenCalled();
  });

  it("should return early when no hooks match the given type", async () => {
    vi.mocked(filterAndSortHooks).mockReturnValue([]);

    await executeAgentHook(mockEntity, "BEFORE_ITERATION", mockEmitEvent, mockStateCoordinator);

    expect(filterAndSortHooks).toHaveBeenCalledWith(mockEntity.config.hooks, "BEFORE_ITERATION");
    expect(executeHooks).not.toHaveBeenCalled();
  });

  it("should execute matching hooks with event emitter handler", async () => {
    const matchingHooks = [
      { hookType: "BEFORE_ITERATION", eventName: "iteration.start", enabled: true },
    ];
    vi.mocked(filterAndSortHooks).mockReturnValue(matchingHooks);

    await executeAgentHook(mockEntity, "BEFORE_ITERATION", mockEmitEvent, mockStateCoordinator);

    expect(filterAndSortHooks).toHaveBeenCalledWith(mockEntity.config.hooks, "BEFORE_ITERATION");
    expect(executeHooks).toHaveBeenCalledWith(
      matchingHooks,
      expect.objectContaining({
        workflowExecutionId: "agent-loop-1",
        entity: mockEntity,
      }),
      expect.any(Function),
      expect.any(Array),
      expect.any(Function),
      expect.objectContaining({
        parallel: true,
        continueOnError: true,
        warnOnConditionFailure: true,
      }),
    );
  });

  it("should pass tool call info to context and evaluation builder when provided", async () => {
    const matchingHooks = [
      { hookType: "AFTER_TOOL_CALL", eventName: "tool.after", enabled: true, weight: 5 },
    ];
    vi.mocked(filterAndSortHooks).mockReturnValue(matchingHooks);

    const toolCallInfo = {
      id: "tool-call-1",
      name: "calculator",
      arguments: { a: 1, b: 2 },
      result: 3,
    };

    await executeAgentHook(mockEntity, "AFTER_TOOL_CALL", mockEmitEvent, mockStateCoordinator, toolCallInfo);

    expect(executeHooks).toHaveBeenCalledWith(
      matchingHooks,
      expect.objectContaining({
        toolCall: toolCallInfo,
      }),
      expect.any(Function),
      expect.any(Array),
      expect.any(Function),
      expect.any(Object),
    );
  });

  it("should pass llm response info to context when provided", async () => {
    const matchingHooks = [{ hookType: "AFTER_LLM_CALL", eventName: "llm.after", enabled: true }];
    vi.mocked(filterAndSortHooks).mockReturnValue(matchingHooks);

    const llmResponse = {
      content: "Hello!",
      toolCalls: [{ id: "tc-1", name: "search" }],
    };

    await executeAgentHook(mockEntity, "AFTER_LLM_CALL", mockEmitEvent, mockStateCoordinator, undefined, llmResponse);

    expect(executeHooks).toHaveBeenCalledWith(
      matchingHooks,
      expect.objectContaining({
        llmResponse,
      }),
      expect.any(Function),
      expect.any(Array),
      expect.any(Function),
      expect.any(Object),
    );
  });

  it("should integrate with event emitter handler correctly", async () => {
    const matchingHooks = [
      { hookType: "BEFORE_ITERATION", eventName: "iteration.start", enabled: true },
    ];
    vi.mocked(filterAndSortHooks).mockReturnValue(matchingHooks);

    await executeAgentHook(mockEntity, "BEFORE_ITERATION", mockEmitEvent);

    // The executeHooks should have been called with an array of handlers
    // where at least one is the event emitter handler
    const handlersArg = vi.mocked(executeHooks).mock.calls[0]![3] as unknown[];
    expect(handlersArg).toHaveLength(1);
    expect(handlersArg[0]).toBeInstanceOf(Function);
  });
});
