/**
 * Partial JSON Parser Unit Tests
 *
 * Tests the core functions: partialParse, isValidPartialJson
 * Covers: basic JSON, partial/incomplete JSON, special chars, scientific notation,
 * bracket balancing, edge cases, and invariant checks.
 */

import { describe, it, expect } from "vitest";
import { partialParse, isValidPartialJson } from "../partial-json-parser.js";

// ---------------------------------------------------------------------------
// partialParse – complete JSON
// ---------------------------------------------------------------------------
describe("partialParse – complete JSON", () => {
  it("should parse a simple object", () => {
    const result = partialParse('{"key": "value"}');
    expect(result).toEqual({ key: "value" });
  });

  it("should parse nested objects", () => {
    const result = partialParse('{"a": {"b": "c"}}');
    expect(result).toEqual({ a: { b: "c" } });
  });

  it("should parse arrays", () => {
    const result = partialParse("[1, 2, 3]");
    expect(result).toEqual([1, 2, 3]);
  });

  it("should parse arrays of objects", () => {
    const result = partialParse('[{"x": 1}, {"x": 2}]');
    expect(result).toEqual([{ x: 1 }, { x: 2 }]);
  });

  it("should parse numbers (integer, float, negative)", () => {
    expect(partialParse('{"n": 42}')).toEqual({ n: 42 });
    expect(partialParse('{"n": 3.14}')).toEqual({ n: 3.14 });
    expect(partialParse('{"n": -7}')).toEqual({ n: -7 });
  });

  it("should parse boolean and null values", () => {
    expect(partialParse('{"a": true, "b": false, "c": null}')).toEqual({
      a: true,
      b: false,
      c: null,
    });
  });

  it("should parse empty object", () => {
    expect(partialParse("{}")).toEqual({});
  });

  it("should parse empty array", () => {
    expect(partialParse("[]")).toEqual([]);
  });
});

// ---------------------------------------------------------------------------
// partialParse – partial / incomplete JSON
// ---------------------------------------------------------------------------
describe("partialParse – partial / incomplete JSON", () => {
  it("should parse a partial object (missing closing brace)", () => {
    const result = partialParse('{"key": "value"');
    expect(result).toEqual({ key: "value" });
  });

  it("should parse a partial object (trailing comma)", () => {
    const result = partialParse('{"a": 1,');
    expect(result).toEqual({ a: 1 });
  });

  it("should parse after incomplete key name (collapsed to empty object)", () => {
    // Incomplete string key "ke is dropped (dangling), then { auto-closes to {}
    const result = partialParse('{"ke');
    expect(result).toEqual({});
  });

  it("should parse an incomplete value (collapsed to empty object)", () => {
    // "key" and ":" tokens remain, but strip removes trailing ":" then also "key"
    // (string after { is stripped), leaving only { which auto-closes to {}
    const result = partialParse('{"key": "val');
    expect(result).toEqual({});
  });

  it("should parse part of an array", () => {
    const result = partialParse("[1, 2,");
    expect(result).toEqual([1, 2]);
  });

  it("should parse nested partial", () => {
    const result = partialParse('{"outer": {"inner": 42');
    expect(result).toEqual({ outer: { inner: 42 } });
  });

  it("should return undefined for empty string", () => {
    expect(partialParse("")).toBeUndefined();
  });

  it("should return undefined for whitespace", () => {
    expect(partialParse("   ")).toBeUndefined();
  });

  it("should return undefined for garbage input", () => {
    expect(partialParse("not json at all")).toBeUndefined();
  });

  it("should handle a partial string that ends with a colon", () => {
    // The colon will be stripped as a dangling separator
    const result = partialParse('{"key":');
    expect(result).toEqual({ key: undefined });
  });

  it("should handle trailing whitespace gracefully", () => {
    const result = partialParse('{"a": 1}  ');
    expect(result).toEqual({ a: 1 });
  });
});

// ---------------------------------------------------------------------------
// partialParse – strings with special characters
// Note: The tokenizer stores escape sequences literally (matching the original
// Anthropic SDK behavior). It does NOT decode \n, \t, \", \\, \uXXXX etc.
// This is acceptable for incremental/partial parsing of streaming tool calls.
// ---------------------------------------------------------------------------
describe("partialParse – special characters in strings", () => {
  it("should handle strings containing double quotes (escaped) as round-trip", () => {
    // partialParse produces the same result as JSON.parse for this input
    const input = '{"msg": "He said \\"hello\\""}';
    const result = partialParse(input);
    const expected = JSON.parse(input);
    expect(result).toEqual(expected);
  });

  it("should handle strings containing backslashes as round-trip", () => {
    const input = '{"path": "C:\\\\Users\\\\test"}';
    const result = partialParse(input);
    const expected = JSON.parse(input);
    expect(result).toEqual(expected);
  });

  it("should handle strings with newline characters as round-trip", () => {
    const input = '{"text": "line1\\nline2"}';
    const result = partialParse(input);
    const expected = JSON.parse(input);
    expect(result).toEqual(expected);
  });

  it("should handle strings with tabs as round-trip", () => {
    const input = '{"text": "col1\\tcol2"}';
    const result = partialParse(input);
    const expected = JSON.parse(input);
    expect(result).toEqual(expected);
  });

  it("should handle partial strings with escaped chars", () => {
    const result = partialParse('{"msg": "hello \\"wor');
    // Partial string with dangling quote after escape produces empty object
    expect(result).toBeDefined();
  });

  it("should handle unicode escape sequences as round-trip", () => {
    const input = '{"emoji": "\\u0048\\u0069"}';
    const result = partialParse(input);
    const expected = JSON.parse(input);
    expect(result).toEqual(expected);
  });

  it("should handle an empty string value", () => {
    const result = partialParse('{"a": ""}');
    expect(result).toEqual({ a: "" });
  });
});

