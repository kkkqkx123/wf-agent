import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { EventScheduler, Priority } from "../event-scheduler.js";

describe("EventScheduler", () => {
  let scheduler: EventScheduler;

  beforeEach(() => {
    vi.useFakeTimers();
    scheduler = new EventScheduler();
  });

  afterEach(() => {
    scheduler.stop();
    vi.useRealTimers();
  });

  it("should start with no sources", () => {
    expect(scheduler.hasSource("test")).toBe(false);
  });

  it("should add a source", () => {
    const handler = vi.fn();
    scheduler.addSource("test", {
      interval: 100,
      handler,
      priority: Priority.Render,
      enabled: () => true,
    });
    expect(scheduler.hasSource("test")).toBe(true);
  });

  it("should call handler on interval after start", () => {
    const handler = vi.fn();
    scheduler.addSource("test", {
      interval: 100,
      handler,
      priority: Priority.Render,
      enabled: () => true,
    });
    scheduler.start();
    expect(handler).not.toHaveBeenCalled();
    vi.advanceTimersByTime(100);
    expect(handler).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(100);
    expect(handler).toHaveBeenCalledTimes(2);
  });

  it("should respect enabled flag", () => {
    const handler = vi.fn();
    let enabled = false;
    scheduler.addSource("test", {
      interval: 100,
      handler,
      priority: Priority.Render,
      enabled: () => enabled,
    });
    scheduler.start();
    vi.advanceTimersByTime(300);
    expect(handler).not.toHaveBeenCalled();
    enabled = true;
    vi.advanceTimersByTime(100);
    expect(handler).toHaveBeenCalledTimes(1);
  });

  it("should not call handler before start", () => {
    const handler = vi.fn();
    scheduler.addSource("test", {
      interval: 100,
      handler,
      priority: Priority.Render,
      enabled: () => true,
    });
    vi.advanceTimersByTime(200);
    expect(handler).not.toHaveBeenCalled();
  });

  it("should remove a source", () => {
    const handler = vi.fn();
    scheduler.addSource("test", {
      interval: 100,
      handler,
      priority: Priority.Render,
      enabled: () => true,
    });
    scheduler.start();
    scheduler.removeSource("test");
    vi.advanceTimersByTime(200);
    expect(handler).not.toHaveBeenCalled();
    expect(scheduler.hasSource("test")).toBe(false);
  });

  it("should replace existing source with same name", () => {
    const handler1 = vi.fn();
    const handler2 = vi.fn();
    scheduler.addSource("dup", {
      interval: 100,
      handler: handler1,
      priority: Priority.Render,
      enabled: () => true,
    });
    scheduler.addSource("dup", {
      interval: 100,
      handler: handler2,
      priority: Priority.Render,
      enabled: () => true,
    });
    scheduler.start();
    vi.advanceTimersByTime(100);
    expect(handler1).not.toHaveBeenCalled();
    expect(handler2).toHaveBeenCalledTimes(1);
  });

  it("should stop all sources", () => {
    const handler = vi.fn();
    scheduler.addSource("test", {
      interval: 100,
      handler,
      priority: Priority.Render,
      enabled: () => true,
    });
    scheduler.start();
    vi.advanceTimersByTime(100);
    expect(handler).toHaveBeenCalledTimes(1);
    scheduler.stop();
    vi.advanceTimersByTime(200);
    expect(handler).toHaveBeenCalledTimes(1);
  });

  it("should be idempotent on start", () => {
    scheduler.start();
    scheduler.start();
    scheduler.start();
  });

  it("should be idempotent on stop", () => {
    scheduler.stop();
    scheduler.stop();
  });

  it("should track tick counts", () => {
    const handler = vi.fn();
    scheduler.addSource("ticker", {
      interval: 100,
      handler,
      priority: Priority.Render,
      enabled: () => true,
    });
    scheduler.start();
    expect(scheduler.getTickCount("ticker")).toBe(0);
    vi.advanceTimersByTime(100);
    expect(scheduler.getTickCount("ticker")).toBe(1);
    vi.advanceTimersByTime(100);
    expect(scheduler.getTickCount("ticker")).toBe(2);
  });

  it("should return 0 tick count for unknown source", () => {
    expect(scheduler.getTickCount("nonexistent")).toBe(0);
  });

  it("should execute deferred tasks in priority order", async () => {
    const order: number[] = [];
    scheduler.deferTask("low", () => order.push(3), Priority.Idle);
    scheduler.deferTask("high", () => order.push(1), Priority.Critical);
    scheduler.deferTask("mid", () => order.push(2), Priority.Render);
    expect(order).toEqual([]);
    await vi.advanceTimersByTimeAsync(0);
    expect(order).toEqual([1, 2, 3]);
  });

  it("should cancel deferred tasks", () => {
    const handler = vi.fn();
    scheduler.deferTask("cancelme", handler, Priority.Idle);
    scheduler.cancelDeferred("cancelme");
    vi.advanceTimersByTime(0);
    expect(handler).not.toHaveBeenCalled();
  });

  it("should clear deferred tasks on stop", () => {
    const handler = vi.fn();
    scheduler.deferTask("lost", handler, Priority.Idle);
    scheduler.stop();
    vi.advanceTimersByTime(0);
    expect(handler).not.toHaveBeenCalled();
  });
});
