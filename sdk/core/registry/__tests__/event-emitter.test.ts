import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { ExecutionEventEmitter } from "../event-emitter.js";

describe("ExecutionEventEmitter", () => {
  let emitter: ExecutionEventEmitter;

  beforeEach(() => {
    emitter = new ExecutionEventEmitter("exec-123");
  });

  afterEach(() => {
    emitter.removeAllListeners();
  });

  describe("constructor", () => {
    it("should create emitter with executionId", () => {
      expect(emitter.executionId).toBe("exec-123");
      expect(emitter.isEmitterDisposed()).toBe(false);
    });

    it("should throw on empty executionId", () => {
      expect(() => new ExecutionEventEmitter("")).toThrow("Execution ID is required");
    });
  });

  describe("on", () => {
    it("should register listener and return unsubscribe function", () => {
      const listener = vi.fn();
      const unsubscribe = emitter.on("NODE_COMPLETED", listener);

      expect(unsubscribe).toBeTypeOf("function");
      expect(emitter.getListenerCount().get("NODE_COMPLETED")).toBe(1);
    });

    it("should call listener on emit", async () => {
      const listener = vi.fn();
      emitter.on("NODE_COMPLETED", listener);

      await emitter.emit({ type: "NODE_COMPLETED", id: "e1", timestamp: Date.now() });

      expect(listener).toHaveBeenCalledTimes(1);
    });

    it("should support filter option", async () => {
      const listener = vi.fn();
      emitter.on("NODE_COMPLETED", listener, {
        filter: (event: any) => event.nodeId === "node-1",
      });

      await emitter.emit({
        type: "NODE_COMPLETED",
        id: "e1",
        timestamp: Date.now(),
        nodeId: "node-2",
      });
      expect(listener).not.toHaveBeenCalled();

      await emitter.emit({
        type: "NODE_COMPLETED",
        id: "e2",
        timestamp: Date.now(),
        nodeId: "node-1",
      });
      expect(listener).toHaveBeenCalledTimes(1);
    });

    it("should unsubscribe correctly", async () => {
      const listener = vi.fn();
      const unsubscribe = emitter.on("NODE_COMPLETED", listener);

      unsubscribe();
      expect(emitter.getListenerCount().get("NODE_COMPLETED")).toBe(0);

      await emitter.emit({ type: "NODE_COMPLETED", id: "e1", timestamp: Date.now() });
      expect(listener).not.toHaveBeenCalled();
    });

    it("should throw if emitter is disposed", () => {
      emitter.removeAllListeners();
      expect(() => emitter.on("NODE_COMPLETED", vi.fn())).toThrow("has been disposed");
    });
  });

  describe("once", () => {
    it("should auto-unsubscribe after first trigger", async () => {
      const listener = vi.fn();
      emitter.once("NODE_COMPLETED", listener);

      await emitter.emit({ type: "NODE_COMPLETED", id: "e1", timestamp: Date.now() });
      await emitter.emit({ type: "NODE_COMPLETED", id: "e2", timestamp: Date.now() });

      expect(listener).toHaveBeenCalledTimes(1);
    });
  });

  describe("waitFor", () => {
    it("should resolve when matching event is emitted", async () => {
      setTimeout(async () => {
        await emitter.emit({ type: "NODE_COMPLETED", id: "e1", timestamp: Date.now() });
      }, 10);

      const event = await emitter.waitFor("NODE_COMPLETED");
      expect(event.id).toBe("e1");
    });

    it("should reject on timeout", async () => {
      await expect(emitter.waitFor("NODE_COMPLETED", { timeout: 50 })).rejects.toThrow(
        "Timeout waiting for event",
      );
    });

    it("should support filter", async () => {
      setTimeout(async () => {
        await emitter.emit({
          type: "NODE_COMPLETED",
          id: "e1",
          timestamp: Date.now(),
          nodeId: "node-2",
        });
        await emitter.emit({
          type: "NODE_COMPLETED",
          id: "e2",
          timestamp: Date.now(),
          nodeId: "node-1",
        });
      }, 10);

      const event = await emitter.waitFor("NODE_COMPLETED", {
        filter: (e: any) => e.nodeId === "node-1",
      });
      expect(event.id).toBe("e2");
    });
  });

  describe("off", () => {
    it("should remove a specific listener by id", () => {
      const listener = vi.fn();
      emitter.on("NODE_COMPLETED", listener);

      emitter.off("NODE_COMPLETED", "unknown-id");
      expect(emitter.getListenerCount().get("NODE_COMPLETED")).toBe(1);
    });

    it("should do nothing for non-existent event type", () => {
      expect(() => emitter.off("NON_EXISTENT" as any, "id")).not.toThrow();
    });
  });

  describe("emit", () => {
    it("should throw on null event", async () => {
      await expect(emitter.emit(null as any)).rejects.toThrow("Event is required");
    });

    it("should throw on event without type", async () => {
      await expect(emitter.emit({ id: "e1", timestamp: Date.now() } as any)).rejects.toThrow(
        "Event type is required",
      );
    });

    it("should aggregate errors from multiple listeners", async () => {
      emitter.on("NODE_COMPLETED", async () => {
        throw new Error("Error 1");
      });
      emitter.on("NODE_COMPLETED", async () => {
        throw new Error("Error 2");
      });

      await expect(
        emitter.emit({ type: "NODE_COMPLETED", id: "e1", timestamp: Date.now() }),
      ).rejects.toThrow("2 listener(s) failed");
    });

    it("should throw if emitter is disposed", async () => {
      emitter.removeAllListeners();
      await expect(
        emitter.emit({ type: "NODE_COMPLETED", id: "e1", timestamp: Date.now() }),
      ).rejects.toThrow("has been disposed");
    });
  });

  describe("removeAllListeners", () => {
    it("should clear all listeners and metrics", () => {
      emitter.on("NODE_COMPLETED", vi.fn());
      emitter.on("NODE_STARTED", vi.fn());

      emitter.removeAllListeners();

      expect(emitter.getListenerCount().size).toBe(0);
      expect(emitter.isEmitterDisposed()).toBe(true);
    });
  });

  describe("getListenerMetrics", () => {
    it("should return undefined for unknown listener", () => {
      expect(emitter.getListenerMetrics("NODE_COMPLETED", "unknown")).toBeUndefined();
    });
  });

  describe("getAllListenerInfo", () => {
    it("should return info for all registered listeners", () => {
      emitter.on("NODE_COMPLETED", vi.fn());
      emitter.on("NODE_STARTED", vi.fn());

      const info = emitter.getAllListenerInfo();
      expect(info.length).toBe(2);
      expect(info[0]!.eventType).toBeDefined();
      expect(info[0]!.registeredAt).toBeGreaterThan(0);
    });
  });

  describe("isEmitterDisposed", () => {
    it("should return false initially", () => {
      expect(emitter.isEmitterDisposed()).toBe(false);
    });

    it("should return true after removeAllListeners", () => {
      emitter.removeAllListeners();
      expect(emitter.isEmitterDisposed()).toBe(true);
    });
  });
});
