import { describe, it, expect, vi } from "vitest";
import { FileViewer } from "../file-viewer.js";
import { ModalAction } from "../modal.js";

const CONTENT = Array.from({ length: 50 }, (_, i) => `Line ${i + 1}`);

describe("FileViewer", () => {
  it("should capture all keys", () => {
    const viewer = new FileViewer("test.txt", ["a"]);
    expect(viewer.capturesAllKeys()).toBe(true);
  });

  it("should close on Esc", () => {
    const onClose = vi.fn();
    const viewer = new FileViewer("test.txt", ["a"], onClose);
    const result = viewer.handleKey("\x1b");
    expect(result).toBe(ModalAction.Close);
    expect(onClose).toHaveBeenCalled();
  });

  it("should close on q", () => {
    const onClose = vi.fn();
    const viewer = new FileViewer("test.txt", ["a"], onClose);
    viewer.handleKey("q");
    expect(onClose).toHaveBeenCalled();
  });

  it("should navigate via handleKey scroll up", () => {
    const viewer = new FileViewer("test.txt", CONTENT);
    viewer["renderedHeight"] = 28;
    viewer["scrollOffset"] = 5;
    viewer.handleKey("\x1b[A");
    expect(viewer["scrollOffset"]).toBe(4);
    viewer.handleKey("\x1b[A");
    expect(viewer["scrollOffset"]).toBe(3);
  });

  it("should not scroll before start", () => {
    const viewer = new FileViewer("test.txt", CONTENT);
    viewer["scrollOffset"] = 0;
    viewer.handleKey("\x1b[A");
    expect(viewer["scrollOffset"]).toBe(0);
  });

  it("should render content with scrolling", () => {
    const viewer = new FileViewer("test.txt", CONTENT);
    const lines = viewer.render(80);
    expect(lines[0]).toContain("test.txt");
    expect(lines.some((l) => l.includes("Line 1"))).toBe(true);
    expect(lines.some((l) => l.includes("of 50"))).toBe(true);
  });

  it("should not scroll before start", () => {
    const viewer = new FileViewer("test.txt", CONTENT);
    viewer["scrollOffset"] = 0;
    viewer.handleKey("k");
    expect(viewer["scrollOffset"]).toBe(0);
  });

  it("should render content with title", () => {
    const viewer = new FileViewer("myfile.txt", ["Hello", "World"]);
    const lines = viewer.render(80);
    expect(lines[0]).toContain("myfile.txt");
    expect(lines.some((l) => l.includes("Hello"))).toBe(true);
    expect(lines.some((l) => l.includes("World"))).toBe(true);
    expect(lines[lines.length - 1]!.startsWith("└")).toBe(true);
  });

  it("should show scroll info when content overflows", () => {
    const viewer = new FileViewer("big.txt", CONTENT);
    const lines = viewer.render(80);
    expect(lines.some((l) => l.includes("Line 1"))).toBe(true);
    expect(lines.some((l) => l.includes("of 50"))).toBe(true);
  });

  it("invalidate should not throw", () => {
    const viewer = new FileViewer("t", ["a"]);
    expect(() => viewer.invalidate()).not.toThrow();
  });
});
