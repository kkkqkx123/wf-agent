import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { SignalHandler } from "../signal-handler.js";

describe("SignalHandler", () => {
  let handler: SignalHandler;

  beforeEach(() => {
    handler = new SignalHandler();
  });

  afterEach(() => {
    handler.disarm();
  });

  it("should start disarmed", () => {
    expect(handler["armed"]).toBe(false);
  });

  it("should arm and register signal handlers", () => {
    handler.arm();
    expect(handler["armed"]).toBe(true);
    expect(process.listeners("SIGINT").length).toBeGreaterThan(0);
  });

  it("should be idempotent on arm", () => {
    handler.arm();
    const count = process.listeners("SIGINT").length;
    handler.arm();
    expect(process.listeners("SIGINT").length).toBe(count);
  });

  it("should disarm and clean up handlers", () => {
    handler.arm();
    const sigintListeners = process.listeners("SIGINT").length;
    handler.disarm();
    expect(handler["armed"]).toBe(false);
    expect(process.listeners("SIGINT").length).toBeLessThan(sigintListeners);
  });

  it("should call interrupt callbacks on SIGINT", () => {
    const cb = vi.fn();
    handler.onInterrupt(cb);
    handler.arm();
    process.emit("SIGINT");
    expect(cb).toHaveBeenCalledTimes(1);
  });

  it("should debounce rapid SIGINT within 500ms", () => {
    const cb = vi.fn();
    handler.onInterrupt(cb);
    handler.arm();
    process.emit("SIGINT");
    process.emit("SIGINT");
    expect(cb).toHaveBeenCalledTimes(1);
  });

  it("should call suspend and resume callbacks", () => {
    if (process.platform === "win32") return;

    const onSuspend = vi.fn();
    const onResume = vi.fn();
    handler.onSuspend(onSuspend);
    handler.onResume(onResume);
    handler.arm();

    process.emit("SIGTSTP");
    expect(onSuspend).toHaveBeenCalled();

    handler["suspended"] = true;
    process.emit("SIGCONT");
    expect(onResume).toHaveBeenCalled();
  });

  it("should be idempotent on disarm", () => {
    handler.disarm();
    handler.disarm();
  });

  it("should not leak handlers after disarm", () => {
    handler.arm();
    handler.disarm();
    process.emit("SIGINT");
  });
});
