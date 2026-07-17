/**
 * AgentIterationCoordinator Unit Tests
 *
 * Tests for single iteration execution:
 * - Sync iteration (with/without tool calls, interruption handling)
 * - Stream iteration (with/without tool calls, error handling)
 * - Event emission to registry
 * - Hook lifecycle during iteration
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import type { LLMResult, AgentHookTriggeredEvent } from "@wf-agent/types";
import type { EventRegistry } from "../../../../shared/registry/event-registry.js";
import type { MessageStream } from "../../../../services/llm/message-stream.js";
import type { LLMExecutionCoordinator as CoreLLMExecutionCoordinator } from "../../../../shared/coordinators/llm-execution-coordinator.js";
import type { AgentStateCoordinator } from "../../../state-managers/agent-state-coordinator.js";
import {
  AgentIterationCoordinator,
  type AgentLoopStreamEvent,
} from "../agent-iteration-coordinator.js";
import type { ToolExecutionCoordinator } from "../tool-execution-coordinator.js";

// Mock hook module
vi.mock("../../handlers/hook-handlers/index.js", () => ({
  executeAgentHook: vi.fn().mockResolvedValue(undefined),
}));

// Mock interruption utils
vi.mock("../../utils/agent-interruption-utils.js", () => ({
  checkAgentInterruption: vi.fn(),
  getAgentInterruptionDescription: vi.fn().mockReturnValue("Agent loop paused at iteration 0"),
}));

// Mock error handler
vi.mock("../../handlers/agent-error-handler.js", () => ({
  handleAgentError: vi.fn().mockResolvedValue(new Error("Standardized error")),
}));

// Mock event builders
vi.mock("../../../../shared/events/builders/agent-events.js", () => ({
  buildAgentStartedEvent: vi.fn(() => ({ type: "AGENT_STARTED" })),
  buildAgentCompletedEvent: vi.fn(() => ({ type: "AGENT_COMPLETED" })),
  buildAgentIterationCompletedEvent: vi.fn(() => ({ type: "AGENT_ITERATION_COMPLETED" })),
  buildAgentToolExecutionStartedEvent: vi.fn(() => ({ type: "AGENT_TOOL_EXECUTION_STARTED" })),
  buildAgentToolExecutionCompletedEvent: vi.fn(() => ({ type: "AGENT_TOOL_EXECUTION_COMPLETED" })),
}));

describe("AgentIterationCoordinator", () => {
  let coordinator: AgentIterationCoordinator;
let mockCoreCoordinator: CoreLLMExecutionCoordinator;
   let mockToolExecutionCoordinator: ToolExecutionCoordinator;
   let mockEmitAgentEvent: (event: AgentHookTriggeredEvent) => Promise<void>;
   let mockEventManager: EventRegistry;
   let mockStateCoordinator: AgentStateCoordinator;
   let mockEntity: any;
  let mockConversationManager: any;

  beforeEach(() => {
    vi.clearAllMocks();

    mockCoreCoordinator = {
      executeLLMCallWithMessages: vi.fn(),
      executeLLMStream: vi.fn(),
    } as unknown as CoreLLMExecutionCoordinator;

    mockToolExecutionCoordinator = {
      executeToolCalls: vi.fn().mockResolvedValue(undefined),
      executeToolCallsStream: vi.fn().mockImplementation(async function* () {}),
    } as unknown as ToolExecutionCoordinator;

    mockEmitAgentEvent = vi.fn().mockResolvedValue(undefined);

    mockEventManager = {
      emit: vi.fn().mockResolvedValue(undefined),
    } as unknown as EventRegistry;

    mockStateCoordinator = {
      getMessages: vi.fn().mockReturnValue([]),
      addMessage: vi.fn(),
      snapshot: vi.fn(),
    } as unknown as AgentStateCoordinator;

    mockEntity = {
      id: "agent-loop-1",
      nodeId: "node-1",
      config: {
        transformContext: undefined,
        violationPolicy: undefined,
      },
      state: {
        currentIteration: 0,
        toolCallCount: 0,
        startIteration: vi.fn(),
        endIteration: vi.fn(),
        complete: vi.fn(),
        recordToolCallStart: vi.fn(),
        recordToolCallEnd: vi.fn(),
      },
      getAbortSignal: vi.fn().mockReturnValue(new AbortController().signal),
      getLockedToolCallFormat: vi.fn().mockReturnValue(undefined),
      addMessage: vi.fn(),
    };

    mockConversationManager = {
      getMessages: vi.fn().mockReturnValue([]),
      getMessageCount: vi.fn().mockReturnValue(0),
      addAssistantMessage: vi.fn(),
      addMessage: vi.fn(),
    };

    coordinator = new AgentIterationCoordinator({
      coreCoordinator: mockCoreCoordinator,
      toolExecutionCoordinator: mockToolExecutionCoordinator,
      emitAgentEvent: mockEmitAgentEvent,
      eventManager: mockEventManager,
      stateCoordinator: mockStateCoordinator,
    });
  });

  describe("executeIteration (sync mode)", () => {
    it("should complete iteration when LLM returns final answer (no tool calls)", async () => {
      const { checkAgentInterruption } = await import("../../utils/agent-interruption-utils.js");
      (checkAgentInterruption as ReturnType<typeof vi.fn>).mockReturnValue({ type: "continue" });

      mockCoreCoordinator.executeLLMCallWithMessages = vi.fn().mockResolvedValue({
        content: "Final answer",
        toolCalls: [],
      } as unknown as LLMResult);

      const result = await coordinator.executeIteration(
        mockEntity,
        mockConversationManager,
        undefined,
        "profile-1",
      );

      expect(result.success).toBe(true);
      expect(result.shouldContinue).toBe(false);
      expect(result.content).toBe("Final answer");
      expect(mockEntity.state.endIteration).toHaveBeenCalledWith("Final answer");
      expect(mockEntity.state.complete).toHaveBeenCalledOnce();
      expect(mockConversationManager.addAssistantMessage).toHaveBeenCalledWith("Final answer", []);
    });

    it("should continue iteration when LLM returns tool calls", async () => {
      const { checkAgentInterruption } = await import("../../utils/agent-interruption-utils.js");
      (checkAgentInterruption as ReturnType<typeof vi.fn>).mockReturnValue({ type: "continue" });

      mockCoreCoordinator.executeLLMCallWithMessages = vi.fn().mockResolvedValue({
        content: "Let me search",
        toolCalls: [{ id: "tc-1", name: "search", arguments: '{"q":"test"}' }],
      } as unknown as LLMResult);

      mockToolExecutionCoordinator.executeToolCalls = vi.fn().mockResolvedValue(undefined);

      const result = await coordinator.executeIteration(
        mockEntity,
        mockConversationManager,
        undefined,
        "profile-1",
      );

      expect(result.success).toBe(true);
      expect(result.shouldContinue).toBe(true);
      expect(mockToolExecutionCoordinator.executeToolCalls).toHaveBeenCalledOnce();
    });

    it("should handle interruption before LLM call (paused)", async () => {
      const { checkAgentInterruption } = await import("../../utils/agent-interruption-utils.js");
      (checkAgentInterruption as ReturnType<typeof vi.fn>).mockReturnValue({
        type: "paused",
        iteration: 0,
      });

      const result = await coordinator.executeIteration(
        mockEntity,
        mockConversationManager,
        undefined,
        "profile-1",
      );

      expect(result.success).toBe(false);
      expect(result.shouldContinue).toBe(false);
      expect(result.interruption).toBeDefined();
      expect(mockCoreCoordinator.executeLLMCallWithMessages).not.toHaveBeenCalled();
    });

    it("should handle interruption after LLM call", async () => {
      const { checkAgentInterruption } = await import("../../utils/agent-interruption-utils.js");
      // First call returns continue (pre-check), second returns paused (post-check)
      (checkAgentInterruption as ReturnType<typeof vi.fn>)
        .mockReturnValueOnce({ type: "continue" })
        .mockReturnValueOnce({ type: "paused", iteration: 0 });

      mockCoreCoordinator.executeLLMCallWithMessages = vi.fn().mockResolvedValue({
        content: "Some response",
        toolCalls: [],
      } as unknown as LLMResult);

      const result = await coordinator.executeIteration(
        mockEntity,
        mockConversationManager,
        undefined,
        "profile-1",
      );

      expect(result.success).toBe(false);
      expect(result.shouldContinue).toBe(false);
      expect(result.interruption).toBeDefined();
    });

    it("should pass transformContext to core coordinator", async () => {
      const { checkAgentInterruption } = await import("../../utils/agent-interruption-utils.js");
      (checkAgentInterruption as ReturnType<typeof vi.fn>).mockReturnValue({ type: "continue" });

      const transformContext = vi.fn();
      mockEntity.config.transformContext = transformContext;
      mockCoreCoordinator.executeLLMCallWithMessages = vi.fn().mockResolvedValue({
        content: "Done",
        toolCalls: [],
      } as unknown as LLMResult);

      await coordinator.executeIteration(
        mockEntity,
        mockConversationManager,
        undefined,
        "profile-1",
      );

      expect(mockCoreCoordinator.executeLLMCallWithMessages).toHaveBeenCalledWith(
        [],
        expect.objectContaining({
          parameters: {},
          profileId: "profile-1",
          tools: undefined,
          lockedToolCallFormat: undefined,
          violationPolicy: undefined,
        }),
        expect.objectContaining({
          executionId: "agent-loop-1",
          nodeId: "node-1",
          messageCount: 0,
          currentIteration: 0,
        }),
        transformContext,
      );
    });
  });

  describe("executeIterationStream (stream mode)", () => {
    it("should stream iteration and complete without tool calls", async () => {
      const { checkAgentInterruption } = await import("../../utils/agent-interruption-utils.js");
      (checkAgentInterruption as ReturnType<typeof vi.fn>).mockReturnValue({ type: "continue" });

      const mockMessageStream = {
        on: vi.fn().mockReturnThis(),
        done: vi.fn().mockResolvedValue(undefined),
        getFinalResult: vi.fn().mockResolvedValue({
          content: "Stream complete",
          toolCalls: [],
        } as unknown as LLMResult),
        cleanup: vi.fn(),
      } as unknown as MessageStream;

      mockCoreCoordinator.executeLLMStream = vi.fn().mockResolvedValue(mockMessageStream);

      const gen = coordinator.executeIterationStream(
        mockEntity,
        mockConversationManager,
        undefined,
        "profile-1",
      );

      // Consume all yielded events
      const events: any[] = [];
      let result: IteratorResult<AgentLoopStreamEvent, boolean>;
      while (!(result = await gen.next()).done) {
        events.push(result.value);
      }

      // Events should have been yielded
      expect(events.length).toBeGreaterThan(0);

      // Iteration without tool calls returns false (should not continue)
      expect(result.done).toBe(true);
      expect(result.value).toBe(false);
    });

    it("should handle LLM stream error", async () => {
      mockCoreCoordinator.executeLLMStream = vi.fn().mockRejectedValue(new Error("LLM error"));

      const events: any[] = [];
      const gen = coordinator.executeIterationStream(
        mockEntity,
        mockConversationManager,
        undefined,
        "profile-1",
      );

      for await (const event of gen) {
        events.push(event);
      }

      expect(events.length).toBeGreaterThan(0);
      expect(events.some(e => e.type === "agent_error")).toBe(true);
    });

    it("should stream iteration with tool calls", async () => {
      const { checkAgentInterruption } = await import("../../utils/agent-interruption-utils.js");
      (checkAgentInterruption as ReturnType<typeof vi.fn>).mockReturnValue({ type: "continue" });

      const mockMessageStream = {
        on: vi.fn().mockReturnThis(),
        done: vi.fn().mockResolvedValue(undefined),
        getFinalResult: vi.fn().mockResolvedValue({
          content: "Using tools",
          toolCalls: [
            {
              id: "tc-1",
              type: "function",
              function: { name: "search", arguments: '{"q":"test"}' },
            },
          ],
        } as unknown as LLMResult),
        cleanup: vi.fn(),
      } as unknown as MessageStream;

      mockCoreCoordinator.executeLLMStream = vi.fn().mockResolvedValue(mockMessageStream);

      mockToolExecutionCoordinator.executeToolCallsStream = vi
        .fn()
        .mockImplementation(async function* () {
          yield { type: "tool_execution_start", toolCallId: "tc-1" };
          yield { type: "tool_execution_end", toolCallId: "tc-1" };
        });

      const gen = coordinator.executeIterationStream(
        mockEntity,
        mockConversationManager,
        undefined,
        "profile-1",
      );

      const events: any[] = [];
      for await (const event of gen) {
        events.push(event);
      }

      expect(mockToolExecutionCoordinator.executeToolCallsStream).toHaveBeenCalledOnce();
    });
  });

  describe("emitToRegistry", () => {
    it("should emit event to registry when eventManager is available", async () => {
      const { checkAgentInterruption } = await import("../../utils/agent-interruption-utils.js");
      (checkAgentInterruption as ReturnType<typeof vi.fn>).mockReturnValue({ type: "continue" });

      const event = {
        type: "iteration_complete" as const,
        timestamp: Date.now(),
        agentLoopId: "agent-loop-1",
        iteration: 1,
        shouldContinue: true,
      };

      await coordinator.emitToRegistry(event, mockEntity);

      expect(mockEventManager.emit).toHaveBeenCalledOnce();
    });

    it("should skip emission when no eventManager", async () => {
      const coordinatorWithoutEventManager = new AgentIterationCoordinator({
        coreCoordinator: mockCoreCoordinator,
        toolExecutionCoordinator: mockToolExecutionCoordinator,
        emitAgentEvent: mockEmitAgentEvent,
        stateCoordinator: mockStateCoordinator,
      });

      const event = {
        type: "iteration_complete" as const,
        timestamp: Date.now(),
        agentLoopId: "agent-loop-1",
        iteration: 1,
        shouldContinue: true,
      };

      await coordinatorWithoutEventManager.emitToRegistry(event, mockEntity);

      expect(mockEventManager.emit).not.toHaveBeenCalled();
    });

    it("should handle emission errors gracefully", async () => {
      mockEventManager.emit = vi.fn().mockRejectedValue(new Error("Registry error"));

      const event = {
        type: "iteration_complete" as const,
        timestamp: Date.now(),
        agentLoopId: "agent-loop-1",
        iteration: 1,
        shouldContinue: true,
      };

      await expect(coordinator.emitToRegistry(event, mockEntity)).resolves.not.toThrow();
    });
  });
});
