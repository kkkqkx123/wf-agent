/**
 * AgentLoopStateTransitor Unit Tests
 *
 * Tests for state transition operations including:
 * - start/pause/resume/complete/fail/cancel transitions
 * - Event emission via EventRegistry
 * - Idempotent transition handling
 * - Edge cases (already paused, already cancelled, etc.)
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import type { EventRegistry } from "../../../../core/registry/event-registry.js";
import type { AgentLoopResult } from "@wf-agent/types";
import { AgentLoopStateTransitor } from "../agent-loop-state-transitor.js";

// Mock event builders
vi.mock("../../../../core/utils/event/builders/agent-events.js", () => ({
  buildAgentStartedEvent: vi.fn(() => ({ type: "AGENT_STARTED", agentLoopId: "test-id" })),
  buildAgentCompletedEvent: vi.fn(() => ({ type: "AGENT_COMPLETED", agentLoopId: "test-id" })),
  buildAgentPausedEvent: vi.fn(() => ({ type: "AGENT_PAUSED", agentLoopId: "test-id" })),
  buildAgentCancelledEvent: vi.fn(() => ({ type: "AGENT_CANCELLED", agentLoopId: "test-id" })),
  buildAgentResumedEvent: vi.fn(() => ({ type: "AGENT_RESUMED", agentLoopId: "test-id" })),
  buildAgentFailedEvent: vi.fn(() => ({ type: "AGENT_FAILED", agentLoopId: "test-id" })),
}));

// Mock event emitter - delegate to eventManager so assertions on eventManager.emit work
vi.mock("../../../../core/utils/event/emit-event.js", () => ({
  emit: vi.fn(async (eventManager, event) => {
    if (eventManager) {
      await eventManager.emit(event);
    }
  }),
}));

describe("AgentLoopStateTransitor", () => {
  let transitor: AgentLoopStateTransitor;
  let mockEventManager: EventRegistry;
  let mockEntity: any;

  beforeEach(() => {
    mockEventManager = {
      emit: vi.fn().mockResolvedValue(undefined),
    } as unknown as EventRegistry;

    transitor = new AgentLoopStateTransitor(mockEventManager);

    // Create a minimal mock entity
    mockEntity = {
      id: "test-agent-loop-id",
      config: {
        maxIterations: 10,
      },
      getStatus: vi.fn(),
      state: {
        currentIteration: 0,
        toolCallCount: 0,
        pendingToolCalls: new Set<string>(),
        isStreaming: false,
        streamMessage: null,
        start: vi.fn(),
        pause: vi.fn(),
        resume: vi.fn(),
        complete: vi.fn(),
        fail: vi.fn(),
        cancel: vi.fn(),
      },
      getMessages: vi.fn().mockReturnValue([]),
    };
  });

  describe("startAgentLoop", () => {
    it("should transition from CREATED to RUNNING and emit event", async () => {
      mockEntity.getStatus.mockReturnValue("CREATED");

      await transitor.startAgentLoop(mockEntity);

      expect(mockEntity.state.start).toHaveBeenCalledOnce();
      expect(mockEventManager.emit).toHaveBeenCalledOnce();
    });

    it("should still start even if status is not CREATED (non-blocking)", async () => {
      mockEntity.getStatus.mockReturnValue("COMPLETED");

      await transitor.startAgentLoop(mockEntity);

      expect(mockEntity.state.start).toHaveBeenCalledOnce();
      expect(mockEventManager.emit).toHaveBeenCalledOnce();
    });
  });

  describe("pauseAgentLoop", () => {
    it("should transition from RUNNING to PAUSED and emit event", async () => {
      mockEntity.getStatus.mockReturnValue("RUNNING");

      await transitor.pauseAgentLoop(mockEntity);

      expect(mockEntity.state.pause).toHaveBeenCalledOnce();
      expect(mockEventManager.emit).toHaveBeenCalledOnce();
    });

    it("should be idempotent when already PAUSED", async () => {
      mockEntity.getStatus.mockReturnValue("PAUSED");

      await transitor.pauseAgentLoop(mockEntity);

      expect(mockEntity.state.pause).not.toHaveBeenCalled();
      expect(mockEventManager.emit).not.toHaveBeenCalled();
    });
  });

  describe("resumeAgentLoop", () => {
    it("should transition from PAUSED to RUNNING and emit event", async () => {
      mockEntity.getStatus.mockReturnValue("PAUSED");

      await transitor.resumeAgentLoop(mockEntity);

      expect(mockEntity.state.resume).toHaveBeenCalledOnce();
      expect(mockEventManager.emit).toHaveBeenCalledOnce();
    });

    it("should be idempotent when already RUNNING", async () => {
      mockEntity.getStatus.mockReturnValue("RUNNING");

      await transitor.resumeAgentLoop(mockEntity);

      expect(mockEntity.state.resume).not.toHaveBeenCalled();
      expect(mockEventManager.emit).not.toHaveBeenCalled();
    });
  });

  describe("completeAgentLoop", () => {
    const mockResult: AgentLoopResult = {
      success: true,
      iterations: 5,
      toolCallCount: 3,
      content: "Task completed",
    };

    it("should transition to COMPLETED and emit event", async () => {
      mockEntity.getStatus.mockReturnValue("RUNNING");

      await transitor.completeAgentLoop(mockEntity, mockResult);

      expect(mockEntity.state.complete).toHaveBeenCalledOnce();
      expect(mockEventManager.emit).toHaveBeenCalledOnce();
    });

    it("should not throw when event emission fails", async () => {
      mockEntity.getStatus.mockReturnValue("RUNNING");
      (mockEventManager.emit as ReturnType<typeof vi.fn>).mockRejectedValueOnce(
        new Error("Emit failed"),
      );

      await expect(transitor.completeAgentLoop(mockEntity, mockResult)).resolves.not.toThrow();
      expect(mockEntity.state.complete).toHaveBeenCalledOnce();
    });
  });

  describe("failAgentLoop", () => {
    it("should transition to FAILED and emit event", async () => {
      mockEntity.getStatus.mockReturnValue("RUNNING");
      const error = new Error("Something went wrong");

      await transitor.failAgentLoop(mockEntity, error);

      expect(mockEntity.state.fail).toHaveBeenCalledWith(error);
      expect(mockEventManager.emit).toHaveBeenCalledOnce();
    });

    it("should handle non-Error fail input", async () => {
      mockEntity.getStatus.mockReturnValue("RUNNING");
      const error = "string error message";

      await transitor.failAgentLoop(mockEntity, error);

      expect(mockEntity.state.fail).toHaveBeenCalledWith(error);
      expect(mockEventManager.emit).toHaveBeenCalledOnce();
    });

    it("should not throw when event emission fails", async () => {
      mockEntity.getStatus.mockReturnValue("RUNNING");
      (mockEventManager.emit as ReturnType<typeof vi.fn>).mockRejectedValueOnce(
        new Error("Emit failed"),
      );

      await expect(transitor.failAgentLoop(mockEntity, new Error("test"))).resolves.not.toThrow();
    });
  });

  describe("cancelAgentLoop", () => {
    it("should transition from RUNNING to CANCELLED and emit event", async () => {
      mockEntity.getStatus.mockReturnValue("RUNNING");

      await transitor.cancelAgentLoop(mockEntity, "User requested stop");

      expect(mockEntity.state.cancel).toHaveBeenCalledOnce();
      expect(mockEventManager.emit).toHaveBeenCalledOnce();
    });

    it("should cancel without a reason", async () => {
      mockEntity.getStatus.mockReturnValue("RUNNING");

      await transitor.cancelAgentLoop(mockEntity);

      expect(mockEntity.state.cancel).toHaveBeenCalledOnce();
      expect(mockEventManager.emit).toHaveBeenCalledOnce();
    });

    it("should be idempotent when already CANCELLED", async () => {
      mockEntity.getStatus.mockReturnValue("CANCELLED");

      await transitor.cancelAgentLoop(mockEntity);

      expect(mockEntity.state.cancel).not.toHaveBeenCalled();
      expect(mockEventManager.emit).not.toHaveBeenCalled();
    });
  });
});