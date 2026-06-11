/**
 * Unit tests for agent-error-handler.ts
 *
 * Tests error handling, interruption handling, error classification,
 * and error creation functions for the Agent Loop.
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import type { AgentLoopEntity } from "../../../entities/agent-loop-entity.js";
import type { EventRegistry } from "../../../../core/registry/event-registry.js";
import type { ExecutionInterruptionCheckResult } from "../../../../core/utils/interruption/index.js";
import { SDKError as SDKErrorClass } from "@wf-agent/types";

// Mock external dependencies
vi.mock("../../../../utils/contextual-logger.js", () => ({
  createContextualLogger: () => ({
    debug: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
  }),
}));

vi.mock("../../../../core/utils/error-utils.js", () => ({
  handleError: vi.fn().mockResolvedValue(undefined),
}));

// Mock emit to forward calls to eventManager.emit for verification
vi.mock("../../../../core/utils/event/emit-event.js", () => ({
  emit: vi.fn(async (eventManager: any, event: any) => {
    if (eventManager) {
      await eventManager.emit(event);
    }
  }),
}));

vi.mock("@wf-agent/common-utils", async (importOriginal) => {
  const actual = await importOriginal();
  return {
    ...(actual as Record<string, unknown>),
    generateId: vi.fn(() => "mock-id-123"),
    now: vi.fn(() => 1000000),
  };
});

// Import after mocks
import {
  handleAgentError,
  handleAgentInterruption,
  isRecoverableAgentError,
  createAgentExecutionError,
} from "../agent-error-handler.js";

describe("AgentErrorHandler", () => {
  let mockEntity: AgentLoopEntity;
  let mockEventManager: EventRegistry;

  beforeEach(() => {
    // Create a minimal mock entity
    mockEntity = {
      id: "agent-loop-1",
      nodeId: "node-1",
      state: {
        currentIteration: 3,
        toolCallCount: 5,
        status: "RUNNING",
        fail: vi.fn(),
        pause: vi.fn(),
        cancel: vi.fn(),
        isStreaming: false,
        pendingToolCalls: new Set<string>(),
        streamMessage: null,
      },
      config: {
        profileId: "test-profile",
      },
      getAbortSignal: vi.fn(() => undefined),
    } as unknown as AgentLoopEntity;

    mockEventManager = {
      emit: vi.fn().mockResolvedValue(undefined),
    } as unknown as EventRegistry;
  });

  describe("handleAgentError", () => {
    it("should standardize a plain Error to SDKError with ERROR severity and fail the entity", async () => {
      const error = new Error("Something went wrong");

      const result = await handleAgentError(mockEntity, error, "test-operation");

      expect(result).toBeInstanceOf(SDKErrorClass);
      expect(result.severity).toBe("error");
      expect(result.message).toBe("Something went wrong");
      expect(mockEntity.state.fail).toHaveBeenCalledWith(result);
    });

    it("should preserve SDKError severity and not fail for warning level", async () => {
      const sdkError = new SDKErrorClass("Recoverable issue", "warning", {
        executionId: "agent-loop-1",
        nodeId: "node-1",
        operation: "tool-call",
      });

      const result = await handleAgentError(mockEntity, sdkError, "tool-call");

      expect(result).toBe(sdkError);
      expect(result.severity).toBe("warning");
      expect(mockEntity.state.fail).not.toHaveBeenCalled();
    });

    it("should preserve SDKError severity and not fail for info level", async () => {
      const sdkError = new SDKErrorClass("Informational", "info", {
        executionId: "agent-loop-1",
        nodeId: "node-1",
        operation: "test",
      });

      const result = await handleAgentError(mockEntity, sdkError, "test");

      expect(result).toBe(sdkError);
      expect(result.severity).toBe("info");
      expect(mockEntity.state.fail).not.toHaveBeenCalled();
    });

    it("should fail entity only for error severity", async () => {
      const error = new Error("Fatal error");

      await handleAgentError(mockEntity, error, "critical-op");

      expect(mockEntity.state.fail).toHaveBeenCalledTimes(1);
    });

    it("should include additional context in the SDKError", async () => {
      const error = new Error("Context test");
      const additionalContext = { workflowId: "wf-123", customField: "value" };

      const result = await handleAgentError(mockEntity, error, "op", additionalContext);

      expect(result.context).toMatchObject({
        executionId: "agent-loop-1",
        nodeId: "node-1",
        operation: "op",
        iteration: 3,
        toolCallCount: 5,
        workflowId: "wf-123",
        customField: "value",
      });
    });
  });

  describe("handleAgentInterruption", () => {
    it("should pause entity and emit AGENT_PAUSED event when interruption type is paused", async () => {
      const interruption: ExecutionInterruptionCheckResult = {
        type: "paused",
      };

      await handleAgentInterruption(mockEntity, interruption, "iteration-start", mockEventManager);

      expect(mockEntity.state.pause).toHaveBeenCalled();
      expect(mockEntity.state.cancel).not.toHaveBeenCalled();

      // Check that eventManager.emit was called (by our emit mock forwarding)
      const emitCalls = vi.mocked(mockEventManager.emit).mock.calls;
      const pauseCall = emitCalls.find(([event]) => (event as { type?: string })?.type === "AGENT_PAUSED");
      expect(pauseCall).toBeDefined();
      const event = pauseCall![0] as unknown as Record<string, unknown>;
      expect(event["agentLoopId"]).toBe("agent-loop-1");
    });

    it("should cancel entity and emit AGENT_CANCELLED event when interruption type is stopped", async () => {
      const interruption: ExecutionInterruptionCheckResult = {
        type: "stopped",
      };

      await handleAgentInterruption(mockEntity, interruption, "execution", mockEventManager);

      expect(mockEntity.state.cancel).toHaveBeenCalled();
      expect(mockEntity.state.pause).not.toHaveBeenCalled();

      const emitCalls = vi.mocked(mockEventManager.emit).mock.calls;
      const cancelCall = emitCalls.find(([event]) => (event as { type?: string })?.type === "AGENT_CANCELLED");
      expect(cancelCall).toBeDefined();
      const event = cancelCall![0] as unknown as Record<string, unknown>;
      expect(event["agentLoopId"]).toBe("agent-loop-1");
    });

    it("should handle interruption without event manager gracefully", async () => {
      const interruption: ExecutionInterruptionCheckResult = {
        type: "paused",
      };

      await handleAgentInterruption(mockEntity, interruption, "test");

      expect(mockEntity.state.pause).toHaveBeenCalled();
    });

    it("should include streaming context in paused event", async () => {
      const state = mockEntity.state as unknown as Record<string, unknown>;
      state["isStreaming"] = true;
      state["streamMessage"] = { role: "assistant", content: "partial" };
      state["pendingToolCalls"] = new Set(["tool-1"]);

      const interruption: ExecutionInterruptionCheckResult = {
        type: "paused",
      };

      await handleAgentInterruption(mockEntity, interruption, "stream", mockEventManager);

      const emitCalls = vi.mocked(mockEventManager.emit).mock.calls;
      const pauseCall = emitCalls.find(([event]) => (event as { type?: string })?.type === "AGENT_PAUSED");
      expect(pauseCall).toBeDefined();
      const event = pauseCall![0] as unknown as Record<string, unknown>;
      expect(event["isStreaming"]).toBe(true);
      expect(event["pendingToolCalls"]).toBe(1);
      expect(event["streamMessagePreserved"]).toBe(true);
    });
  });

  describe("isRecoverableAgentError", () => {
    it("should return true for warning severity", () => {
      const error = new SDKErrorClass("Warning", "warning");
      expect(isRecoverableAgentError(error)).toBe(true);
    });

    it("should return true for info severity", () => {
      const error = new SDKErrorClass("Info", "info");
      expect(isRecoverableAgentError(error)).toBe(true);
    });

    it("should return false for error severity", () => {
      const error = new SDKErrorClass("Error", "error");
      expect(isRecoverableAgentError(error)).toBe(false);
    });
  });

  describe("createAgentExecutionError", () => {
    it("should create an ERROR severity SDKError by default", () => {
      const result = createAgentExecutionError(mockEntity, "Creation failed", "create");

      expect(result).toBeInstanceOf(SDKErrorClass);
      expect(result.severity).toBe("error");
      expect(result.message).toBe("Creation failed");
      expect(result.context).toMatchObject({
        executionId: "agent-loop-1",
        nodeId: "node-1",
        operation: "create",
      });
    });

    it("should create a warning level error when specified", () => {
      const result = createAgentExecutionError(mockEntity, "Warning message", "op", undefined, "warning");

      expect(result.severity).toBe("warning");
    });

    it("should include cause when provided", () => {
      const cause = new Error("Root cause");

      const result = createAgentExecutionError(mockEntity, "Failed", "op", cause);

      expect(result.cause).toBe(cause);
    });
  });
});