import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { TerminalGuard } from "../terminal-guard.js";
import type { Terminal } from "../terminal.js";

function createMockTerminal(): Terminal {
  return {
    write: vi.fn(),
    showCursor: vi.fn(),
    hideCursor: vi.fn(),
    clearScreen: vi.fn(),
    start: vi.fn(),
    stop: vi.fn(),
    drainInput: vi.fn(),
    moveBy: vi.fn(),
    clearLine: vi.fn(),
    clearFromCursor: vi.fn(),
    setTitle: vi.fn(),
    setProgress: vi.fn(),
    get columns() { return 80; },
    get rows() { return 24; },
    get capabilities() { return {} as any; },
  };
}

describe("TerminalGuard", () => {
  let guard: TerminalGuard;
  let terminal: Terminal;

  beforeEach(() => {
    TerminalGuard["instance"] = null;
    guard = TerminalGuard.getInstance();
    terminal = createMockTerminal();
  });

  afterEach(() => {
    guard.disarm();
  });

  it("should be a singleton", () => {
    const g2 = TerminalGuard.getInstance();
    expect(g2).toBe(guard);
  });

  it("should start disarmed", () => {
    expect(guard["armed"]).toBe(false);
  });

  it("should arm with terminal", () => {
    guard.arm(terminal);
    expect(guard["armed"]).toBe(true);
    expect(guard["terminal"]).toBe(terminal);
  });

  it("should be idempotent on arm", () => {
    guard.arm(terminal);
    const t2 = createMockTerminal();
    guard.arm(t2);
    expect(guard["terminal"]).toBe(terminal);
  });

  it("should disarm and clear terminal reference", () => {
    guard.arm(terminal);
    guard.disarm();
    expect(guard["armed"]).toBe(false);
    expect(guard["terminal"]).toBeNull();
    expect(guard["restoreFns"].length).toBe(0);
  });

  it("should register signal handlers on arm", () => {
    const sigintCount = process.listeners("SIGINT").length;
    guard.arm(terminal);
    expect(process.listeners("SIGINT").length).toBeGreaterThan(sigintCount);
  });

  it("should clean up signal handlers on disarm", () => {
    guard.arm(terminal);
    const sigintCount = process.listeners("SIGINT").length;
    guard.disarm();
    expect(process.listeners("SIGINT").length).toBeLessThan(sigintCount);
  });

  it("should restore terminal state on restoreTerminal", () => {
    const writeSpy = vi.spyOn(process.stdout, "write").mockImplementation(() => true);
    const origRaw = Object.getOwnPropertyDescriptor(process.stdin, "isRaw")?.value;
    Object.defineProperty(process.stdin, "isRaw", { value: false, configurable: true, writable: true });

    guard.restoreTerminal();
    expect(writeSpy).toHaveBeenCalledWith("\x1b[?25h");
    expect(writeSpy).toHaveBeenCalledWith("\x1b[?2004l");

    writeSpy.mockRestore();
  });
});
