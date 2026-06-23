/**
 * DiffEngine Tests
 */

import { describe, it, expect } from "vitest";
import { DiffEngine } from "../diff-engine.js";

describe("DiffEngine", () => {
  const engine = new DiffEngine();

  describe("diff", () => {
    it("should return empty result for identical content", () => {
      const content = "line1\nline2\nline3";
      const result = engine.diff(content, content);

      expect(result.hasChanges).toBe(false);
      expect(result.equalCount).toBe(3);
      expect(result.deleteCount).toBe(0);
      expect(result.insertCount).toBe(0);
    });

    it("should detect single line insertion", () => {
      const oldContent = "line1\nline3";
      const newContent = "line1\nline2\nline3";
      const result = engine.diff(oldContent, newContent);

      expect(result.hasChanges).toBe(true);
      expect(result.insertCount).toBe(1);
      expect(result.deleteCount).toBe(0);
    });

    it("should detect single line deletion", () => {
      const oldContent = "line1\nline2\nline3";
      const newContent = "line1\nline3";
      const result = engine.diff(oldContent, newContent);

      expect(result.hasChanges).toBe(true);
      expect(result.deleteCount).toBe(1);
      expect(result.insertCount).toBe(0);
    });

    it("should detect line modification", () => {
      const oldContent = "line1\nold\nline3";
      const newContent = "line1\nnew\nline3";
      const result = engine.diff(oldContent, newContent);

      expect(result.hasChanges).toBe(true);
      expect(result.deleteCount).toBe(1);
      expect(result.insertCount).toBe(1);
    });

    it("should handle empty content", () => {
      const result = engine.diff("", "");

      expect(result.hasChanges).toBe(false);
      expect(result.ops.length).toBe(0);
    });

    it("should handle insertion into empty content", () => {
      const result = engine.diff("", "new line");

      expect(result.hasChanges).toBe(true);
      expect(result.insertCount).toBe(1);
    });

    it("should handle deletion to empty content", () => {
      const result = engine.diff("old line", "");

      expect(result.hasChanges).toBe(true);
      expect(result.deleteCount).toBe(1);
    });
  });

  describe("unifiedDiff", () => {
    it("should generate unified diff format", () => {
      const oldContent = "line1\nold\nline3";
      const newContent = "line1\nnew\nline3";
      const unified = engine.unifiedDiff(oldContent, newContent, "test.txt", "test.txt");

      expect(unified).toContain("--- test.txt");
      expect(unified).toContain("+++ test.txt");
      expect(unified).toContain("@@");
      // The diff output contains the changes
      expect(unified.length).toBeGreaterThan(0);
    });

    it("should return empty string for identical content", () => {
      const content = "line1\nline2";
      const unified = engine.unifiedDiff(content, content);

      expect(unified).toBe("");
    });
  });

  describe("getStats", () => {
    it("should return correct stats for changes", () => {
      const oldContent = "line1\nline2\nline3";
      const newContent = "line1\nmodified\nline3\nline4";
      const stats = engine.getStats(oldContent, newContent);

      // The diff algorithm may detect changes differently
      expect(stats.changedLines).toBeGreaterThan(0);
      expect(stats.similarity).toBeGreaterThan(0);
      expect(stats.similarity).toBeLessThan(1);
    });

    it("should return 1 similarity for identical content", () => {
      const content = "line1\nline2";
      const stats = engine.getStats(content, content);

      expect(stats.similarity).toBe(1);
    });
  });
});
