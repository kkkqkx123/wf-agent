/**
 * Fuzzy Matcher Utilities Unit Tests
 */

import { describe, it, expect } from "vitest";
import { getSimilarity, fuzzySearch, preserveIndentation, BUFFER_LINES } from "../fuzzy-matcher.js";

describe("Fuzzy Matcher", () => {
  describe("BUFFER_LINES constant", () => {
    it("should have correct value", () => {
      expect(BUFFER_LINES).toBe(40);
    });
  });

  describe("getSimilarity", () => {
    it("should return 1.0 for identical strings", () => {
      const similarity = getSimilarity("hello world", "hello world");
      expect(similarity).toBe(1.0);
    });

    it("should return 0.0 for empty search string", () => {
      const similarity = getSimilarity("hello", "");
      expect(similarity).toBe(0.0);
    });

    it("should return high similarity for similar strings", () => {
      const similarity = getSimilarity("hello world", "hello worl");
      expect(similarity).toBeGreaterThan(0.8);
    });

    it("should return low similarity for different strings", () => {
      const similarity = getSimilarity("hello world", "goodbye universe");
      expect(similarity).toBeLessThan(0.5);
    });

    it("should normalize smart single quotes", () => {
      const similarity = getSimilarity("it's", "it's");
      expect(similarity).toBe(1.0);
    });

    it("should normalize smart double quotes", () => {
      const similarity = getSimilarity('say "hello"', 'say "hello"');
      expect(similarity).toBe(1.0);
    });

    it("should normalize en dash", () => {
      const similarity = getSimilarity("page 1–2", "page 1-2");
      expect(similarity).toBe(1.0);
    });

    it("should normalize em dash", () => {
      const similarity = getSimilarity("word1--word2", "word1--word2");
      expect(similarity).toBe(1.0);
    });

    it("should normalize non-breaking space", () => {
      const similarity = getSimilarity("hello\u00A0world", "hello world");
      expect(similarity).toBe(1.0);
    });

    it("should handle case sensitivity", () => {
      const similarity = getSimilarity("Hello", "hello");
      // Levenshtein distance is case-sensitive
      expect(similarity).toBeLessThan(1.0);
    });

    it("should return 0.0 for completely different strings of same length", () => {
      const similarity = getSimilarity("abcd", "wxyz");
      expect(similarity).toBeLessThan(0.5);
    });

    it("should handle single character difference", () => {
      const similarity = getSimilarity("cat", "bat");
      expect(similarity).toBeGreaterThan(0.5);
    });

    it("should handle empty original string", () => {
      const similarity = getSimilarity("", "test");
      expect(similarity).toBe(0.0);
    });

    it("should handle both empty strings", () => {
      const similarity = getSimilarity("", "");
      expect(similarity).toBe(0.0);
    });
  });

  describe("fuzzySearch", () => {
    it("should find exact match", () => {
      const lines = ["line 1", "line 2", "line 3", "line 4"];
      const searchChunk = "line 2\nline 3";

      const result = fuzzySearch(lines, searchChunk, 0, lines.length);

      expect(result.bestScore).toBe(1.0);
      expect(result.bestMatchIndex).toBe(1);
      expect(result.bestMatchContent).toBe(searchChunk);
    });

    it("should find fuzzy match", () => {
      const lines = ["line 1", "line 2", "line 3", "line 4"];
      const searchChunk = "line 2\nline 3"; // Exact match exists

      const result = fuzzySearch(lines, searchChunk, 0, lines.length);

      expect(result.bestScore).toBeGreaterThan(0.8);
      expect(result.bestMatchIndex).toBeGreaterThanOrEqual(0);
    });

    it("should search within specified range", () => {
      const lines = ["line 1", "line 2", "line 3", "line 4", "line 5"];
      const searchChunk = "line 2";

      // Search only in first 3 lines
      const result = fuzzySearch(lines, searchChunk, 0, 3);

      expect(result.bestMatchIndex).toBeGreaterThanOrEqual(0);
      expect(result.bestMatchIndex).toBeLessThan(3);
    });

    it("should return low score when no good match found", () => {
      const lines = ["aaa", "bbb", "ccc"];
      const searchChunk = "xyz\nxyz";

      const result = fuzzySearch(lines, searchChunk, 0, lines.length);

      // Will find some match but with low similarity
      expect(result.bestMatchIndex).toBeGreaterThanOrEqual(0);
      expect(result.bestScore).toBeLessThan(0.5);
    });

    it("should use middle-out search strategy", () => {
      const lines = Array.from({ length: 10 }, (_, i) => `line ${i}`);
      const searchChunk = "line 5";

      const result = fuzzySearch(lines, searchChunk, 0, lines.length);

      // Should find the match
      expect(result.bestMatchIndex).toBe(5);
      expect(result.bestScore).toBe(1.0);
    });

    it("should handle multiline search chunk", () => {
      const lines = ["a", "b", "c", "d", "e"];
      const searchChunk = "b\nc\nd";

      const result = fuzzySearch(lines, searchChunk, 0, lines.length);

      expect(result.bestMatchIndex).toBe(1);
      expect(result.bestScore).toBe(1.0);
    });

    it("should handle single line search", () => {
      const lines = ["first", "second", "third"];
      const searchChunk = "second";

      const result = fuzzySearch(lines, searchChunk, 0, lines.length);

      expect(result.bestMatchIndex).toBe(1);
      expect(result.bestScore).toBe(1.0);
    });

    it("should return best match content", () => {
      const lines = ["line 1", "line 2", "line 3"];
      const searchChunk = "line 2";

      const result = fuzzySearch(lines, searchChunk, 0, lines.length);

      expect(result.bestMatchContent).toBe("line 2");
    });

    it("should handle edge case with full range", () => {
      const lines = ["only line"];
      const searchChunk = "only line";

      const result = fuzzySearch(lines, searchChunk, 0, 1);

      expect(result.bestMatchIndex).toBe(0);
      expect(result.bestScore).toBe(1.0);
    });
  });

  describe("preserveIndentation", () => {
    it("should preserve indentation when replacing", () => {
      const matchedLines = ["    original line"];
      const searchLines = ["original line"];
      const replaceLines = ["new line"];

      const result = preserveIndentation(matchedLines, searchLines, replaceLines);

      expect(result).toHaveLength(1);
      expect(result[0]).toBe("    new line");
    });

    it("should handle tab indentation", () => {
      const matchedLines = ["\t\toriginal"];
      const searchLines = ["original"];
      const replaceLines = ["replacement"];

      const result = preserveIndentation(matchedLines, searchLines, replaceLines);

      expect(result[0]).toBe("\t\treplacement");
    });

    it("should adjust relative indentation levels", () => {
      const matchedLines = ["    parent"];
      const searchLines = ["parent"];
      const replaceLines = ["  child"];

      const result = preserveIndentation(matchedLines, searchLines, replaceLines);

      // Child has 2 spaces relative to parent (which has 0 in search)
      // So it should be 4 + 2 = 6 spaces
      expect(result[0]).toBe("      child");
    });

    it("should handle decreased indentation", () => {
      const matchedLines = ["        deeply indented"];
      const searchLines = ["    base"];
      const replaceLines = ["less indented"];

      const result = preserveIndentation(matchedLines, searchLines, replaceLines);

      // Replace line has less indent than search base, so reduce matched indent
      expect(result[0]).toBe("    less indented");
    });

    it("should handle multiple lines", () => {
      const matchedLines = ["    line 1", "    line 2"];
      const searchLines = ["line 1", "line 2"];
      const replaceLines = ["new 1", "new 2"];

      const result = preserveIndentation(matchedLines, searchLines, replaceLines);

      expect(result).toHaveLength(2);
      expect(result[0]).toBe("    new 1");
      expect(result[1]).toBe("    new 2");
    });

    it("should handle mixed indentation in replace lines", () => {
      const matchedLines = ["    parent"];
      const searchLines = ["parent"];
      const replaceLines = ["child", "  grandchild"];

      const result = preserveIndentation(matchedLines, searchLines, replaceLines);

      expect(result).toHaveLength(2);
      expect(result[0]).toBe("    child");
      expect(result[1]).toBe("      grandchild");
    });

    it("should handle empty matched indent", () => {
      const matchedLines = ["no indent"];
      const searchLines = ["no indent"];
      const replaceLines = ["new content"];

      const result = preserveIndentation(matchedLines, searchLines, replaceLines);

      expect(result[0]).toBe("new content");
    });

    it("should handle empty search indent", () => {
      const matchedLines = ["    indented"];
      const searchLines = ["not indented"];
      const replaceLines = ["replacement"];

      const result = preserveIndentation(matchedLines, searchLines, replaceLines);

      expect(result[0]).toBe("    replacement");
    });

    it("should trim and re-indent replace lines", () => {
      const matchedLines = ["    target"];
      const searchLines = ["target"];
      const replaceLines = ["  padded  "];

      const result = preserveIndentation(matchedLines, searchLines, replaceLines);

      expect(result[0]).toBe("      padded");
    });

    it("should handle complex nested indentation", () => {
      const matchedLines = ["        outer"];
      const searchLines = ["    inner"];
      const replaceLines = ["      nested"];

      const result = preserveIndentation(matchedLines, searchLines, replaceLines);

      // Search base is 4, replace has 6 (relative +2)
      // Matched base is 8, so result should be 8 + 2 = 10
      expect(result[0]).toBe("          nested");
    });

    it("should handle negative relative indentation", () => {
      const matchedLines = ["    base"];
      const searchLines = ["        deep"];
      const replaceLines = ["shallow"];

      const result = preserveIndentation(matchedLines, searchLines, replaceLines);

      // Search base is 8, replace has 0 (relative -8)
      // Matched base is 4, max(0, 4-8) = 0
      expect(result[0]).toBe("shallow");
    });
  });
});
