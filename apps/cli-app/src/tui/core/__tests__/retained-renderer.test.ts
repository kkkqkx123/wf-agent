import { describe, it, expect, vi, beforeEach } from "vitest";
import { RetainedRenderer } from "../retained-renderer.js";
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

describe("RetainedRenderer", () => {
  let terminal: Terminal;
  let renderer: RetainedRenderer;

  beforeEach(() => {
    terminal = createMockTerminal();
    renderer = new RetainedRenderer(terminal);
  });

  it("should start with zero full redraws", () => {
    expect(renderer.stats.fullRedraws).toBe(0);
  });

  it("should perform full render on first call", () => {
    renderer.render(["line1", "line2"], 80, 24);
    expect(terminal.write).toHaveBeenCalledTimes(1);
    const output = (terminal.write as ReturnType<typeof vi.fn>).mock.calls[0][0] as string;
    expect(output).toContain("\x1b[?2026h");
    expect(output).toContain("line1");
    expect(output).toContain("line2");
    expect(output).toContain("\x1b[?2026l");
    expect(renderer.stats.fullRedraws).toBe(1);
  });

  it("should skip render when lines are unchanged", () => {
    renderer.render(["a", "b"], 80, 24);
    (terminal.write as ReturnType<typeof vi.fn>).mockClear();
    renderer.render(["a", "b"], 80, 24);
    expect(terminal.write).not.toHaveBeenCalled();
  });

  it("should perform differential render on line change", () => {
    renderer.render(["a", "b"], 80, 24);
    (terminal.write as ReturnType<typeof vi.fn>).mockClear();
    renderer.render(["a", "c"], 80, 24);
    expect(terminal.write).toHaveBeenCalledTimes(1);
    const output = (terminal.write as ReturnType<typeof vi.fn>).mock.calls[0][0] as string;
    expect(output).toContain("\x1b[2K");
    expect(output).toContain("c");
    expect(output).not.toContain("b");
  });

  it("should perform full re-render on width change", () => {
    renderer.render(["a"], 80, 24);
    (terminal.write as ReturnType<typeof vi.fn>).mockClear();
    renderer.render(["a"], 100, 24);
    expect(terminal.write).toHaveBeenCalledTimes(1);
    const output = (terminal.write as ReturnType<typeof vi.fn>).mock.calls[0][0] as string;
    expect(output).toContain("\x1b[2J\x1b[H");
  });

  it("should perform full re-render on height change", () => {
    renderer.render(["a"], 80, 24);
    (terminal.write as ReturnType<typeof vi.fn>).mockClear();
    renderer.render(["a"], 80, 30);
    expect(terminal.write).toHaveBeenCalledTimes(1);
  });

  it("should clear extra lines when content shrinks", () => {
    renderer.render(["a", "b", "c"], 80, 24);
    (terminal.write as ReturnType<typeof vi.fn>).mockClear();
    renderer.render(["a"], 80, 24);
    const output = (terminal.write as ReturnType<typeof vi.fn>).mock.calls[0][0] as string;
    expect(output).toContain("\x1b[2K");
    expect(output).toContain("\x1b[2A");
  });

  it("should reset state correctly", () => {
    renderer.render(["a"], 80, 24);
    renderer.reset();
    expect(renderer.stats.fullRedraws).toBe(1);
    (terminal.write as ReturnType<typeof vi.fn>).mockClear();
    renderer.render(["b"], 80, 24);
    expect(renderer.stats.fullRedraws).toBe(2);
  });

  it("should delegate clearScreen to terminal", () => {
    renderer.clearScreen();
    expect(terminal.clearScreen).toHaveBeenCalledTimes(1);
  });

  it("should write sync begin/end markers", () => {
    renderer.beginSync();
    expect(terminal.write).toHaveBeenCalledWith("\x1b[?2026h");
    (terminal.write as ReturnType<typeof vi.fn>).mockClear();
    renderer.endSync();
    expect(terminal.write).toHaveBeenCalledWith("\x1b[?2026l");
  });

  it("should move cursor to end on shutdown", () => {
    renderer.render(["a", "b", "c"], 80, 24);
    (terminal.write as ReturnType<typeof vi.fn>).mockClear();
    renderer.shutdown();
    expect(terminal.showCursor).toHaveBeenCalled();
  });

  it("flush should be a no-op", () => {
    renderer.flush();
    expect(terminal.write).not.toHaveBeenCalled();
  });
});
