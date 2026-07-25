import { describe, it, expect } from "vitest";
import { DiffPanel } from "../diff-panel.js";

const DIFF = [
  "diff --git a/src/index.ts b/src/index.ts",
  "index abc123..def456 100644",
  "--- a/src/index.ts",
  "+++ b/src/index.ts",
  "@@ -1,5 +1,7 @@",
  " import { foo } from './foo.js';",
  " const a = 1;",
  "+const b = 2;",
  "-const c = 3;",
  " const d = 4;",
];

describe("DiffPanel", () => {
  it("should render diff lines", () => {
    const panel = new DiffPanel();
    panel.setDiff(DIFF);
    const lines = panel.render(80);
    expect(lines.length).toBeGreaterThan(0);
  });

  it("should colorize added lines green", () => {
    const panel = new DiffPanel();
    panel.setDiff(DIFF);
    const lines = panel.render(80);
    const added = lines.find((l) => l.includes("+const b"));
    expect(added).toBeDefined();
    expect(added).toContain("\x1b[32m");
  });

  it("should colorize removed lines red", () => {
    const panel = new DiffPanel();
    panel.setDiff(DIFF);
    const lines = panel.render(80);
    const removed = lines.find((l) => l.includes("-const c"));
    expect(removed).toBeDefined();
    expect(removed).toContain("\x1b[31m");
  });

  it("should colorize hunk headers cyan", () => {
    const panel = new DiffPanel();
    panel.setDiff(DIFF);
    const lines = panel.render(80);
    const hunk = lines.find((l) => l.includes("@@"));
    expect(hunk).toBeDefined();
    expect(hunk).toContain("\x1b[36m");
  });

  it("should colorize metadata lines gray", () => {
    const panel = new DiffPanel();
    panel.setDiff(DIFF);
    const lines = panel.render(80);
    const meta = lines.find((l) => l.includes("diff --git"));
    expect(meta).toBeDefined();
    expect(meta).toContain("\x1b[90m");
  });

  it("should show title when provided", () => {
    const panel = new DiffPanel({ title: "myfile.ts" });
    panel.setDiff(["line1"]);
    const lines = panel.render(80);
    expect(lines[0]).toContain("myfile.ts");
  });

  it("should scroll up and down", () => {
    const bigDiff = Array.from({ length: 50 }, (_, i) => `Line ${i + 1}`);
    const panel = new DiffPanel({ maxVisibleLines: 10 });
    panel.setDiff(bigDiff);
    panel.scrollDown(2);
    expect(panel["scrollOffset"]).toBe(2);
    panel.scrollUp(1);
    expect(panel["scrollOffset"]).toBe(1);
  });

  it("should scroll to top and bottom", () => {
    const bigDiff = Array.from({ length: 50 }, (_, i) => `Line ${i + 1}`);
    const panel = new DiffPanel({ maxVisibleLines: 10 });
    panel.setDiff(bigDiff);
    panel.scrollDown(100);
    expect(panel["scrollOffset"]).toBeGreaterThan(0);
    panel.scrollToTop();
    expect(panel["scrollOffset"]).toBe(0);
    panel.scrollToBottom();
    expect(panel["scrollOffset"]).toBe(bigDiff.length - 10);
  });

  it("should show scroll indicators when content overflows", () => {
    const panel = new DiffPanel({ maxVisibleLines: 10 });
    const bigDiff = Array.from({ length: 50 }, (_, i) => `Line ${i + 1}`);
    panel.setDiff(bigDiff);
    const lines = panel.render(40);
    const hasScrollInfo = lines.some((l) => l.includes("/50"));
    expect(hasScrollInfo).toBe(true);
  });

  it("should handle empty diff", () => {
    const panel = new DiffPanel();
    panel.setDiff([]);
    const lines = panel.render(80);
    expect(lines.length).toBe(0);
  });

  it("should truncate long lines", () => {
    const panel = new DiffPanel();
    panel.setDiff(["A".repeat(200)]);
    const lines = panel.render(40);
    expect(lines[0]?.length).toBeLessThanOrEqual(40);
  });

  it("invalidate should not throw", () => {
    const panel = new DiffPanel();
    expect(() => panel.invalidate()).not.toThrow();
  });
});
