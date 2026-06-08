/**
 * AgentExecutionCoordinator Unit Tests
 *
 * Tests for the agent loop execution coordinator covering:
 * - Synchronous execution flow (normal completion, max iterations, interruption, error)
 * - Streaming execution flow (normal, interruption, error)
 * - Metrics recording
 * - Event emission
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import type { AgentLoopEntity } from "../../../entities/agent-loop-entity.js";
import type { ConversationSession } from "../../../../core/messaging/conversation-session.js";
import type { MetricsRegistry } from "../../../../core/metrics/metrics-registry.js";
import type { AgentIterationCoordinator } from "../agent-iteration-coordinator.js";
import { AgentExecutionCoordinator } from "../agent-execution-coordinator.js";

// Store mock refs in a mutable object to avoid hoisting issues
const __mocks__ = {
  executeWithInterruptionHandling: vi.fn(),
  iterateWithInterruptionHandling: vi.fn(),
  handleAgentError: vi.fn().mockResolvedValue(new Error("Standardized error")),
};

vi.mock("../../../../core/utils/interruption/index.js", () => ({
  executeWithInterruptionHandling: (...args: any[]) => __mocks__.executeWithInterruptionHandling(...args),
  iterateWithInterruptionHandling: (...args: any[]) => __mocks__.iterateWithInterruptionHandling(...args),
}));

vi.mock("../../handlers/agent-error-handler.js", () => ({
  handleAgentError: (...args: any[]) => __mocks__.handleAgentError(...args),
}));

/**
 * Helper to wrap callback-based mock for executeWithInterruptionHandling.
 * The real executeWithInterruptionHandling returns { success: true, result: <T> }
 * when the callback runs without interruption, or { success: false, interruption: ... }
 * when interrupted. Tests that want the callback to run should use this wrapper.
 */
function wrapCallbackResult() {
  __mocks__.executeWithInterruptionHandling.mockImplementation(async (fn: Function) => {
    const innerResult = await fn(new AbortController().signal);
    return { success: true, result: innerResult };
  });
}

