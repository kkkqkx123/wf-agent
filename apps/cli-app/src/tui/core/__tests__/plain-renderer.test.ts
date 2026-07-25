import { describe, it, expect, vi, beforeEach } from "vitest";
import { PlainRenderer } from "../plain-renderer.js";

describe("PlainRenderer", () => {
  let renderer: PlainRenderer;
  let writeSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    renderer = new PlainRenderer();
    writeSpy = vi.spyOn(process.stdout, "write").mockImplementation(() => true);
  });

  it("should write each line with newline", () => {
    renderer.render(["hello", "world"], 80, 24);
    expect(writeSpy).toHaveBeenCalledTimes(2);
    expect(writeSpy).toHaveBeenNthCalledWith(1, "hello\n");
    expect(writeSpy).toHaveBeenNthCalledWith(2, "world\n");
  });

  it("should write single line", () => {
    renderer.render(["only"], 80, 24);
    expect(writeSpy).toHaveBeenCalledTimes(1);
    expect(writeSpy).toHaveBeenCalledWith("only\n");
  });

  it("should handle empty lines", () => {
    renderer.render([], 80, 24);
    expect(writeSpy).not.toHaveBeenCalled();
  });

  it("reset should not throw", () => {
    expect(() => renderer.reset()).not.toThrow();
  });

  it("clearScreen should not throw", () => {
    expect(() => renderer.clearScreen()).not.toThrow();
    expect(writeSpy).not.toHaveBeenCalled();
  });

  it("flush should not throw", () => {
    expect(() => renderer.flush()).not.toThrow();
  });

  it("shutdown should not throw", () => {
    expect(() => renderer.shutdown()).not.toThrow();
  });

  it("beginSync should not throw", () => {
    expect(() => renderer.beginSync()).not.toThrow();
    expect(writeSpy).not.toHaveBeenCalled();
  });

  it("endSync should not throw", () => {
    expect(() => renderer.endSync()).not.toThrow();
    expect(writeSpy).not.toHaveBeenCalled();
  });
});
