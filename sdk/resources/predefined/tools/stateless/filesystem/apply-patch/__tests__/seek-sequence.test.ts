/**
 * Tests for the sequence matching algorithm
 */

import { describe, it, expect } from "vitest";
import { seekSequence } from "../utils/matcher.js";

describe("seekSequence", () => {
  describe("exact matching", () => {
    it("should find exact match at start", () => {
      const lines = ["a", "b", "c", "d"];
      const pattern = ["a", "b"];
      const result = seekSequence(lines, pattern, 0, false);
      expect(result).toBe(0);
    });

    it("should find exact match in middle", () => {
      const lines = ["a", "b", "c", "d"];
      const pattern = ["b", "c"];
      const result = seekSequence(lines, pattern, 0, false);
      expect(result).toBe(1);
    });

    it("should find exact match at end", () => {
      const lines = ["a", "b", "c", "d"];
      const pattern = ["c", "d"];
      const result = seekSequence(lines, pattern, 0, false);
      expect(result).toBe(2);
    });

    it("should return null if not found", () => {
      const lines = ["a", "b", "c", "d"];
      const pattern = ["x", "y"];
      const result = seekSequence(lines, pattern, 0, false);
      expect(result).toBeNull();
    });
  });

  describe("trim-end matching", () => {
    it("should match ignoring trailing whitespace", () => {
      const lines = ["a  ", "b\t", "c", "d"];
      const pattern = ["a", "b"];
      const result = seekSequence(lines, pattern, 0, false);
      expect(result).toBe(0);
    });
  });

  describe("trim matching", () => {
    it("should match ignoring leading and trailing whitespace", () => {
      const lines = ["  a  ", "\tb\t", "c", "d"];
      const pattern = ["a", "b"];
      const result = seekSequence(lines, pattern, 0, false);
      expect(result).toBe(0);
    });
  });

  describe("unicode normalization", () => {
    it("should match fancy quotes", () => {
      const lines = ["'hello'", '"world"'];
      const pattern = ["'hello'", '"world"'];
      const result = seekSequence(lines, pattern, 0, false);
      expect(result).toBe(0);
    });

    it("should match fancy dashes", () => {
      const lines = ["hello—world"];
      const pattern = ["hello-world"];
      const result = seekSequence(lines, pattern, 0, false);
      expect(result).toBe(0);
    });

    it("should match non-breaking spaces", () => {
      const lines = ["hello\u00A0world"];
      const pattern = ["hello world"];
      const result = seekSequence(lines, pattern, 0, false);
      expect(result).toBe(0);
    });
  });

  describe("eof mode", () => {
    it("should search from end when eof is true", () => {
      const lines = ["a", "b", "a", "b"];
      const pattern = ["a", "b"];
      const result = seekSequence(lines, pattern, 0, true);
      expect(result).toBe(2); // Should find the last occurrence
    });

    it("should find match at end when eof is true", () => {
      const lines = ["x", "y", "a", "b"];
      const pattern = ["a", "b"];
      const result = seekSequence(lines, pattern, 0, true);
      expect(result).toBe(2); // Should find at end
    });
  });

  describe("edge cases", () => {
    it("should handle empty pattern", () => {
      const lines = ["a", "b", "c"];
      const pattern: string[] = [];
      const result = seekSequence(lines, pattern, 0, false);
      expect(result).toBe(0);
    });

    it("should handle pattern longer than lines", () => {
      const lines = ["a", "b"];
      const pattern = ["a", "b", "c", "d"];
      const result = seekSequence(lines, pattern, 0, false);
      expect(result).toBeNull();
    });

    it("should handle single line pattern", () => {
      const lines = ["a", "b", "c"];
      const pattern = ["b"];
      const result = seekSequence(lines, pattern, 0, false);
      expect(result).toBe(1);
    });

    it("should handle start index", () => {
      const lines = ["a", "b", "a", "b"];
      const pattern = ["a", "b"];
      const result = seekSequence(lines, pattern, 1, false);
      expect(result).toBe(2); // Should skip the first occurrence
    });
  });

  describe("multi-pass matching priority", () => {
    it("should prefer exact match over fuzzy match", () => {
      const lines = ["a", "b", "a ", "b"];
      const pattern = ["a", "b"];
      const result = seekSequence(lines, pattern, 0, false);
      expect(result).toBe(0); // Should find exact match first
    });

    it("should use trim-end match if no exact match", () => {
      const lines = ["a ", "b", "c"];
      const pattern = ["a", "b"];
      const result = seekSequence(lines, pattern, 0, false);
      expect(result).toBe(0);
    });

    it("should use trim match if no trim-end match", () => {
      const lines = [" a ", " b ", "c"];
      const pattern = ["a", "b"];
      const result = seekSequence(lines, pattern, 0, false);
      expect(result).toBe(0);
    });

    it("should use unicode normalization as last resort", () => {
      const lines = ["'a'", "'b'"];
      const pattern = ["'a'", "'b'"];
      const result = seekSequence(lines, pattern, 0, false);
      expect(result).toBe(0);
    });
  });
});
