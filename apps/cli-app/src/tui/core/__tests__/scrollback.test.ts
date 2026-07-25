import { describe, it, expect } from "vitest";
import { ScrollbackBuffer, MAX_SCROLLBACK_ROWS } from "../scrollback.js";

describe("ScrollbackBuffer", () => {
  it("should start empty", () => {
    const buf = new ScrollbackBuffer();
    expect(buf.lineCount).toBe(0);
    expect(buf.entryCount).toBe(0);
  });

  it("should append a single line", () => {
    const buf = new ScrollbackBuffer();
    buf.appendLine("hello");
    expect(buf.lineCount).toBe(1);
    expect(buf.entryCount).toBe(1);
  });

  it("should append multiple lines as an entry", () => {
    const buf = new ScrollbackBuffer();
    buf.append(["a", "b", "c"]);
    expect(buf.lineCount).toBe(3);
    expect(buf.entryCount).toBe(1);
  });

  it("should retrieve lines by index", () => {
    const buf = new ScrollbackBuffer();
    buf.append(["a", "b"]);
    buf.append(["c", "d"]);
    const lines = buf.getLines(0, 4);
    expect(lines).toEqual(["a", "b", "c", "d"]);
  });

  it("should retrieve partial range", () => {
    const buf = new ScrollbackBuffer();
    buf.append(["a", "b", "c", "d", "e"]);
    expect(buf.getLines(1, 3)).toEqual(["b", "c", "d"]);
  });

  it("should return empty array for out-of-range", () => {
    const buf = new ScrollbackBuffer();
    buf.append(["a"]);
    expect(buf.getLines(10, 5)).toEqual([]);
  });

  it("should find entry containing a line index", () => {
    const buf = new ScrollbackBuffer<string>();
    buf.append(["a", "b"], "marker1");
    buf.append(["c", "d"], "marker2");
    const entry = buf.getEntryContaining(2);
    expect(entry).toBeDefined();
    expect(entry!.marker).toBe("marker2");
    expect(entry!.lines).toEqual(["c", "d"]);
  });

  it("should prune when exceeding maxLines", () => {
    const buf = new ScrollbackBuffer(3);
    buf.append(["a", "b"]);
    buf.append(["c", "d"]);
    expect(buf.lineCount).toBe(2);
    expect(buf.entryCount).toBe(1);
    expect(buf.getLines(0, 10)).toEqual(["c", "d"]);
  });

  it("should use default MAX_SCROLLBACK_ROWS", () => {
    const buf = new ScrollbackBuffer();
    expect(buf["maxLines"]).toBe(MAX_SCROLLBACK_ROWS);
  });

  it("should clear all entries", () => {
    const buf = new ScrollbackBuffer();
    buf.append(["a", "b"]);
    buf.clear();
    expect(buf.lineCount).toBe(0);
    expect(buf.entryCount).toBe(0);
  });

  it("should be iterable", () => {
    const buf = new ScrollbackBuffer<string>();
    buf.append(["a"], "m1");
    buf.append(["b"], "m2");
    const entries = [...buf];
    expect(entries.length).toBe(2);
    expect(entries[0]!.marker).toBe("m1");
    expect(entries[1]!.marker).toBe("m2");
  });

  it("should store timestamps on entries", () => {
    const buf = new ScrollbackBuffer();
    const before = Date.now();
    buf.append(["a"]);
    const after = Date.now();
    const entry = buf.getEntryContaining(0);
    expect(entry!.timestamp).toBeGreaterThanOrEqual(before);
    expect(entry!.timestamp).toBeLessThanOrEqual(after);
  });
});
