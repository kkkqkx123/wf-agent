/**
 * Tests for AgentLoopDiffCalculator
 */

import { describe, it, expect, beforeEach } from "vitest";
import { AgentLoopDiffCalculator } from "../agent-loop-diff-calculator.js";
import type { AgentLoopStateSnapshot, Message } from "@wf-agent/types";
import { AgentLoopStatus } from "@wf-agent/types";

describe("AgentLoopDiffCalculator", () => {
  let calculator: AgentLoopDiffCalculator;

  beforeEach(() => {
    calculator = new AgentLoopDiffCalculator();
  });

  const createSnapshot = (overrides?: Partial<AgentLoopStateSnapshot>): AgentLoopStateSnapshot => ({
    status: AgentLoopStatus.RUNNING,
    currentIteration: 1,
    toolCallCount: 0,
    startTime: Date.now(),
    endTime: null,
    error: undefined,
    messages: [],
    variables: {},
    config: {},
    ...overrides,
  });

  describe("calculateDelta", () => {
    it("should return empty delta when snapshots are identical", () => {
      const snapshot = createSnapshot();
      const delta = calculator.calculateDelta(snapshot, snapshot);

      expect(delta).toEqual({});
    });

    it("should detect added messages", () => {
      const previous = createSnapshot({
        messages: [{ role: "user", content: "Hello" } as Message],
      });
      const current = createSnapshot({
        messages: [
          { role: "user", content: "Hello" } as Message,
          { role: "assistant", content: "Hi there" } as Message,
        ],
      });

      const delta = calculator.calculateDelta(previous, current);

      expect(delta.addedMessages).toBeDefined();
      expect(delta.addedMessages?.length).toBe(1);
      expect(delta.addedMessages?.[0]).toEqual({ role: "assistant", content: "Hi there" });
    });

    it("should detect status changes", () => {
      const previous = createSnapshot({ status: AgentLoopStatus.RUNNING });
      const current = createSnapshot({ status: AgentLoopStatus.COMPLETED });

      const delta = calculator.calculateDelta(previous, current);

      expect(delta.statusChange).toEqual({
        from: AgentLoopStatus.RUNNING,
        to: AgentLoopStatus.COMPLETED,
      });
    });

    it("should detect tool call count changes", () => {
      const previous = createSnapshot({ toolCallCount: 0 });
      const current = createSnapshot({ toolCallCount: 5 });

      const delta = calculator.calculateDelta(previous, current);

      expect(delta.otherChanges?.toolCallCount).toEqual({
        from: 0,
        to: 5,
      });
    });

    it("should detect error changes", () => {
      const previous = createSnapshot({ error: undefined });
      const current = createSnapshot({ error: new Error("Test error") });

      const delta = calculator.calculateDelta(previous, current);

      expect(delta.otherChanges?.error).toBeDefined();
      expect((delta.otherChanges?.error as { from?: unknown; to?: unknown })?.from).toBeUndefined();
      expect(((delta.otherChanges?.error as { from?: unknown; to?: unknown })?.to as Error).message).toBe("Test error");
    });

    it("should detect time changes", () => {
      const now = Date.now();
      const previous = createSnapshot({ startTime: now, endTime: undefined });
      const current = createSnapshot({ startTime: now, endTime: now + 1000 });

      const delta = calculator.calculateDelta(previous, current);

      expect(delta.otherChanges?.endTime).toEqual({
        from: undefined,
        to: now + 1000,
      });
    });

    it("should use optimized message calculation with context", () => {
      const previous = createSnapshot({
        messages: [{ role: "user", content: "Hello" } as Message],
      });
      const current = createSnapshot({
        messages: [
          { role: "user", content: "Hello" } as Message,
          { role: "assistant", content: "Hi" } as Message,
          { role: "user", content: "How are you?" } as Message,
        ],
      });

      const delta = calculator.calculateDelta(previous, current, {
        previousMessageCount: 1,
        currentMessages: current.messages,
      });

      expect(delta.addedMessages?.length).toBe(2);
    });

    it("should handle multiple changes simultaneously", () => {
      const previous = createSnapshot({
        status: AgentLoopStatus.RUNNING,
        toolCallCount: 0,
        messages: [],
      });
      const current = createSnapshot({
        status: AgentLoopStatus.COMPLETED,
        toolCallCount: 3,
        messages: [{ role: "assistant", content: "Done" } as Message],
      });

      const delta = calculator.calculateDelta(previous, current);

      expect(delta.statusChange).toBeDefined();
      expect(delta.addedMessages).toBeDefined();
      expect(delta.otherChanges?.toolCallCount).toBeDefined();
    });
  });

  describe("calculateMessageDelta with context optimization", () => {
    it("should return empty array when no new messages", () => {
      const previous = createSnapshot({
        messages: [{ role: "user", content: "Hello" } as Message],
      });
      const current = createSnapshot({
        messages: [{ role: "user", content: "Hello" } as Message],
      });

      const delta = calculator.calculateDelta(previous, current, {
        previousMessageCount: 1,
        currentMessages: current.messages,
      });

      expect(delta.addedMessages).toBeUndefined();
    });

    it("should fallback to generic calculation without context", () => {
      const previous = createSnapshot({
        messages: [{ role: "user", content: "Hello" } as Message],
      });
      const current = createSnapshot({
        messages: [
          { role: "user", content: "Hello" } as Message,
          { role: "assistant", content: "Hi" } as Message,
        ],
      });

      const delta = calculator.calculateDelta(previous, current);

      expect(delta.addedMessages?.length).toBe(1);
    });
  });
});