// ---------------------------------------------------------------------------
// partialParse – scientific notation (regression: number tokenization)
// ---------------------------------------------------------------------------
describe("partialParse – scientific notation", () => {
  it("should parse positive exponent", () => {
    expect(partialParse('{"v": 1.5e10}')).toEqual({ v: 1.5e10 });
  });

  it("should parse positive exponent with uppercase E", () => {
    expect(partialParse('{"v": 1E5}')).toEqual({ v: 1e5 });
  });

  it("should parse negative exponent", () => {
    expect(partialParse('{"v": 2.5e-3}')).toEqual({ v: 2.5e-3 });
  });

  it("should parse positive exponent with + sign", () => {
    expect(partialParse('{"v": 3e+2}')).toEqual({ v: 300 });
  });

  it("should parse integer with exponent", () => {
    expect(partialParse('{"v": 1e2}')).toEqual({ v: 100 });
  });
});

// ---------------------------------------------------------------------------
// partialParse – bracket balancing / nesting edge cases
// ---------------------------------------------------------------------------
describe("partialParse – bracket balancing", () => {
  it("should handle deeply nested objects", () => {
    const input = '{"a":{"b":{"c":{"d":"e"}}}}';
    expect(partialParse(input)).toEqual({ a: { b: { c: { d: "e" } } } });
  });

  it("should handle partial deeply nested (truncated)", () => {
    // The last incomplete str will be dropped during strip/unstrip
    const input = '{"a":{"b":{"c":{"d": "e"';
    const result = partialParse(input);
    expect(result).toEqual({ a: { b: { c: { d: "e" } } } });
  });

  it("should handle mixed arrays and objects", () => {
    const input = '{"items":[{"id":1},{"id":2}]}';
    expect(partialParse(input)).toEqual({ items: [{ id: 1 }, { id: 2 }] });
  });

  it("should handle partial mixed (truncated)", () => {
    const input = '{"items":[{"id":1}';
    const result = partialParse(input);
    expect(result).toEqual({ items: [{ id: 1 }] });
  });
});

// ---------------------------------------------------------------------------
// isValidPartialJson
// ---------------------------------------------------------------------------
describe("isValidPartialJson", () => {
  it("should return true for a valid partial object", () => {
    expect(isValidPartialJson('{"a": 1')).toBe(true);
  });

  it("should return true for a complete valid JSON", () => {
    expect(isValidPartialJson('{"a": 1}')).toBe(true);
  });

  it("should return false for empty string", () => {
    expect(isValidPartialJson("")).toBe(false);
  });

  it("should return false for whitespace only", () => {
    expect(isValidPartialJson("   ")).toBe(false);
  });

  it("should return false for a lone string token", () => {
    expect(isValidPartialJson('"key"')).toBe(false);
  });

  it("should return false for more closing braces than opening", () => {
    expect(isValidPartialJson('{"a": 1}}')).toBe(false);
  });

  it("should return false for more closing brackets than opening", () => {
    expect(isValidPartialJson("[1, 2]]")).toBe(false);
  });

  it("should return false for mismatched closing brace (no opener)", () => {
    expect(isValidPartialJson("}")).toBe(false);
  });

  it("should return true for balanced but partial input", () => {
    expect(isValidPartialJson('{"a": [1, 2')).toBe(true);
  });

  it("should return true for valid nested brackets", () => {
    expect(isValidPartialJson('{"a": {"b": [1, 2, 3]}}')).toBe(true);
  });

  it("should handle array of objects", () => {
    expect(isValidPartialJson('[{"a": 1}, {"b": 2}]')).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// Invariant: partialParse(valid JSON) === JSON.parse
// ---------------------------------------------------------------------------
describe("invariant: partialParse(valid JSON) === JSON.parse", () => {
  const cases = [
    '{"a":1}',
    '{"a":"hello"}',
    '{"a":true,"b":false,"c":null}',
    '{"n":-42,"f":3.14}',
    '{"arr":[1,2,3]}',
    '{"nested":{"x":{"y":"z"}}}',
    "[]",
    "{}",
    "[1,2,3]",
    '{"s":"with spaces"}',
    '{"e":"underscores_and_123"}',
  ];

  for (const json of cases) {
    it(`should match JSON.parse for: ${json.slice(0, 40)}`, () => {
      expect(partialParse(json)).toEqual(JSON.parse(json));
    });
  }
});

// ---------------------------------------------------------------------------
// Regression: the parser should not throw for any input
// ---------------------------------------------------------------------------
describe("regression: no-throw guarantee", () => {
  const trickyInputs = [
    "",
    "   ",
    "{",
    "}",
    "[",
    "]",
    ":",
    ",",
    "{:}",
    "{,}",
    '{"a"}',
    '{"a":}',
    "[,]",
    "[1,]",
    '{"a": tr',
    '{"a": n',
    '{"a": "\\',
    '{"a": "\\\\',
    '\\"',
    "nul", // incomplete null
    "tru", // incomplete true
    "fals", // incomplete false
    "\0",
    "\n",
    '{"a": -',
    '{"a": .',
    '{"a": 1.',
    '{"a": 1.2.3}',
    '{"a": "hello',
    '{"a": "hello\\"}',
    '{"a": 1e',
    '{"a": 1e-',
    '{"a": 1e+',
  ];

  for (const input of trickyInputs) {
    it(`should not throw for: ${JSON.stringify(input.slice(0, 50))}`, () => {
      expect(() => partialParse(input)).not.toThrow();
      expect(() => isValidPartialJson(input)).not.toThrow();
    });
  }
});
