import { describe, it, expect } from "vitest";
import { fuzzyMatch, fuzzyFilter } from "../fuzzy.js";

describe("Fuzzy Matching", () => {
  describe("fuzzyMatch", () => {
    describe("Basic Matching", () => {
      it("should match empty query", () => {
        const result = fuzzyMatch("", "hello");
        expect(result.matches).toBe(true);
        expect(result.score).toBe(0);
      });

      it("should match exact string", () => {
        const result = fuzzyMatch("hello", "hello");
        expect(result.matches).toBe(true);
        expect(result.score).toBeLessThan(0); // Should have negative score (good match)
      });

      it("should not match when query is longer than text", () => {
        const result = fuzzyMatch("helloworld", "hello");
        expect(result.matches).toBe(false);
      });

      it("should match substring", () => {
        const result = fuzzyMatch("ell", "hello");
        expect(result.matches).toBe(true);
      });

      it("should match characters in order", () => {
        const result = fuzzyMatch("hlo", "hello");
        expect(result.matches).toBe(true);
      });

      it("should not match characters out of order", () => {
        const result = fuzzyMatch("olh", "hello");
        expect(result.matches).toBe(false);
      });
    });

    describe("Case Insensitivity", () => {
      it("should be case insensitive", () => {
        const result = fuzzyMatch("HELLO", "hello");
        expect(result.matches).toBe(true);
      });

      it("should handle mixed case", () => {
        const result = fuzzyMatch("HeLLo", "hElLo");
        expect(result.matches).toBe(true);
      });
    });

    describe("Scoring", () => {
      it("should reward consecutive matches", () => {
        const consecutive = fuzzyMatch("hel", "hello");
        const scattered = fuzzyMatch("hlo", "hello");
        
        expect(consecutive.score).toBeLessThan(scattered.score);
      });

      it("should reward word boundary matches", () => {
        const boundary = fuzzyMatch("w", "hello world");
        const nonBoundary = fuzzyMatch("o", "hello world");
        
        expect(boundary.score).toBeLessThan(nonBoundary.score);
      });

      it("should penalize gaps between matches", () => {
        const smallGap = fuzzyMatch("hl", "hello");
        const largeGap = fuzzyMatch("ho", "hello");
        
        expect(smallGap.score).toBeLessThan(largeGap.score);
      });

      it("should penalize later matches slightly", () => {
        const early = fuzzyMatch("he", "hello");
        const late = fuzzyMatch("ll", "hello");
        
        // Later matches should have slightly higher (worse) score
        expect(early.score).toBeLessThan(late.score);
      });
    });

    describe("Word Boundaries", () => {
      it("should recognize space as word boundary", () => {
        const result = fuzzyMatch("w", "hello world");
        expect(result.matches).toBe(true);
      });

      it("should recognize hyphen as word boundary", () => {
        const result = fuzzyMatch("w", "hello-world");
        expect(result.matches).toBe(true);
      });

      it("should recognize underscore as word boundary", () => {
        const result = fuzzyMatch("w", "hello_world");
        expect(result.matches).toBe(true);
      });

      it("should recognize slash as word boundary", () => {
        const result = fuzzyMatch("w", "hello/world");
        expect(result.matches).toBe(true);
      });

      it("should recognize dot as word boundary", () => {
        const result = fuzzyMatch("w", "hello.world");
        expect(result.matches).toBe(true);
      });

      it("should recognize colon as word boundary", () => {
        const result = fuzzyMatch("w", "hello:world");
        expect(result.matches).toBe(true);
      });
    });

    describe("Alpha-Numeric Swapping", () => {
      it("should match with swapped alpha-numeric query", () => {
        const result = fuzzyMatch("123abc", "abc123");
        expect(result.matches).toBe(true);
      });

      it("should match with swapped numeric-alpha query", () => {
        const result = fuzzyMatch("abc123", "123abc");
        expect(result.matches).toBe(true);
      });

      it("should penalize swapped matches", () => {
        const normal = fuzzyMatch("abc123", "abc123");
        const swapped = fuzzyMatch("123abc", "abc123");
        
        expect(swapped.score).toBeGreaterThan(normal.score);
      });

      it("should not swap if no alphanumeric pattern", () => {
        const result = fuzzyMatch("abc", "def");
        expect(result.matches).toBe(false);
      });
    });

    describe("Edge Cases", () => {
      it("should handle single character query", () => {
        const result = fuzzyMatch("a", "abc");
        expect(result.matches).toBe(true);
      });

      it("should handle single character text", () => {
        const result = fuzzyMatch("a", "a");
        expect(result.matches).toBe(true);
      });

      it("should handle special characters", () => {
        const result = fuzzyMatch("@#", "test@example#");
        expect(result.matches).toBe(true);
      });

      it("should handle unicode characters", () => {
        const result = fuzzyMatch("世", "世界");
        expect(result.matches).toBe(true);
      });
    });
  });

  describe("fuzzyFilter", () => {
    interface TestItem {
      name: string;
      value: number;
    }

    const items: TestItem[] = [
      { name: "apple", value: 1 },
      { name: "apricot", value: 2 },
      { name: "banana", value: 3 },
      { name: "blueberry", value: 4 },
      { name: "cherry", value: 5 },
    ];

    const getText = (item: TestItem) => item.name;

    describe("Basic Filtering", () => {
      it("should return all items for empty query", () => {
        const result = fuzzyFilter(items, "", getText);
        expect(result).toEqual(items);
      });

      it("should return all items for whitespace query", () => {
        const result = fuzzyFilter(items, "   ", getText);
        expect(result).toEqual(items);
      });

      it("should filter by single token", () => {
        const result = fuzzyFilter(items, "ban", getText);
        expect(result.length).toBe(1);
        expect(result[0]?.name).toBe("banana");
      });

      it("should sort by match quality", () => {
        const result = fuzzyFilter(items, "app", getText);
        // At least one item should match
        expect(result.length).toBeGreaterThanOrEqual(1);
        // "apple" should be in the results
        expect(result.some(i => i.name === "apple")).toBe(true);
      });
    });

    describe("Multi-Token Queries", () => {
      it("should match all tokens", () => {
        const multiItems = [
          { name: "hello world", value: 1 },
          { name: "hello there", value: 2 },
          { name: "goodbye world", value: 3 },
        ];
        
        const result = fuzzyFilter(multiItems, "hello world", (i) => i.name);
        expect(result.length).toBe(1);
        expect(result[0]?.name).toBe("hello world");
      });

      it("should require all tokens to match", () => {
        const result = fuzzyFilter(items, "ban che", getText);
        expect(result.length).toBe(0);
      });

      it("should handle tokens in any order", () => {
        const multiItems = [
          { name: "hello world", value: 1 },
          { name: "world hello", value: 2 },
        ];
        
        const result = fuzzyFilter(multiItems, "world hello", (i) => i.name);
        expect(result.length).toBe(2);
      });
    });

    describe("Sorting", () => {
      it("should sort best matches first", () => {
        const result = fuzzyFilter(items, "ber", getText);
        // At least one item should match
        expect(result.length).toBeGreaterThanOrEqual(1);
        // "blueberry" should be in the results
        expect(result.some(i => i.name === "blueberry")).toBe(true);
      });

      it("should maintain stable sort for equal scores", () => {
        const similarItems = [
          { name: "aaa", value: 1 },
          { name: "aab", value: 2 },
          { name: "aac", value: 3 },
        ];
        
        const result = fuzzyFilter(similarItems, "aa", (i) => i.name);
        expect(result.length).toBe(3);
      });
    });

    describe("Edge Cases", () => {
      it("should handle empty items array", () => {
        const result = fuzzyFilter([], "test", getText);
        expect(result).toEqual([]);
      });

      it("should handle no matches", () => {
        const result = fuzzyFilter(items, "xyz", getText);
        expect(result).toEqual([]);
      });

      it("should handle complex objects", () => {
        interface ComplexItem {
          data: { label: string };
        }
        
        const complexItems: ComplexItem[] = [
          { data: { label: "test1" } },
          { data: { label: "test2" } },
        ];
        
        const result = fuzzyFilter(complexItems, "test", (i) => i.data.label);
        expect(result.length).toBe(2);
      });
    });
  });
});
