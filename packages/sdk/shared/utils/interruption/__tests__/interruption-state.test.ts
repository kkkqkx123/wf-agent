/**
 * Unit Tests for InterruptionState
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import { InterruptionState } from "../interruption-state.js";

// Mock contextual-logger to prevent actual logging
vi.mock("../../../utils/contextual-logger.js", () => ({
  createContextualLogger: () => ({
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
    debug: vi.fn(),
  }),
}));

describe("InterruptionState", () => {
  let state: InterruptionState;

  beforeEach(() => {
    state = new InterruptionState({ contextId: "exec-1" });
  });

  describe("constructor", () => {
    it("should initialize with DEFAULT_UNKNOWN context", () => {
      expect(state.getContextId()).toBe("exec-1");
      expect(state.getContext()).toEqual({ domain: "UNKNOWN" });
    });

    it("should initialize with provided context", () => {
      const ctxState = new InterruptionState({
        contextId: "exec-2",
        context: {
          domain: "WORKFLOW_NODE",
          workflowId: "wf-1",
          nodeId: "n-1",
          nodeExecutionId: "ne-1",
        },
      });
      expect(ctxState.getContext().domain).toBe("WORKFLOW_NODE");
    });

    it("should initialize with default state (no interruption)", () => {
      expect(state.getInterruptionType()).toBeNull();
      expect(state.isAborted()).toBe(false);
      expect(state.shouldPause()).toBe(false);
      expect(state.shouldStop()).toBe(false);
    });
  });

  describe("getInterruptionType / isAborted / shouldPause / shouldStop", () => {
    it("should return null / false initially", () => {
      expect(state.getInterruptionType()).toBeNull();
      expect(state.isAborted()).toBe(false);
      expect(state.shouldPause()).toBe(false);
      expect(state.shouldStop()).toBe(false);
    });

    it("should reflect PAUSE state after requestPause", () => {
      state.requestPause();
      expect(state.getInterruptionType()).toBe("PAUSE");
      expect(state.isAborted()).toBe(true);
      expect(state.shouldPause()).toBe(true);
      expect(state.shouldStop()).toBe(false);
    });

    it("should reflect STOP state after requestStop", () => {
      state.requestStop();
      expect(state.getInterruptionType()).toBe("STOP");
      expect(state.isAborted()).toBe(true);
      expect(state.shouldPause()).toBe(false);
      expect(state.shouldStop()).toBe(true);
    });

    it("should return null after resume", () => {
      state.requestPause();
      state.resume();
      expect(state.getInterruptionType()).toBeNull();
      expect(state.isAborted()).toBe(false);
    });
  });

  describe("requestPause", () => {
    it("should be idempotent", () => {
      state.requestPause();
      state.requestPause(); // Second call should be a no-op
      expect(state.getInterruptionType()).toBe("PAUSE");
    });

    it("should abort the signal with PAUSE reason", () => {
      state.requestPause();
      const signal = state.getAbortSignal();
      expect(signal.aborted).toBe(true);
      const reason = signal.reason as Error & { interruptionType: string };
      expect(reason.interruptionType).toBe("PAUSE");
    });
  });

  describe("requestStop", () => {
    it("should be idempotent", () => {
      state.requestStop();
      state.requestStop();
      expect(state.getInterruptionType()).toBe("STOP");
    });

    it("should upgrade PAUSE to STOP", () => {
      state.requestPause();
      state.requestStop();
      expect(state.getInterruptionType()).toBe("STOP");
    });

    it("should abort the signal with STOP reason", () => {
      state.requestStop();
      const signal = state.getAbortSignal();
      expect(signal.aborted).toBe(true);
      const reason = signal.reason as Error & { interruptionType: string };
      expect(reason.interruptionType).toBe("STOP");
    });
  });

  describe("resume", () => {
    it("should reset interruption state and create new signal", () => {
      state.requestPause();
      const oldSignal = state.getAbortSignal();

      state.resume();

      expect(state.getInterruptionType()).toBeNull();
      const newSignal = state.getAbortSignal();
      expect(newSignal).not.toBe(oldSignal);
      expect(newSignal.aborted).toBe(false);
    });

    it("should be a no-op when not previously paused (fresh resume)", () => {
      // Resume on a fresh state is now guarded: only allowed when actually paused
      expect(() => state.resume()).not.toThrow();
      // State should remain unchanged (null)
      expect(state.getInterruptionType()).toBeNull();
    });
  });

  describe("getAbortSignal", () => {
    it("should return a non-aborted signal initially", () => {
      expect(state.getAbortSignal().aborted).toBe(false);
    });

    it("should return aborted signal after requestPause", () => {
      state.requestPause();
      expect(state.getAbortSignal().aborted).toBe(true);
    });

    it("should return new non-aborted signal after resume", () => {
      state.requestPause();
      state.resume();
      expect(state.getAbortSignal().aborted).toBe(false);
    });
  });

  describe("getAbortReason", () => {
    it("should return undefined when not aborted", () => {
      expect(state.getAbortReason()).toBeUndefined();
    });

    it("should return the abort reason after pause", () => {
      state.requestPause();
      const reason = state.getAbortReason();
      expect(reason).toBeInstanceOf(Error);
      expect(reason!.message).toBe("Execution paused");
    });
  });

  describe("updateContext", () => {
    it("should replace the context entirely", () => {
      state.updateContext({
        domain: "AGENT_LOOP",
        agentExecutionId: "agent-1",
        iteration: 5,
      });
      const ctx = state.getContext();
      if (ctx.domain === "AGENT_LOOP") {
        expect(ctx.iteration).toBe(5);
      } else {
        // TypeScript narrowing; if we get here the test should fail conceptually
        expect(ctx.domain).toBe("AGENT_LOOP");
      }
    });
  });

  describe("onInterrupted", () => {
    it("should notify listeners on PAUSE", () => {
      const listener = vi.fn();
      state.onInterrupted(listener);

      state.requestPause();
      expect(listener).toHaveBeenCalledWith("PAUSE");
    });

    it("should notify listeners on STOP", () => {
      const listener = vi.fn();
      state.onInterrupted(listener);

      state.requestStop();
      expect(listener).toHaveBeenCalledWith("STOP");
    });

    it("should notify listeners on RESUME", () => {
      const listener = vi.fn();
      state.onInterrupted(listener);
      state.requestPause();

      state.resume();
      expect(listener).toHaveBeenCalledWith("RESUME");
    });

    it("should support unsubscribe", () => {
      const listener = vi.fn();
      const unsubscribe = state.onInterrupted(listener);

      unsubscribe();
      state.requestPause();
      expect(listener).not.toHaveBeenCalled();
    });

    it("should not throw when listener throws", () => {
      state.onInterrupted(() => {
        throw new Error("listener error");
      });

      expect(() => state.requestPause()).not.toThrow();
    });
  });

  describe("onResumed", () => {
    it("should notify on resume", () => {
      const listener = vi.fn();
      state.onResumed(listener);
      state.requestPause();

      state.resume();
      expect(listener).toHaveBeenCalled();
    });

    it("should support unsubscribe", () => {
      const listener = vi.fn();
      const unsubscribe = state.onResumed(listener);
      unsubscribe();

      state.requestPause();
      state.resume();
      expect(listener).not.toHaveBeenCalled();
    });

    it("should not throw when listener throws", () => {
      state.onResumed(() => {
        throw new Error("resume listener error");
      });
      state.requestPause();

      expect(() => state.resume()).not.toThrow();
    });
  });

  describe("EventRegistry integration", () => {
    it("should emit event via EventRegistry on PAUSE", () => {
      const mockEmit = vi.fn();
      const mockGetEmitter = vi.fn().mockReturnValue({ emit: mockEmit });
      const mockEventRegistry = { getEmitter: mockGetEmitter } as any;

      const stateWithEvents = new InterruptionState({
        contextId: "exec-1",
        eventRegistry: mockEventRegistry,
      });

      stateWithEvents.requestPause();

      expect(mockGetEmitter).toHaveBeenCalledWith("exec-1");
      expect(mockEmit).toHaveBeenCalledOnce();
      const emittedEvent = mockEmit.mock.calls[0]![0];
      expect(emittedEvent.type).toBe("EXECUTION_PAUSED");
      expect(emittedEvent.executionId).toBe("exec-1");
    });

    it("should emit event via EventRegistry on STOP", () => {
      const mockEmit = vi.fn();
      const mockGetEmitter = vi.fn().mockReturnValue({ emit: mockEmit });
      const mockEventRegistry = { getEmitter: mockGetEmitter } as any;

      const stateWithEvents = new InterruptionState({
        contextId: "exec-1",
        eventRegistry: mockEventRegistry,
      });

      stateWithEvents.requestStop();

      const emittedEvent = mockEmit.mock.calls[0]![0];
      expect(emittedEvent.type).toBe("EXECUTION_CANCELLED");
    });

    it("should emit event via EventRegistry on RESUME", () => {
      const mockEmit = vi.fn();
      const mockGetEmitter = vi.fn().mockReturnValue({ emit: mockEmit });
      const mockEventRegistry = { getEmitter: mockGetEmitter } as any;

      const stateWithEvents = new InterruptionState({
        contextId: "exec-1",
        eventRegistry: mockEventRegistry,
      });

      stateWithEvents.requestPause();
      stateWithEvents.resume();

      // PAUSE + RESUME
      expect(mockEmit).toHaveBeenCalledTimes(2);
      const resumeEvent = mockEmit.mock.calls[1]![0];
      expect(resumeEvent.type).toBe("EXECUTION_RESUMED");
    });

    it("should not throw when emit fails", () => {
      const mockEmit = vi.fn().mockRejectedValue(new Error("emit failed"));
      const mockGetEmitter = vi.fn().mockReturnValue({ emit: mockEmit });
      const mockEventRegistry = { getEmitter: mockGetEmitter } as any;

      const stateWithEvents = new InterruptionState({
        contextId: "exec-1",
        eventRegistry: mockEventRegistry,
      });

      expect(() => stateWithEvents.requestPause()).not.toThrow();
    });
  });

  describe("setEventRegistry", () => {
    it("should set event registry after construction", () => {
      const mockEmit = vi.fn();
      const mockGetEmitter = vi.fn().mockReturnValue({ emit: mockEmit });
      const mockEventRegistry = { getEmitter: mockGetEmitter } as any;

      state.setEventRegistry(mockEventRegistry);
      state.requestPause();

      expect(mockEmit).toHaveBeenCalled();
    });
  });

  describe("connectToParent / disconnectFromParent", () => {
    it("should subscribe to parent events when EventRegistry is set", () => {
      const mockOn = vi.fn().mockReturnValue(vi.fn());
      const mockGetEmitter = vi.fn().mockReturnValue({ on: mockOn });
      const mockEventRegistry = { getEmitter: mockGetEmitter } as any;

      new InterruptionState({
        contextId: "child-1",
        eventRegistry: mockEventRegistry,
        parentExecutionId: "parent-1",
      });

      expect(mockGetEmitter).toHaveBeenCalledWith("parent-1");
      // EVENT_PAUSED, EVENT_CANCELLED, EVENT_RESUMED
      expect(mockOn).toHaveBeenCalledTimes(3);
    });

    it("should warn when connectToParent called without EventRegistry", () => {
      // Should not throw, just warn
      expect(() => state.connectToParent("parent-1")).not.toThrow();
    });

    it("should disconnect from parent", () => {
      const parentUnsubscribe = vi.fn();
      const mockOn = vi.fn().mockReturnValue(parentUnsubscribe);
      const mockGetEmitter = vi.fn().mockReturnValue({ on: mockOn });
      const mockEventRegistry = { getEmitter: mockGetEmitter } as any;

      state.setEventRegistry(mockEventRegistry);
      state.connectToParent("parent-1");
      state.disconnectFromParent();
    });
  });

  describe("dispose", () => {
    it("should mark as disposed", () => {
      state.dispose();
      expect(state.isDisposed()).toBe(true);
    });

    it("should prevent further operations", () => {
      state.dispose();
      expect(() => state.requestPause()).not.toThrow(); // Should warn only, not throw
      // State should not be changed
      expect(state.getInterruptionType()).toBeNull();
    });

    it("should be idempotent", () => {
      state.dispose();
      expect(() => state.dispose()).not.toThrow();
    });
  });
});
