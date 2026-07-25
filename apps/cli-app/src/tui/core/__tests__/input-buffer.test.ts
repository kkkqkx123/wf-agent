import { describe, it, expect } from "vitest";
import { InputBuffer } from "../input-buffer.js";

describe("InputBuffer", () => {
  it("should start empty", () => {
    const buf = new InputBuffer();
    expect(buf.length).toBe(0);
  });

  it("should push and shift items", () => {
    const buf = new InputBuffer();
    buf.push("a");
    buf.push("b");
    expect(buf.length).toBe(2);
    expect(buf.shift()).toBe("a");
    expect(buf.shift()).toBe("b");
    expect(buf.shift()).toBeNull();
    expect(buf.length).toBe(0);
  });

  it("should flush all items", () => {
    const buf = new InputBuffer();
    buf.push("x");
    buf.push("y");
    buf.push("z");
    const flushed = buf.flush();
    expect(flushed).toEqual(["x", "y", "z"]);
    expect(buf.length).toBe(0);
  });

  it("should clear items", () => {
    const buf = new InputBuffer();
    buf.push("a");
    buf.push("b");
    buf.clear();
    expect(buf.length).toBe(0);
    expect(buf.shift()).toBeNull();
  });

  it("should pause and buffer incoming items while paused", () => {
    const buf = new InputBuffer();
    buf.push("a");
    buf.pause();
    expect(buf.paused).toBe(true);
    buf.push("b");
    buf.push("c");
    expect(buf.length).toBe(1);

    buf.resume();
    expect(buf.paused).toBe(false);
    expect(buf.length).toBe(3);
    expect(buf.shift()).toBe("a");
    expect(buf.shift()).toBe("b");
    expect(buf.shift()).toBe("c");
  });

  it("should discard pending items on clear even when paused", () => {
    const buf = new InputBuffer();
    buf.pause();
    buf.push("a");
    buf.push("b");
    buf.clear();
    expect(buf.length).toBe(0);
    buf.resume();
    expect(buf.length).toBe(0);
  });
});