describe("AgentExecutionCoordinator", () => {
  let coordinator: AgentExecutionCoordinator;
  let mockIterationCoordinator: AgentIterationCoordinator;
  let mockMetricsRegistry: MetricsRegistry;
  let mockEntity: any;
  let mockConversationManager: ConversationSession;

  const defaultProfileId = "profile-1";
  const defaultMaxIterations = 10;

  beforeEach(() => {
    vi.clearAllMocks();
    __mocks__.executeWithInterruptionHandling.mockReset();
    __mocks__.iterateWithInterruptionHandling.mockReset();

    mockEntity = {
      id: "agent-loop-1",
      config: { agentConfigId: "agent-config-1" },
      state: {
        currentIteration: 0,
        toolCallCount: 0,
        start: vi.fn(),
        pause: vi.fn(),
        cancel: vi.fn(),
        complete: vi.fn(),
        startIteration: vi.fn(),
        endIteration: vi.fn(),
      },
      getAbortSignal: vi.fn().mockReturnValue(new AbortController().signal),
    };

    mockConversationManager = {
      getMessageCount: vi.fn().mockReturnValue(5),
      getMessages: vi.fn().mockReturnValue([]),
    } as unknown as ConversationSession;

    mockIterationCoordinator = {
      executeIteration: vi.fn(),
      executeIterationStream: vi.fn(),
      emitToRegistry: vi.fn().mockResolvedValue(undefined),
    } as unknown as AgentIterationCoordinator;

    mockMetricsRegistry = {
      getAgentCollector: vi.fn().mockReturnValue({
        recordExecutionStart: vi.fn(),
        recordExecutionComplete: vi.fn(),
      }),
    } as unknown as MetricsRegistry;

    coordinator = new AgentExecutionCoordinator({
      iterationCoordinator: mockIterationCoordinator,
      metricsRegistry: mockMetricsRegistry,
    });
  });

  describe("execute (sync mode)", () => {
    it("should complete normally when iterations finish with final answer", async () => {
      wrapCallbackResult();

      mockIterationCoordinator.executeIteration = vi
        .fn()
        .mockResolvedValueOnce({
          success: true,
          shouldContinue: false,
          content: "Final answer",
        });

      const result = await coordinator.execute(
        mockEntity,
        mockConversationManager,
        undefined,
        defaultProfileId,
        defaultMaxIterations,
      );

      expect(result.success).toBe(true);
      expect(result.content).toBe("Final answer");
      expect(result.iterations).toBe(0);
    });

    it("should continue iterations when tool calls are made", async () => {
      let callCount = 0;
      wrapCallbackResult();

      mockIterationCoordinator.executeIteration = vi.fn().mockImplementation(() => {
        callCount++;
        if (callCount < 3) {
          return Promise.resolve({
            success: true,
            shouldContinue: true,
            content: `Iteration ${callCount}`,
          });
        }
        return Promise.resolve({
          success: true,
          shouldContinue: false,
          content: "Final answer",
        });
      });

      const result = await coordinator.execute(
        mockEntity,
        mockConversationManager,
        undefined,
        defaultProfileId,
        defaultMaxIterations,
      );

      expect(result.success).toBe(true);
      expect(result.content).toBe("Final answer");
    });

    it("should return max iterations reached when exceeding limit", async () => {
      wrapCallbackResult();

      const result = await coordinator.execute(
        { ...mockEntity, state: { ...mockEntity.state, currentIteration: 10 } },
        mockConversationManager,
        undefined,
        defaultProfileId,
        defaultMaxIterations,
      );

      expect(result.success).toBe(true);
      expect(result.content).toBe("Reached maximum iterations without final answer.");
    });

    it("should handle interruption (pause)", async () => {
      const mockInterruption = { type: "paused" };
      __mocks__.executeWithInterruptionHandling.mockResolvedValue({
        success: false,
        interruption: mockInterruption,
      });

      const result = await coordinator.execute(
        mockEntity,
        mockConversationManager,
        undefined,
        defaultProfileId,
        defaultMaxIterations,
      );

      expect(result.success).toBe(false);
      expect(result.error).toBe("Execution paused");
      expect(mockEntity.state.pause).toHaveBeenCalledOnce();
    });

    it("should handle interruption (stop)", async () => {
      const mockInterruption = { type: "stopped" };
      __mocks__.executeWithInterruptionHandling.mockResolvedValue({
        success: false,
        interruption: mockInterruption,
      });

      const result = await coordinator.execute(
        mockEntity,
        mockConversationManager,
        undefined,
        defaultProfileId,
        defaultMaxIterations,
      );

      expect(result.success).toBe(false);
      expect(result.error).toBe("Execution stopped");
      expect(mockEntity.state.cancel).toHaveBeenCalledOnce();
    });

    it("should handle errors during execution", async () => {
      __mocks__.executeWithInterruptionHandling.mockRejectedValue(new Error("Something failed"));

      const result = await coordinator.execute(
        mockEntity,
        mockConversationManager,
        undefined,
        defaultProfileId,
        defaultMaxIterations,
      );

      expect(result.success).toBe(false);
    });

    it("should record metrics when metricsRegistry is provided", async () => {
      const collector = { recordExecutionStart: vi.fn(), recordExecutionComplete: vi.fn() };
      const metricsRegistryWithCollector = {
        getAgentCollector: vi.fn().mockReturnValue(collector),
      } as unknown as MetricsRegistry;

      coordinator = new AgentExecutionCoordinator({
        iterationCoordinator: mockIterationCoordinator,
        metricsRegistry: metricsRegistryWithCollector,
      });

      wrapCallbackResult();
      mockIterationCoordinator.executeIteration = vi.fn().mockResolvedValue({
        success: true,
        shouldContinue: false,
        content: "Done",
      });

      await coordinator.execute(
        mockEntity,
        mockConversationManager,
        undefined,
        defaultProfileId,
        defaultMaxIterations,
      );

      expect(collector.recordExecutionStart).toHaveBeenCalledOnce();
      expect(collector.recordExecutionComplete).toHaveBeenCalledOnce();
    });

    it("should handle iteration interruption (non-pause/non-stop interruption)", async () => {
      wrapCallbackResult();

      mockIterationCoordinator.executeIteration = vi.fn().mockResolvedValue({
        success: false,
        shouldContinue: false,
        interruption: "paused",
      });

      const result = await coordinator.execute(
        mockEntity,
        mockConversationManager,
        undefined,
        defaultProfileId,
        defaultMaxIterations,
      );

      expect(result.success).toBe(false);
      expect(result.error).toBe("Execution paused");
    });
  });

  describe("executeStream (stream mode)", () => {
    it("should stream events and complete normally", async () => {
      __mocks__.iterateWithInterruptionHandling.mockImplementation(
        async function* (mainLoop: AsyncGenerator) {
          for await (const item of mainLoop) {
            yield { type: "value", value: item };
          }
        },
      );

      const genStream = (async function* () {
        yield { type: "iteration_complete", agentLoopId: "agent-loop-1", iteration: 1 };
      })();

      mockIterationCoordinator.executeIterationStream = vi.fn().mockReturnValue(genStream);

      const events: any[] = [];
      for await (const event of coordinator.executeStream(
        mockEntity,
        mockConversationManager,
        undefined,
        defaultProfileId,
        1,
      )) {
        events.push(event);
      }

      expect(events.length).toBeGreaterThan(0);
      expect(events[0].type).toBe("agent_start");
    });

    it("should handle interruption in stream (pause)", async () => {
      __mocks__.iterateWithInterruptionHandling.mockImplementation(async function* () {
        yield { type: "interrupted", interruption: { type: "paused" } };
      });

      const events: any[] = [];
      for await (const event of coordinator.executeStream(
        mockEntity,
        mockConversationManager,
        undefined,
        defaultProfileId,
        defaultMaxIterations,
      )) {
        events.push(event);
      }

      expect(events.length).toBeGreaterThan(0);
      expect(mockEntity.state.pause).toHaveBeenCalledOnce();
    });

    it("should handle interruption in stream (stop)", async () => {
      __mocks__.iterateWithInterruptionHandling.mockImplementation(async function* () {
        yield { type: "interrupted", interruption: { type: "stopped" } };
      });

      const events: any[] = [];
      for await (const event of coordinator.executeStream(
        mockEntity,
        mockConversationManager,
        undefined,
        defaultProfileId,
        defaultMaxIterations,
      )) {
        events.push(event);
      }

      expect(mockEntity.state.cancel).toHaveBeenCalledOnce();
    });

    it("should handle errors during stream execution", async () => {
      __mocks__.iterateWithInterruptionHandling.mockImplementation(async function* () {
        throw new Error("Stream error");
      });

      const events: any[] = [];
      for await (const event of coordinator.executeStream(
        mockEntity,
        mockConversationManager,
        undefined,
        defaultProfileId,
        defaultMaxIterations,
      )) {
        events.push(event);
      }

      expect(events.length).toBeGreaterThan(0);
      const errorEvent = events.find(e => e.type === "agent_error");
      expect(errorEvent).toBeDefined();
    });
  });
});