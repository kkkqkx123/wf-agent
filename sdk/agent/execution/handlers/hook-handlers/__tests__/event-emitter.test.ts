/**
 * Unit tests for event-emitter.ts
 *
 * Tests the emitAgentHookEvent function that builds and emits
 * AgentHookTriggeredEvent through the hook event system.
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import type { AgentLoopEntity } from "../../../../entities/agent-loop-entity.js";
import type { AgentHookTriggeredEvent } from "@wf-agent/types";

// Mock external dependencies
vi.mock("../../../../../core/utils/event/builders/index.js", () => ({
  buildAgentHookTriggeredEvent: vi.fn(params => ({
    id: "mock-event-id",
    type: "AGENT_HOOK_TRIGGERED",
    timestamp: 1000000,
    ...params,
  })),
}));

vi.mock("../../../../../core/utils/event/emit-hook-event.js", () => ({
  emitHookEventSafe: vi.fn().mockResolvedValue(undefined),
}));

import { emitAgentHookEvent } from "../event-emitter.js";
import { emitHookEventSafe } from "../../../../../core/utils/event/emit-hook-event.js";

describe("emitAgentHookEvent", () => {
  let mockEntity: AgentLoopEntity;
  let mockEmitEvent: (event: AgentHookTriggeredEvent) => Promise<void>;

  beforeEach(() => {
    vi.clearAllMocks();

    mockEntity = {
      id: "agent-loop-1",
      nodeId: "node-1",
      config: { profileId: "test-profile" },
      state: {
        currentIteration: 3,
        toolCallCount: 5,
      },
      getParentContext: vi.fn(() => undefined),
    } as unknown as AgentLoopEntity;

    mockEmitEvent = vi.fn() as unknown as (event: AgentHookTriggeredEvent) => Promise<void>;
  });

  it("should emit a hook triggered event with correct data", async () => {
    await emitAgentHookEvent(
      mockEntity,
      "BEFORE_ITERATION",
      "iteration.start",
      { customField: "value" },
      mockEmitEvent,
    );

    expect(emitHookEventSafe).toHaveBeenCalledOnce();

    const callArg = vi.mocked(emitHookEventSafe).mock.calls[0]![0] as unknown as Record<
      string,
      unknown
    >;
    expect(callArg["type"]).toBe("AGENT_HOOK_TRIGGERED");
    expect(callArg["hookType"]).toBe("BEFORE_ITERATION");
    expect(callArg["eventName"]).toBe("iteration.start");
    expect(callArg["eventData"]).toEqual({ customField: "value" });
    expect(callArg["iteration"]).toBe(3);
    expect(callArg["agentLoopId"]).toBe("agent-loop-1");
    expect(callArg["metadata"]).toEqual({
      profileId: "test-profile",
      toolCallCount: 5,
    });
  });

  it("should default eventData to empty object when undefined", async () => {
    await emitAgentHookEvent(mockEntity, "AFTER_TOOL_CALL", "tool.after", undefined, mockEmitEvent);

    const callArg = vi.mocked(emitHookEventSafe).mock.calls[0]![0] as unknown as Record<
      string,
      unknown
    >;
    expect(callArg["eventData"]).toEqual({});
  });

  it("should include parent context when entity has one", async () => {
    mockEntity.getParentContext = vi.fn(() => ({
      parentType: "WORKFLOW" as const,
      parentId: "wf-1",
      nodeId: "node-1",
    }));

    await emitAgentHookEvent(
      mockEntity,
      "BEFORE_LLM_CALL",
      "llm.before",
      { input: "test" },
      mockEmitEvent,
    );

    const callArg = vi.mocked(emitHookEventSafe).mock.calls[0]![0] as unknown as Record<
      string,
      unknown
    >;
    expect(callArg["parentContext"]).toEqual({
      parentType: "WORKFLOW",
      parentId: "wf-1",
      nodeId: "node-1",
    });
  });

  it("should include delegation purpose for AGENT_LOOP parent", async () => {
    mockEntity.getParentContext = vi.fn(() => ({
      parentType: "AGENT_LOOP" as const,
      parentId: "parent-agent",
      delegationPurpose: "Code review task",
    }));

    await emitAgentHookEvent(mockEntity, "AFTER_ITERATION", "iteration.after", {}, mockEmitEvent);

    const callArg = vi.mocked(emitHookEventSafe).mock.calls[0]![0] as unknown as Record<
      string,
      unknown
    >;
    expect(callArg["parentContext"]).toEqual({
      parentType: "AGENT_LOOP",
      parentId: "parent-agent",
      delegationPurpose: "Code review task",
    });
    expect((callArg["parentContext"] as Record<string, unknown>)["nodeId"]).toBeUndefined();
  });

  it("should pass the emitEvent function to emitHookEventSafe", async () => {
    await emitAgentHookEvent(mockEntity, "BEFORE_ITERATION", "iteration.start", {}, mockEmitEvent);

    const emitFnArg = vi.mocked(emitHookEventSafe).mock.calls[0]![1];
    expect(emitFnArg).toBe(mockEmitEvent);
  });
});
