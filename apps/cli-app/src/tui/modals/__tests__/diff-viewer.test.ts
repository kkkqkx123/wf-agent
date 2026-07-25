import { describe, it, expect, vi } from "vitest";
import { DiffViewer } from "../diff-viewer.js";
import { ModalAction } from "../modal.js";

const DIFF = [
  "diff --git a/file.ts b/file.ts",
  "index abc..def 100644",
  "--- a/file.ts",
  "+++ b/file.ts",
  "@@ -1,5 +1,7 @@",
  " const a = 1;",
  "+const b = 2;",
  "-const c = 3;",
  " const d = 4;",
];

describe("DiffViewer", () => {
  it("should capture all keys", () => {
    const viewer = new DiffViewer("patch", ["a"]);
    expect(viewer.capturesAllKeys()).toBe(true);
  });

  it("should close on Esc", () => {
    const onClose = vi.fn();
    const viewer = new DiffViewer("patch", ["a"], onClose);
    const result = viewer.handleKey("\x1b");
    expect(result).toBe(ModalAction.Close);
    expect(onClose).toHaveBeenCalled();
  });

  it("should close on q", () => {
    const onClose = vi.fn();
    const viewer = new DiffViewer("patch", ["a"], onClose);
    viewer.handleKey("q");
    expect(onClose).toHaveBeenCalled();
  });

  it("should navigate via handleKey scroll up", () => {
    const viewer = new DiffViewer("patch", DIFF);
    viewer["renderedHeight"] = 10;
    viewer["scrollOffset"] = 3;
    viewer.handleKey("\x1b[A");
    expect(viewer["scrollOffset"]).toBe(2);
    viewer.handleKey("\x1b[A");
    expect(viewer["scrollOffset"]).toBe(1);
  });

  it("should colorize diff lines in render output", () => {
    const viewer = new DiffViewer("patch.diff", DIFF);
    const lines = viewer.render(80);
    expect(lines[0]).toContain("patch.diff");
    expect(lines.some((l) => l.includes("\x1b[32m"))).toBe(true);
    expect(lines.some((l) => l.includes("\x1b[31m"))).toBe(true);
    expect(lines.some((l) => l.includes("\x1b[36m"))).toBe(true);
    expect(lines[lines.length - 1]!.startsWith("└")).toBe(true);
  });

  it("should show scroll info when content overflows", () => {
    const bigDiff = Array.from({ length: 50 }, (_, i) => ` Line ${i + 1}`);
    const viewer = new DiffViewer("big.diff", bigDiff);
    const lines = viewer.render(80);
    expect(lines.some((l) => l.includes("of 50"))).toBe(true);
  });

  it("invalidate should not throw", () => {
    const viewer = new DiffViewer("t", ["a"]);
    expect(() => viewer.invalidate()).not.toThrow();
  });
});
