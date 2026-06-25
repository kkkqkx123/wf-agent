/**
 * BaseDiffCalculator Tests
 * Tests for generic deep comparison, delta calculation, and delta application
 */

import { describe, it, expect, beforeEach } from "vitest";
import { BaseDiffCalculator } from "../base-diff-calculator.js";

describe("BaseDiffCalculator", () => {
  let calculator: BaseDiffCalculator;

  beforeEach(() => {
    calculator = new BaseDiffCalculator();
  });

  describe("calculateDelta", () => {
    it("should return empty delta for identical objects", () => {
      const state = { a: 1, b: "hello", c: true };
      const delta = calculator.calculateDelta(state, { ...state });
      expect(delta).toEqual({});
    });

    it("should detect changed field values", () => {
      const previous = { name: "foo", value: 42 };
      const current = { name: "bar", value: 42 };
      const delta = calculator.calculateDelta(previous, current);
      expect(delta).toHaveProperty("name");
      expect(delta["name"]).toEqual({ from: "foo", to: "bar" });
      expect(delta).not.toHaveProperty("value");
    });

    it("should detect deleted keys", () => {
      const previous = { a: 1, b: 2, c: 3 };
      const current = { a: 1, c: 3 };
      const delta = calculator.calculateDelta(previous, current);
      expect(delta).toHaveProperty("b");
      expect(delta["b"]).toEqual({ from: 2, to: undefined });
    });

    it("should detect newly added keys", () => {
      const previous = { a: 1 };
      const current = { a: 1, b: 2 };
      const delta = calculator.calculateDelta(previous, current);
      expect(delta).toHaveProperty("b");
      expect(delta["b"]).toEqual({ from: undefined, to: 2 });
    });

    it("should handle nested object changes", () => {
      const previous = { nested: { x: 1, y: 2 } };
      const current = { nested: { x: 99, y: 2 } };
      const delta = calculator.calculateDelta(previous, current);
      expect(delta).toHaveProperty("nested");
      expect(delta["nested"]!.from).toEqual({ x: 1, y: 2 });
      expect(delta["nested"]!.to).toEqual({ x: 99, y: 2 });
    });

    it("should handle array changes", () => {
      const previous = { items: [1, 2, 3] };
      const current = { items: [1, 2, 4] };
      const delta = calculator.calculateDelta(previous, current);
      expect(delta).toHaveProperty("items");
    });

    it("should detect type changes", () => {
      const previous = { value: "42" };
      const current = { value: 42 };
      const delta = calculator.calculateDelta(previous, current as unknown as { value: string });
      expect(delta).toHaveProperty("value");
    });

    it("should detect null vs undefined changes", () => {
      const previous = { a: null };
      const current = { a: undefined };
      const delta = calculator.calculateDelta(previous, current as unknown as { a: null });
      expect(delta).toHaveProperty("a");
    });

    it("should handle empty objects", () => {
      const delta = calculator.calculateDelta({}, {});
      expect(delta).toEqual({});
    });

    it("should handle deeply nested objects", () => {
      const previous = { level1: { level2: { value: 1 } } };
      const current = { level1: { level2: { value: 2 } } };
      const delta = calculator.calculateDelta(previous, current);
      expect(delta["level1"]!.from).toEqual({ level2: { value: 1 } });
      expect(delta["level1"]!.to).toEqual({ level2: { value: 2 } });
    });

    it("should detect boolean changes", () => {
      const previous = { flag: true };
      const current = { flag: false };
      const delta = calculator.calculateDelta(previous, current);
      expect(delta["flag"]).toEqual({ from: true, to: false });
    });

    it("should treat same object reference as equal", () => {
      const obj = { nested: true };
      const state = { ref: obj };
      const delta = calculator.calculateDelta(state, { ref: obj });
      expect(delta).toEqual({});
    });
  });

  describe("applyDelta", () => {
    it("should return a copy of base when delta is empty", () => {
      const base = { a: 1, b: 2 };
      const result = calculator.applyDelta(base, {});
      expect(result).toEqual(base);
      expect(result).not.toBe(base); // Should be a new object
    });

    it("should update changed fields", () => {
      const base = { name: "foo", value: 42 };
      const delta = { name: { from: "foo", to: "bar" } };
      const result = calculator.applyDelta(base, delta);
      expect(result).toEqual({ name: "bar", value: 42 });
    });

    it("should remove deleted fields", () => {
      const base = { a: 1, b: 2, c: 3 };
      const delta = { b: { from: 2, to: undefined } };
      const result = calculator.applyDelta(base, delta);
      expect(result).toEqual({ a: 1, c: 3 });
      expect(result).not.toHaveProperty("b");
    });

    it("should handle multiple changes simultaneously", () => {
      const base = { a: 1, b: 2, c: 3, d: 4 };
      const delta = {
        a: { from: 1, to: 10 },
        c: { from: 3, to: undefined },
        d: { from: 4, to: 40 },
      };
      const result = calculator.applyDelta(base, delta);
      expect(result).toEqual({ a: 10, b: 2, d: 40 });
    });

    it("should not mutate the original base object", () => {
      const base = { a: 1, b: 2 };
      const delta = { a: { from: 1, to: 99 } };
      calculator.applyDelta(base, delta);
      expect(base.a).toBe(1); // Should be unchanged
    });

    it("should handle adding new fields via delta", () => {
      const base = { a: 1 };
      const delta = { b: { from: undefined, to: 2 } };
      const result = calculator.applyDelta(base, delta);
      expect(result).toEqual({ a: 1, b: 2 });
    });

    it("should handle nested object replacement", () => {
      const base = { nested: { x: 1, y: 2 } };
      const delta = { nested: { from: { x: 1, y: 2 }, to: { x: 99, y: 2 } } };
      const result = calculator.applyDelta(base, delta);
      expect(result).toEqual({ nested: { x: 99, y: 2 } });
    });
  });

  describe("mergeDeltas", () => {
    it("should return empty delta when both deltas are empty", () => {
      const result = calculator.mergeDeltas({}, {});
      expect(result).toEqual({});
    });

    it("should return second delta when first is empty", () => {
      const second = { a: { from: undefined, to: 1 } };
      const result = calculator.mergeDeltas({}, second);
      expect(result).toEqual(second);
    });

    it("should return first delta when second is empty", () => {
      const first = { a: { from: undefined, to: 1 } };
      const result = calculator.mergeDeltas(first, {});
      expect(result).toEqual(first);
    });

    it("should merge two deltas with different keys", () => {
      const first = { a: { from: undefined, to: 1 } };
      const second = { b: { from: undefined, to: 2 } };
      const result = calculator.mergeDeltas(first, second);
      expect(result).toEqual({
        a: { from: undefined, to: 1 },
        b: { from: undefined, to: 2 },
      });
    });

    it("should merge two deltas with overlapping keys (second overwrites)", () => {
      const first = { key: { from: 1, to: 10 } };
      const second = { key: { from: 10, to: 100 } };
      const result = calculator.mergeDeltas(first, second);
      expect(result).toEqual({ key: { from: 1, to: 100 } });
    });

    it("should handle key added in first delta then deleted in second", () => {
      const first = { key: { from: undefined, to: "value" } };
      const second = { key: { from: "value", to: undefined } };
      const result = calculator.mergeDeltas(first, second);
      expect(result).toEqual({ key: { from: undefined, to: undefined } });
    });

    it("should handle key deleted in first delta and absent from second", () => {
      const first = { key: { from: "original", to: undefined } };
      const second = {};
      const result = calculator.mergeDeltas(first, second);
      expect(result).toEqual({ key: { from: "original", to: undefined } });
    });

    it("should handle key unchanged in first delta but changed in second", () => {
      const first = {};
      const second = { key: { from: "s1_value", to: "s2_value" } };
      const result = calculator.mergeDeltas(first, second);
      expect(result).toEqual({ key: { from: "s1_value", to: "s2_value" } });
    });

    it("should merge deltas with mixed field modifications", () => {
      const first = {
        a: { from: 1, to: 10 },
        b: { from: 2, to: 20 },
        c: { from: 3, to: 30 },
      };
      const second = {
        b: { from: 20, to: 200 },
        c: { from: 30, to: undefined },
        d: { from: undefined, to: 400 },
      };
      const result = calculator.mergeDeltas(first, second);
      expect(result).toEqual({
        a: { from: 1, to: 10 },     // unchanged from first
        b: { from: 2, to: 200 },     // from from first, to from second
        c: { from: 3, to: undefined }, // from from first, to=undefined from second
        d: { from: undefined, to: 400 }, // from undefined (not in first), to from second
      });
    });

    it("should not mutate input deltas", () => {
      const first = { a: { from: 1, to: 2 } };
      const second = { a: { from: 2, to: 3 } };
      calculator.mergeDeltas(first, second);
      expect(first.a).toEqual({ from: 1, to: 2 });
      expect(second.a).toEqual({ from: 2, to: 3 });
    });

    it("merged delta should produce correct state when applied via applyDelta", () => {
      // Verify end-to-end: applyDelta(applyDelta(base, delta1), delta2)
      // should equal applyDelta(base, mergeDeltas(delta1, delta2))
      const base = { x: 1, y: 2, z: 3 };
      const delta1 = { x: { from: 1, to: 10 }, y: { from: 2, to: undefined } };
      const delta2 = { x: { from: 10, to: 100 }, z: { from: 3, to: 30 } };

      const step1 = calculator.applyDelta(base, delta1);
      const step2 = calculator.applyDelta(step1, delta2);

      const merged = calculator.mergeDeltas(delta1, delta2);
      const direct = calculator.applyDelta(base, merged);

      expect(direct).toEqual(step2);
      expect(direct).toEqual({ x: 100, z: 30 });
    });
  });

  describe("Edge Cases", () => {
    it("should handle objects with symbol keys (ignored by Object.keys)", () => {
      const sym = Symbol("test");
      const previous: Record<string | symbol, unknown> = { a: 1 };
      const current: Record<string | symbol, unknown> = { a: 1 };
      (previous as any)[sym] = "secret";
      (current as any)[sym] = "secret";
      const delta = calculator.calculateDelta(previous as any, current as any);
      expect(delta).toEqual({});
    });

    it("should handle undefined values in state", () => {
      const previous = { a: undefined, b: 1 };
      const current = { a: undefined, b: 2 };
      const delta = calculator.calculateDelta(previous, current);
      expect(delta).toHaveProperty("b");
      expect(delta).not.toHaveProperty("a"); // undefined === undefined
    });

    it("should handle Date objects (reference inequality)", () => {
      const date1 = new Date("2024-01-01");
      const date2 = new Date("2024-01-01");
      const previous = { date: date1 };
      const current = { date: date2 };
      const delta = calculator.calculateDelta(previous, current);
      // Different Date objects with same time → not deeply equal by our implementation
      // because deepEqual compares Object.keys which is empty for Date → equal lengths → true
      expect(delta).toEqual({});
    });
  });
});
