import { describe, it, expect, beforeEach, vi } from "vitest";
import { EventRegistry } from "../event-registry.js";

describe("EventRegistry", () => {
  let registry: EventRegistry;

  beforeEach(() => {
    registry = new EventRegistry();
  });

  describe("getEmitter", () => {
    it("should create and return emitter for execution", () => {
      const emitter = registry.getEmitter("exec-1");
      expect(emitter.executionId).toBe("exec-1");
    });

    it("should return same emitter for same execution", () => {
      const emitter1 = registry.getEmitter("exec-1");
      const emitter2 = registry.getEmitter("exec-1");
      expect(emitter1).toBe(emitter2);
    });

    it("should throw on empty executionId", () => {
      expect(() => registry.getEmitter("")).toThrow("Execution ID is required");
    });
  });

  describe("on", () => {
    it("should register listener via registry", () => {
      const listener = vi.fn();
      const unsubscribe = registry.on("NODE_COMPLETED", listener, { executionId: "exec-1" });

      expect(unsubscribe).toBeTypeOf("function");
      unsubscribe();
    });
  });

  describe("once", () => {
    it("should register one-time listener", async () => {
      const listener = vi.fn();
      registry.once("NODE_COMPLETED", listener, { executionId: "exec-1" });

      const emitter = registry.getEmitter("exec-1");
      await emitter.emit({ type: "NODE_COMPLETED", id: "e1", timestamp: Date.now() });
      await emitter.emit({ type: "NODE_COMPLETED", id: "e2", timestamp: Date.now() });

      expect(listener).toHaveBeenCalledTimes(1);
    });
  });

  describe("emit", () => {
    it("should throw on event without executionId", async () => {
      await expect(
        registry.emit({ type: "NODE_COMPLETED", id: "e1", timestamp: Date.now() } as any),
      ).rejects.toThrow("Event must have executionId");
    });

    it("should emit event to the correct execution", async () => {
      const listener1 = vi.fn();
      const listener2 = vi.fn();
      registry.on("NODE_COMPLETED", listener1, { executionId: "exec-1" });
      registry.on("NODE_COMPLETED", listener2, { executionId: "exec-2" });

      await registry.emit({
        type: "NODE_COMPLETED",
        id: "e1",
        timestamp: Date.now(),
        executionId: "exec-1",
      });

      expect(listener1).toHaveBeenCalledTimes(1);
      expect(listener2).not.toHaveBeenCalled();
    });
  });

  describe("waitFor", () => {
    it("should wait for event on specific execution", async () => {
      setTimeout(async () => {
        await registry.emit({
          type: "NODE_COMPLETED",
          id: "e1",
          timestamp: Date.now(),
          executionId: "exec-1",
        });
      }, 10);

      const event = await registry.waitFor("NODE_COMPLETED", "exec-1");
      expect(event.id).toBe("e1");
    });
  });

  describe("onGlobal", () => {
    it("should receive all events across executions", async () => {
      const globalListener = vi.fn();
      registry.onGlobal(globalListener);

      await registry.emit({
        type: "NODE_COMPLETED",
        id: "e1",
        timestamp: Date.now(),
        executionId: "exec-1",
      });
      await registry.emit({
        type: "NODE_STARTED",
        id: "e2",
        timestamp: Date.now(),
        executionId: "exec-2",
      });

      expect(globalListener).toHaveBeenCalledTimes(2);
    });

    it("should return unsubscribe function", () => {
      const listener = vi.fn();
      const unsubscribe = registry.onGlobal(listener);
      expect(unsubscribe).toBeTypeOf("function");
    });

    it("should stop receiving events after unsubscribe", async () => {
      const listener = vi.fn();
      const unsubscribe = registry.onGlobal(listener);
      unsubscribe();

      await registry.emit({
        type: "NODE_COMPLETED",
        id: "e1",
        timestamp: Date.now(),
        executionId: "exec-1",
      });
      expect(listener).not.toHaveBeenCalled();
    });
  });

  describe("cleanupExecutionListeners", () => {
    it("should cleanup emitter and metrics for execution", () => {
      registry.on("NODE_COMPLETED", vi.fn(), { executionId: "exec-1" });
      registry.on("NODE_STARTED", vi.fn(), { executionId: "exec-1" });

      const cleaned = registry.cleanupExecutionListeners("exec-1");
      expect(cleaned).toBe(2);
    });

    it("should return 0 for empty executionId", () => {
      const cleaned = registry.cleanupExecutionListeners("");
      expect(cleaned).toBe(0);
    });

    it("should return 0 for non-existent execution", () => {
      const cleaned = registry.cleanupExecutionListeners("non-existent");
      expect(cleaned).toBe(0);
    });
  });

  describe("getExecutionListenerStats", () => {
    it("should return stats for all executions", () => {
      registry.on("NODE_COMPLETED", vi.fn(), { executionId: "exec-1" });
      registry.on("NODE_STARTED", vi.fn(), { executionId: "exec-1" });
      registry.on("NODE_FAILED", vi.fn(), { executionId: "exec-2" });

      const stats = registry.getExecutionListenerStats();
      expect(stats.get("exec-1")).toBe(2);
      expect(stats.get("exec-2")).toBe(1);
    });
  });

  describe("getMetricsCollector", () => {
    it("should return the metrics collector instance", () => {
      const collector = registry.getMetricsCollector();
      expect(collector).toBeDefined();
    });
  });

  describe("getEventStatistics", () => {
    it("should return stats for events emitted through registry", async () => {
      await registry.emit({
        type: "NODE_COMPLETED",
        id: "e1",
        timestamp: Date.now(),
        executionId: "exec-1",
      });

      const stats = registry.getEventStatistics("NODE_COMPLETED");
      expect(stats).toBeDefined();
      expect(stats!.count).toBe(1);
      expect(stats!.byExecution.get("exec-1")).toBe(1);
    });

    it("should return undefined for non-tracked event type", () => {
      const stats = registry.getEventStatistics("NON_EXISTENT");
      expect(stats).toBeUndefined();
    });
  });

  describe("getMetricsSummary", () => {
    it("should return a metrics summary with recorded data", async () => {
      await registry.emit({
        type: "NODE_COMPLETED",
        id: "e1",
        timestamp: Date.now(),
        executionId: "exec-1",
      });

      const summary = registry.getMetricsSummary();
      expect(summary).toBeDefined();
      expect(summary.totalEvents).toBe(1);
      expect(summary.byEventType.has("NODE_COMPLETED")).toBe(true);
    });
  });
});
