/**
 * Tests for the universal sequence matching utilities
 */

import { describe, it, expect } from "vitest";
import {
  normalizeUnicode,
  exactMatch,
  trimEndMatch,
  trimMatch,
  normalizedMatch,
  seekSequence,
} from "../matcher.js";

describe("Universal Matcher", () => {
  describe("normalizeUnicode", () => {
    it("should convert fancy quotes to ASCII", () => {
      expect(normalizeUnicode("\u201chello\u201d")).toBe('"hello"');
      expect(normalizeUnicode("\u2018world\u2019")).toBe("'world'");
    });

    it("should convert fancy dashes to ASCII", () => {
      expect(normalizeUnicode("item\u2013one")).toBe("item-one");
      expect(normalizeUnicode("range: 1\u201410")).toBe("range: 1-10");
    });

    it("should convert non-breaking spaces", () => {
      expect(normalizeUnicode("hello\u00A0world")).toBe("hello world");
    });

    it("should trim whitespace", () => {
      expect(normalizeUnicode("  hello  ")).toBe("hello");
    });

    it("should leave ASCII unchanged", () => {
      expect(normalizeUnicode('console.log("test")')).toBe('console.log("test")');
    });
  });

  describe("exactMatch", () => {
    it("should match identical lines", () => {
      const lines = ["a", "b", "c"];
      const pattern = ["b", "c"];
      expect(exactMatch(lines, pattern, 1)).toBe(true);
    });

    it("should not match different lines", () => {
      const lines = ["a", "b", "c"];
      const pattern = ["b", "d"];
      expect(exactMatch(lines, pattern, 1)).toBe(false);
    });

    it("should handle empty pattern", () => {
      const lines = ["a", "b", "c"];
      const pattern: string[] = [];
      expect(exactMatch(lines, pattern, 0)).toBe(true);
    });
  });

  describe("trimEndMatch", () => {
    it("should match ignoring trailing whitespace", () => {
      const lines = ["a  ", "b   "];
      const pattern = ["a", "b"];
      expect(trimEndMatch(lines, pattern, 0)).toBe(true);
    });

    it("should not match if content differs", () => {
      const lines = ["a  ", "c   "];
      const pattern = ["a", "b"];
      expect(trimEndMatch(lines, pattern, 0)).toBe(false);
    });
  });

  describe("trimMatch", () => {
    it("should match ignoring leading and trailing whitespace", () => {
      const lines = ["  a  ", "  b  "];
      const pattern = ["a", "b"];
      expect(trimMatch(lines, pattern, 0)).toBe(true);
    });

    it("should not match if trimmed content differs", () => {
      const lines = ["  a  ", "  c  "];
      const pattern = ["a", "b"];
      expect(trimMatch(lines, pattern, 0)).toBe(false);
    });
  });

  describe("normalizedMatch", () => {
    it("should match with Unicode normalization", () => {
      const lines = ["console.log(\u201chello\u201d)"];
      const pattern = ['console.log("hello")'];
      expect(normalizedMatch(lines, pattern, 0)).toBe(true);
    });

    it("should match with fancy dashes", () => {
      const lines = ["item\u2013one"];
      const pattern = ["item-one"];
      expect(normalizedMatch(lines, pattern, 0)).toBe(true);
    });

    it("should not match if normalization doesn't help", () => {
      const lines = ["hello"];
      const pattern = ["world"];
      expect(normalizedMatch(lines, pattern, 0)).toBe(false);
    });
  });

  describe("seekSequence", () => {
    describe("exact matching", () => {
      it("should find exact match at start", () => {
        const lines = ["a", "b", "c"];
        const pattern = ["a", "b"];
        expect(seekSequence(lines, pattern, 0)).toBe(0);
      });

      it("should find exact match in middle", () => {
        const lines = ["a", "b", "c", "d"];
        const pattern = ["b", "c"];
        expect(seekSequence(lines, pattern, 0)).toBe(1);
      });

      it("should find exact match at end", () => {
        const lines = ["a", "b", "c"];
        const pattern = ["b", "c"];
        expect(seekSequence(lines, pattern, 0)).toBe(1);
      });

      it("should return null if not found", () => {
        const lines = ["a", "b", "c"];
        const pattern = ["x", "y"];
        expect(seekSequence(lines, pattern, 0)).toBeNull();
      });
    });

    describe("fuzzy matching passes", () => {
      it("should match with trailing whitespace (pass 2)", () => {
        const lines = ["a  ", "b  ", "c"];
        const pattern = ["a", "b"];
        expect(seekSequence(lines, pattern, 0)).toBe(0);
      });

      it("should match with leading/trailing whitespace (pass 3)", () => {
        const lines = ["  a  ", "  b  ", "c"];
        const pattern = ["a", "b"];
        expect(seekSequence(lines, pattern, 0)).toBe(0);
      });

      it("should match with Unicode differences (pass 4)", () => {
        const lines = ["console.log(\u201chello\u201d)", "next"];
        const pattern = ['console.log("hello")'];
        expect(seekSequence(lines, pattern, 0)).toBe(0);
      });
    });

    describe("eof mode", () => {
      it("should search from end when eof is true", () => {
        const lines = ["a", "b", "a", "b"];
        const pattern = ["a", "b"];
        // With eof=true, should find the last occurrence at index 2
        const result = seekSequence(lines, pattern, 0, { eof: true });
        expect(result).toBe(2);
      });

      it("should find match at end when eof is true", () => {
        const lines = ["x", "y", "a", "b"];
        const pattern = ["a", "b"];
        const result = seekSequence(lines, pattern, 0, { eof: true });
        expect(result).toBe(2);
      });

      it("should return null if pattern not at EOF when eof is true", () => {
        const lines = ["a", "b", "x", "y"];
        const pattern = ["a", "b"];
        // Pattern exists but not at EOF, with eof=true should not find it
        const result = seekSequence(lines, pattern, 0, { eof: true });
        expect(result).toBeNull();
      });
    });

    describe("edge cases", () => {
      it("should handle empty pattern", () => {
        const lines = ["a", "b", "c"];
        const pattern: string[] = [];
        expect(seekSequence(lines, pattern, 0)).toBe(0);
      });

      it("should handle pattern longer than lines", () => {
        const lines = ["a", "b"];
        const pattern = ["a", "b", "c", "d"];
        expect(seekSequence(lines, pattern, 0)).toBeNull();
      });

      it("should handle single line pattern", () => {
        const lines = ["a", "b", "c"];
        const pattern = ["b"];
        expect(seekSequence(lines, pattern, 0)).toBe(1);
      });

      it("should respect start index", () => {
        const lines = ["a", "b", "c", "b", "d"];
        const pattern = ["b"];
        // Start searching from index 2, should find second 'b'
        expect(seekSequence(lines, pattern, 2)).toBe(3);
      });
    });

    describe("multi-pass matching priority", () => {
      it("should prefer exact match over fuzzy match", () => {
        const lines = ["a", "b  ", "c"];
        const pattern = ["b"];
        // Should match exactly at index 1, not use fuzzy
        const result = seekSequence(lines, pattern, 0);
        expect(result).toBe(1);
      });

      it("should use trim-end match if no exact match", () => {
        const lines = ["a  ", "b  "];
        const pattern = ["a", "b"];
        const result = seekSequence(lines, pattern, 0);
        expect(result).toBe(0);
      });

      it("should use trim match if no trim-end match", () => {
        const lines = ["  a  ", "  b  "];
        const pattern = ["a", "b"];
        const result = seekSequence(lines, pattern, 0);
        expect(result).toBe(0);
      });

      it("should use unicode normalization as last resort", () => {
        const lines = ["console.log(\u201chello\u201d)"];
        const pattern = ['console.log("hello")'];
        const result = seekSequence(lines, pattern, 0);
        expect(result).toBe(0);
      });
    });

    describe("complex scenarios", () => {
      it("should handle multi-line patterns", () => {
        const lines = ["function test() {", "  return 1;", "}", "other code"];
        const pattern = ["function test() {", "  return 1;", "}"];
        expect(seekSequence(lines, pattern, 0)).toBe(0);
      });

      it("should find pattern in middle of file", () => {
        const lines = ["line 1", "line 2", "target", "line 4"];
        const pattern = ["target"];
        expect(seekSequence(lines, pattern, 0)).toBe(2);
      });

      it("should handle overlapping potential matches", () => {
        const lines = ["a", "a", "b"];
        const pattern = ["a", "b"];
        // Should find first valid match
        expect(seekSequence(lines, pattern, 0)).toBe(1);
      });
    });
  });
});
