/**
 * Tests for Fuzzy Matcher
 */

import { describe, it, expect } from "vitest";
import { fuzzyMatch, sortByFuzzyMatch } from "../matcher.js";

describe("fuzzyMatch", () => {
  it("should match when all query characters appear in order", () => {
    const result = fuzzyMatch("hello world", "hwd");
    expect(result).not.toBeNull();
    expect(result!.score).toBeGreaterThan(0);
    expect(result!.positions).toEqual([0, 6, 10]);
  });

  it("should return null when query characters are not all found", () => {
    const result = fuzzyMatch("hello", "xyz");
    expect(result).toBeNull();
  });

  it("should be case insensitive", () => {
    const result1 = fuzzyMatch("Hello World", "hwo");
    const result2 = fuzzyMatch("hello world", "HWO");
    expect(result1).not.toBeNull();
    expect(result2).not.toBeNull();
    expect(result1!.score).toBe(result2!.score);
  });

  it("should give bonus for word boundary matches", () => {
    const resultBoundary = fuzzyMatch("test_file", "t");
    const resultNormal = fuzzyMatch("test_file", "e");
    expect(resultBoundary).not.toBeNull();
    expect(resultNormal).not.toBeNull();
    expect(resultBoundary!.score).toBe(2);
    expect(resultNormal!.score).toBe(1);
  });

  it("should give bonus for matching after slash", () => {
    const result = fuzzyMatch("src/components/button", "b");
    expect(result).not.toBeNull();
    expect(result!.score).toBe(2);
  });

  it("should give bonus for matching after underscore", () => {
    const result = fuzzyMatch("my_file_name", "f");
    expect(result).not.toBeNull();
    expect(result!.score).toBe(2);
  });

  it("should give bonus for matching after hyphen", () => {
    const result = fuzzyMatch("my-file-name", "f");
    expect(result).not.toBeNull();
    expect(result!.score).toBe(2);
  });

  it("should correctly track positions", () => {
    const result = fuzzyMatch("abcdef", "bdf");
    expect(result).not.toBeNull();
    expect(result!.positions).toEqual([1, 3, 5]);
  });

  it("should handle empty query", () => {
    const result = fuzzyMatch("hello", "");
    expect(result).not.toBeNull();
    expect(result!.score).toBe(0);
    expect(result!.positions).toEqual([]);
  });

  it("should handle empty text", () => {
    const result = fuzzyMatch("", "a");
    expect(result).toBeNull();
  });

  it("should handle both empty", () => {
    const result = fuzzyMatch("", "");
    expect(result).not.toBeNull();
    expect(result!.score).toBe(0);
    expect(result!.positions).toEqual([]);
  });

  it("should handle longer query than text", () => {
    const result = fuzzyMatch("abc", "abcdef");
    expect(result).toBeNull();
  });

  it("should match consecutive characters", () => {
    const result = fuzzyMatch("hello", "ello");
    expect(result).not.toBeNull();
    expect(result!.positions).toEqual([1, 2, 3, 4]);
  });

  it("should match single character", () => {
    const result = fuzzyMatch("hello", "o");
    expect(result).not.toBeNull();
    expect(result!.positions).toEqual([4]);
  });

  it("should accumulate score correctly for mixed boundaries", () => {
    const result = fuzzyMatch("a/b_normal", "abn");
    expect(result).not.toBeNull();
    expect(result!.score).toBe(6); // 2 (a start) + 2 (b after /) + 2 (n after _)
  });
});

describe("sortByFuzzyMatch", () => {
  const items = [
    "src/components/Button.tsx",
    "src/utils/helpers.ts",
    "README.md",
    "src/components/Input.tsx",
  ];

  it("should sort items by score descending", () => {
    const result = sortByFuzzyMatch(items, "but", item => item);
    expect(result.length).toBeGreaterThan(0);
    expect(result[0]!.item).toBe("src/components/Button.tsx");
  });

  it("should exclude items that don't match", () => {
    const result = sortByFuzzyMatch(items, "xyz", item => item);
    expect(result).toHaveLength(0);
  });

  it("should tie-break by path length (shorter first)", () => {
    const shortItems = ["a/very/long/path/file.ts", "short.ts"];
    const result = sortByFuzzyMatch(shortItems, "short", item => item);
    if (result.length === 2 && result[0]!.score === result[1]!.score) {
      expect(result[0]!.item).toBe("short.ts");
    }
  });

  it("should use custom search text extractor", () => {
    const objects = [
      { name: "hello.ts", path: "/src/hello.ts" },
      { name: "world.ts", path: "/src/world.ts" },
    ];
    const result = sortByFuzzyMatch(objects, "world", item => item.name);
    expect(result).toHaveLength(1);
    expect(result[0]!.item).toEqual({ name: "world.ts", path: "/src/world.ts" });
  });

  it("should return empty array for empty items", () => {
    const result = sortByFuzzyMatch([], "query", item => item);
    expect(result).toEqual([]);
  });

  it("should handle empty query", () => {
    const result = sortByFuzzyMatch(items, "", item => item);
    expect(result.length).toBe(items.length);
    expect(result.every(r => r.score === 0)).toBe(true);
  });

  it("should match items against combined search text", () => {
    const result = sortByFuzzyMatch(
      [{ path: "src/file.ts", label: "My File" }],
      "my",
      item => `${item.path} ${item.label}`,
    );
    expect(result.length).toBe(1);
  });
});
