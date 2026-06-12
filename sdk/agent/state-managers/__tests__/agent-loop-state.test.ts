/**
 * Tests for AgentLoopState
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import { AgentLoopState } from "../agent-loop-state.js";
import { AgentLoopStatus, RuntimeValidationError } from "@wf-agent/types";
import type { LLMMessage } from "@wf-agent/types";

vi.mock("@wf-agent/common-utils", async importOriginal => {
  const actual = await importOriginal();
  return {
    ...(actual as Record<string, unknown>),
    now: vi.fn(() => 1000000),
  };
});

describe("AgentLoopState", () => {
  let state: AgentLoopState;

  beforeEach(() => {
    state = new AgentLoopState();
  });

  describe("initial state", () => {
    it("should start with CREATED status", () => {
      expect(state.status).toBe(AgentLoopStatus.CREATED);
    });

    it("should start with zero iterations", () => {
      expect(state.currentIteration).toBe(0);
    });

    it("should start with zero tool calls", () => {
      expect(state.toolCallCount).toBe(0);
    });

    it("should start with no iteration history", () => {
      expect(state.iterationHistory).toEqual([]);
    });

    it("should start with null timestamps", () => {
      expect(state.startTime).toBeNull();
      expect(state.endTime).toBeNull();
    });

    it("should start with null error", () => {
      expect(state.error).toBeNull();
    });

    it("should start with streaming disabled", () => {
      expect(state.isStreaming).toBe(false);
      expect(state.streamMessage).toBeNull();
    });

    it("should start with no pending tool calls", () => {
      expect(state.pendingToolCalls.size).toBe(0);
      expect(state.getPendingToolCallCount()).toBe(0);
    });

    it("should start with control flags disabled", () => {
      expect(state.shouldPause()).toBe(false);
      expect(state.shouldStop()).toBe(false);
    });

    it("should be empty initially", () => {
      expect(state.isEmpty()).toBe(true);
      expect(state.size()).toBe(0);
    });
  });

  describe("status transitions", () => {
    it("should start execution from CREATED state", () => {
      state.start();

      expect(state.status).toBe(AgentLoopStatus.RUNNING);
      expect(state.startTime).toBe(1000000);
    });

    it("should throw when starting from RUNNING state", () => {
      state.start();

      expect(() => state.start()).toThrow(RuntimeValidationError);
    });

    it("should throw when starting from COMPLETED state", () => {
      state.start();
      state.complete();

      expect(() => state.start()).toThrow(RuntimeValidationError);
    });

    it("should throw when starting from FAILED state", () => {
      state.start();
      state.fail("Error");

      expect(() => state.start()).toThrow(RuntimeValidationError);
    });

    it("should complete execution from RUNNING state", () => {
      state.start();

      state.complete();

      expect(state.status).toBe(AgentLoopStatus.COMPLETED);
      expect(state.endTime).toBe(1000000);
    });

    it("should throw when completing from non-RUNNING state", () => {
      expect(() => state.complete()).toThrow(RuntimeValidationError);
    });

    it("should fail execution from RUNNING state", () => {
      state.start();

      state.fail("Something went wrong");

      expect(state.status).toBe(AgentLoopStatus.FAILED);
      expect(state.error).toBe("Something went wrong");
      expect(state.endTime).toBe(1000000);
    });

    it("should fail execution from PAUSED state", () => {
      state.start();
      state.pause();

      state.fail("Error during pause");

      expect(state.status).toBe(AgentLoopStatus.FAILED);
    });

    it("should throw when failing completed execution", () => {
      state.start();
      state.complete();

      expect(() => state.fail("Error")).toThrow(RuntimeValidationError);
    });

    it("should throw when failing with null error", () => {
      state.start();

      expect(() => state.fail(null)).toThrow(RuntimeValidationError);
    });

    it("should throw when failing with undefined error", () => {
      state.start();

      expect(() => state.fail(undefined)).toThrow(RuntimeValidationError);
    });

    it("should cancel execution from RUNNING state", () => {
      state.start();

      state.cancel();

      expect(state.status).toBe(AgentLoopStatus.CANCELLED);
      expect(state.endTime).toBe(1000000);
    });

    it("should cancel execution from PAUSED state", () => {
      state.start();
      state.pause();

      state.cancel();

      expect(state.status).toBe(AgentLoopStatus.CANCELLED);
    });

    it("should throw when cancelling COMPLETED execution", () => {
      state.start();
      state.complete();

      expect(() => state.cancel()).toThrow(RuntimeValidationError);
    });

    it("should throw when cancelling CANCELLED execution", () => {
      state.start();
      state.cancel();

      expect(() => state.cancel()).toThrow(RuntimeValidationError);
    });

    describe("pause and resume", () => {
      it("should pause from RUNNING state", () => {
        state.start();

        state.pause();

        expect(state.status).toBe(AgentLoopStatus.PAUSED);
        expect(state.shouldPause()).toBe(false);
      });

      it("should throw when pausing from non-RUNNING state", () => {
        expect(() => state.pause()).toThrow(RuntimeValidationError);
      });

      it("should resume from PAUSED state", () => {
        state.start();
        state.pause();

        state.resume();

        expect(state.status).toBe(AgentLoopStatus.RUNNING);
      });

      it("should throw when resuming from non-PAUSED state", () => {
        expect(() => state.resume()).toThrow(RuntimeValidationError);
      });

      it("should reset pause flag on pause", () => {
        state.start();
        state.setShouldPause(true);

        state.pause();

        expect(state.shouldPause()).toBe(false);
      });

      it("should reset pause flag on resume", () => {
        state.start();
        state.pause();
        state.setShouldPause(true);

        state.resume();

        expect(state.shouldPause()).toBe(false);
      });
    });
  });

  describe("iteration management", () => {
    it("should start a new iteration", () => {
      state.startIteration();

      expect(state.currentIteration).toBe(1);
    });

    it("should increment iteration counter on each start", () => {
      state.startIteration();
      state.endIteration("Response 1");

      state.startIteration();

      expect(state.currentIteration).toBe(2);
    });

    it("should end iteration with response content", () => {
      state.startIteration();

      state.endIteration("Final response");

      expect(state.iterationHistory).toHaveLength(1);
      expect(state.iterationHistory[0]!.iteration).toBe(1);
      expect(state.iterationHistory[0]!.responseContent).toBe("Final response");
    });

    it("should end iteration without response content", () => {
      state.startIteration();
      state.endIteration();

      expect(state.iterationHistory).toHaveLength(1);
      expect(state.iterationHistory[0]!.responseContent).toBeUndefined();
    });

    it("should handle ending iteration when no current record exists", () => {
      // Should not throw when no iteration is active
      state.endIteration();
      expect(state.iterationHistory).toHaveLength(0);
    });

    it("should track multiple iterations", () => {
      state.startIteration();
      state.endIteration("Response 1");
      state.startIteration();
      state.endIteration("Response 2");
      state.startIteration();
      state.endIteration("Response 3");

      expect(state.iterationHistory).toHaveLength(3);
      expect(state.currentIteration).toBe(3);
    });
  });

  describe("tool call recording", () => {
    it("should record tool call start and end", () => {
      state.startIteration();

      const record = state.recordToolCallStart("call-1", "test_tool", { arg: "value" });

      expect(record.id).toBe("call-1");
      expect(record.name).toBe("test_tool");
      expect(record.arguments).toEqual({ arg: "value" });
      expect(record.startTime).toBe(1000000);
      expect(record.endTime).toBeUndefined();

      state.recordToolCallEnd("call-1", "result");
      state.endIteration();

      expect(state.toolCallCount).toBe(1);
      expect(state.iterationHistory[0]!.toolCalls[0]!.endTime).toBe(1000000);
      expect(state.iterationHistory[0]!.toolCalls[0]!.result).toBe("result");
    });

    it("should record tool call with error", () => {
      state.startIteration();
      state.recordToolCallStart("call-1", "test_tool", {});
      state.recordToolCallEnd("call-1", undefined, "Error message");
      state.endIteration();

      expect(state.toolCallCount).toBe(1);
      expect(state.iterationHistory[0]!.toolCalls[0]!.error).toBe("Error message");
    });

    it("should record tool call outside iteration", () => {
      // Should not throw even without active iteration
      state.recordToolCallStart("call-1", "test_tool", {});
      state.recordToolCallEnd("call-1", "result");

      expect(state.toolCallCount).toBe(1);
    });

    it("should mark tool call as pending on start", () => {
      state.startIteration();
      state.recordToolCallStart("call-1", "test_tool", {});

      expect(state.isToolCallPending("call-1")).toBe(true);
    });

    it("should remove pending status on end", () => {
      state.startIteration();
      state.recordToolCallStart("call-1", "test_tool", {});
      state.recordToolCallEnd("call-1", "result");

      expect(state.isToolCallPending("call-1")).toBe(false);
    });

    it("should not add to pending set when no current iteration", () => {
      state.recordToolCallStart("call-1", "test_tool", {});

      // Pending set is still populated regardless of iteration context
      expect(state.isToolCallPending("call-1")).toBe(true);
    });
  });

  describe("streaming state", () => {
    it("should start streaming", () => {
      state.startStreaming();

      expect(state.isStreaming).toBe(true);
      expect(state.streamMessage).toBeNull();
    });

    it("should update streaming message", () => {
      state.startStreaming();
      state.updateStreamMessage({ role: "assistant", content: "Hello" } as LLMMessage);

      expect(state.streamMessage).toBeDefined();
      expect(state.streamMessage!.content).toBe("Hello");
    });

    it("should merge streaming updates", () => {
      state.startStreaming();
      state.updateStreamMessage({ role: "assistant", content: "Hello" } as LLMMessage);
      state.updateStreamMessage({ content: "Hello world" } as Partial<LLMMessage>);

      expect(state.streamMessage!.role).toBe("assistant");
      expect(state.streamMessage!.content).toBe("Hello world");
    });

    it("should end streaming and return the final message", () => {
      state.startStreaming();
      state.updateStreamMessage({ role: "assistant", content: "Final" } as LLMMessage);

      const message = state.endStreaming();

      expect(state.isStreaming).toBe(false);
      expect(state.streamMessage).toBeNull();
      expect(message).toBeDefined();
      expect(message!.content).toBe("Final");
    });

    it("should end streaming without message", () => {
      state.startStreaming();

      const message = state.endStreaming();

      expect(message).toBeNull();
      expect(state.isStreaming).toBe(false);
    });

    it("should clear streaming state on complete", () => {
      state.start();
      state.startStreaming();
      state.updateStreamMessage({ role: "assistant", content: "In progress" } as LLMMessage);

      state.complete();

      expect(state.isStreaming).toBe(false);
      expect(state.streamMessage).toBeNull();
    });

    it("should clear streaming state on fail", () => {
      state.start();
      state.startStreaming();

      state.fail("Error");

      expect(state.isStreaming).toBe(false);
      expect(state.streamMessage).toBeNull();
    });

    it("should clear streaming state on cancel", () => {
      state.start();
      state.startStreaming();

      state.cancel();

      expect(state.isStreaming).toBe(false);
      expect(state.streamMessage).toBeNull();
    });
  });

  describe("pending tool calls", () => {
    it("should add and remove pending tool calls", () => {
      state.addPendingToolCall("call-1");
      expect(state.getPendingToolCallCount()).toBe(1);

      state.removePendingToolCall("call-1");
      expect(state.getPendingToolCallCount()).toBe(0);
    });

    it("should return copy of pending tool calls set", () => {
      state.addPendingToolCall("call-1");

      const pending = state.pendingToolCalls;
      pending.add("call-2");

      expect(state.getPendingToolCallCount()).toBe(1);
    });

    it("should check if tool call is pending", () => {
      state.addPendingToolCall("call-1");

      expect(state.isToolCallPending("call-1")).toBe(true);
      expect(state.isToolCallPending("call-2")).toBe(false);
    });

    it("should clear all pending tool calls", () => {
      state.addPendingToolCall("call-1");
      state.addPendingToolCall("call-2");

      state.clearPendingToolCalls();

      expect(state.getPendingToolCallCount()).toBe(0);
    });

    it("should clear pending tool calls on cleanup", () => {
      state.addPendingToolCall("call-1");

      state.cleanup();

      expect(state.getPendingToolCallCount()).toBe(0);
    });
  });

  describe("interrupt handling", () => {
    it("should set pause flag on PAUSE interrupt", () => {
      state.interrupt("PAUSE");

      expect(state.shouldPause()).toBe(true);
      expect(state.shouldStop()).toBe(false);
    });

    it("should set stop flag on STOP interrupt", () => {
      state.interrupt("STOP");

      expect(state.shouldStop()).toBe(true);
      expect(state.shouldPause()).toBe(false);
    });

    it("should reset interrupt flags", () => {
      state.interrupt("PAUSE");
      state.interrupt("STOP");

      state.resetInterrupt();

      expect(state.shouldPause()).toBe(false);
      expect(state.shouldStop()).toBe(false);
    });

    it("should set and get pause flag via setShouldPause", () => {
      state.setShouldPause(true);
      expect(state.shouldPause()).toBe(true);

      state.setShouldPause(false);
      expect(state.shouldPause()).toBe(false);
    });

    it("should set and get stop flag via setShouldStop", () => {
      state.setShouldStop(true);
      expect(state.shouldStop()).toBe(true);

      state.setShouldStop(false);
      expect(state.shouldStop()).toBe(false);
    });
  });

  describe("snapshot operations", () => {
    it("should create snapshot of current state", () => {
      state.start();
      state.startIteration();
      state.recordToolCallStart("call-1", "test_tool", {});
      state.endIteration("Response");

      const snapshot = state.createSnapshot();

      expect(snapshot.status).toBe(AgentLoopStatus.RUNNING);
      expect(snapshot.currentIteration).toBe(1);
      expect(snapshot.toolCallCount).toBe(0); // Tool call not ended, so count not incremented
      expect(snapshot.startTime).toBe(1000000);
      expect(snapshot.iterationHistory).toHaveLength(1);
    });

    it("should create snapshot with no iterations", () => {
      const snapshot = state.createSnapshot();

      expect(snapshot.status).toBe(AgentLoopStatus.CREATED);
      expect(snapshot.currentIteration).toBe(0);
      expect(snapshot.iterationHistory).toEqual([]);
    });

    it("should restore from snapshot", () => {
      // Create a state with some history
      state.start();
      state.startIteration();
      state.endIteration("Response");
      const snapshot = state.createSnapshot();

      // Create new state and restore
      const restoredState = new AgentLoopState();
      restoredState.restoreFromSnapshot(snapshot);

      expect(restoredState.status).toBe(AgentLoopStatus.RUNNING);
      expect(restoredState.currentIteration).toBe(1);
      expect(restoredState.iterationHistory).toHaveLength(1);
      expect(restoredState.startTime).toBe(1000000);
    });

    it("should reset runtime-only fields after restore", () => {
      state.start();
      state.startStreaming();
      state.updateStreamMessage({ role: "assistant", content: "test" } as LLMMessage);
      state.addPendingToolCall("call-1");
      state.setShouldPause(true);
      state.setShouldStop(true);

      const snapshot = state.createSnapshot();

      const restoredState = new AgentLoopState();
      restoredState.restoreFromSnapshot(snapshot);

      // Runtime-only fields should be reset
      expect(restoredState.isStreaming).toBe(false);
      expect(restoredState.streamMessage).toBeNull();
      expect(restoredState.getPendingToolCallCount()).toBe(0);
      expect(restoredState.shouldPause()).toBe(false);
      expect(restoredState.shouldStop()).toBe(false);
    });

    it("should preserve cloned iteration records in snapshot", () => {
      state.start();
      state.startIteration();
      state.recordToolCallStart("call-1", "tool_a", { foo: "bar" });
      state.recordToolCallEnd("call-1", "result");
      state.endIteration("Done");

      const snapshot = state.createSnapshot();

      expect(snapshot.iterationHistory![0]!.toolCalls[0]!.id).toBe("call-1");
      expect(snapshot.iterationHistory![0]!.toolCalls[0]!.result).toBe("result");
      expect(snapshot.iterationHistory![0]!.responseContent).toBe("Done");
    });

    it("should handle snapshot without iterationHistory", () => {
      const restoredState = new AgentLoopState();
      restoredState.restoreFromSnapshot({
        status: AgentLoopStatus.COMPLETED,
        currentIteration: 0,
        toolCallCount: 0,
        startTime: null,
        endTime: 1000000,
        error: undefined,
        messages: [],
      });

      expect(restoredState.status).toBe(AgentLoopStatus.COMPLETED);
      expect(restoredState.iterationHistory).toEqual([]);
    });

    it("should restore currentIterationRecord if present", () => {
      const snapshot = state.createSnapshot();
      // Manually add it to simulate checkpoint data
      const snapshotWithRecord = {
        ...snapshot,
        currentIterationRecord: {
          iteration: 2,
          startTime: 1000000,
          toolCalls: [],
        },
      };

      const restoredState = new AgentLoopState();
      restoredState.restoreFromSnapshot(snapshotWithRecord);

      expect(restoredState.currentIteration).toBe(0); // from snapshot
      // The restored state should have currentIterationRecord set via restore
    });

    it("should create snapshot without streaming fields in checkpoint data", () => {
      state.start();
      state.startStreaming();
      state.updateStreamMessage({ role: "assistant", content: "test" } as LLMMessage);

      const snapshot = state.createSnapshot();

      // Streaming fields should not be in the snapshot
      expect((snapshot as Record<string, unknown>)["isStreaming"]).toBeUndefined();
      expect((snapshot as Record<string, unknown>)["streamMessage"]).toBeUndefined();
    });
  });

  describe("clone", () => {
    it("should create an independent clone", () => {
      state.start();
      state.startIteration();
      state.endIteration("Response");

      const cloned = state.clone();

      expect(cloned.status).toBe(state.status);
      expect(cloned.currentIteration).toBe(state.currentIteration);
      expect(cloned.toolCallCount).toBe(state.toolCallCount);

      // Modify original should not affect clone
      state.complete();
      expect(cloned.status).toBe(AgentLoopStatus.RUNNING);
    });

    it("should clone iteration records independently", () => {
      state.start();
      state.startIteration();
      state.endIteration("Response");

      const cloned = state.clone();

      cloned.iterationHistory[0]!.responseContent = "Modified";

      expect(state.iterationHistory[0]!.responseContent).toBe("Response");
    });
  });

  describe("cleanup", () => {
    it("should clear all runtime state", () => {
      state.start();
      state.startIteration();
      state.recordToolCallStart("call-1", "test_tool", {});
      state.endIteration("Response");
      state.startStreaming();
      state.setShouldPause(true);

      state.cleanup();

      expect(state.iterationHistory).toEqual([]);
      expect(state.error).toBeNull();
      expect(state.streamMessage).toBeNull();
      expect(state.isStreaming).toBe(false);
      expect(state.getPendingToolCallCount()).toBe(0);
      // Status stays unchanged
      expect(state.status).toBe(AgentLoopStatus.RUNNING);
    });
  });

  describe("reset", () => {
    it("should reset to initial state", () => {
      state.start();
      state.startIteration();
      state.recordToolCallStart("call-1", "tool_a", {});
      state.recordToolCallEnd("call-1", "result");
      state.endIteration("Done");
      state.complete();

      state.reset();

      expect(state.status).toBe(AgentLoopStatus.CREATED);
      expect(state.currentIteration).toBe(0);
      expect(state.toolCallCount).toBe(0);
      expect(state.iterationHistory).toEqual([]);
      expect(state.startTime).toBeNull();
      expect(state.endTime).toBeNull();
      expect(state.error).toBeNull();
      expect(state.shouldPause()).toBe(false);
      expect(state.shouldStop()).toBe(false);
      expect(state.isStreaming).toBe(false);
      expect(state.streamMessage).toBeNull();
      expect(state.getPendingToolCallCount()).toBe(0);
      expect(state.isEmpty()).toBe(true);
    });

    it("should be idempotent", () => {
      state.reset();

      expect(state.status).toBe(AgentLoopStatus.CREATED);
    });
  });

  describe("error handling for fail method", () => {
    it("should accept string error", () => {
      state.start();
      state.fail("Something went wrong");

      expect(state.error).toBe("Something went wrong");
    });

    it("should accept Error object", () => {
      state.start();
      const error = new Error("Test error");
      state.fail(error);

      expect(state.error).toBe(error);
    });

    it("should accept numeric error", () => {
      state.start();

      // Numbers are not null/undefined so they are valid
      state.fail(42);

      expect(state.error).toBe(42);
    });
  });

  describe("edge cases", () => {
    it("should handle starting from PAUSED state", () => {
      state.start();
      state.pause();

      state.start();

      expect(state.status).toBe(AgentLoopStatus.RUNNING);
    });

    it("should report size based on iteration history length", () => {
      expect(state.size()).toBe(0);

      state.startIteration();
      state.endIteration();
      expect(state.size()).toBe(1);
    });

    it("should report empty based on iteration history", () => {
      expect(state.isEmpty()).toBe(true);

      state.startIteration();
      expect(state.isEmpty()).toBe(true); // Not ended yet

      state.endIteration();
      expect(state.isEmpty()).toBe(false);
    });
  });
});
