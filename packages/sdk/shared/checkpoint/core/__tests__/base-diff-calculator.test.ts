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

    it("should detect deleted fields", () => {
      const previous = { a: 1, b: 2, c: 3 };
      const current = { a: 1, c: 3 };
      const delta = calculator.calculateDelta(previous, current);
      expect(delta).toHaveProperty("b");
      expect(delta["b"]).toEqual({ from: 2, to: undefined });
    });

    it("should detect added fields", () => {
      const previous = { a: 1 };
      const current = { a: 1, b: 2 };
      const delta = calculator.calculateDelta(previous, current);
      expect(delta).toHaveProperty("b");
      expect(delta["b"]).toEqual({ from: undefined, to: 2 });
    });

    it("should handle nested objects", () => {
      const previous = { nested: { x: 1, y: 2 } };
      const current = { nested: { x: 1, y: 3 } };
      const delta = calculator.calculateDelta(previous, current);
      expect(delta).toHaveProperty("nested");
      expect(delta["nested"]).toEqual({ from: { x: 1, y: 2 }, to: { x: 1, y: 3 } });
    });

    it("should handle empty objects", () => {
      const delta = calculator.calculateDelta({}, {});
      expect(delta).toEqual({});
    });

    it("should handle Map objects", () => {
      const prevMap = new Map([["key1", "value1"]]);
      const currMap = new Map([["key1", "value2"]]);
      const previous = { data: prevMap };
      const current = { data: currMap };
      const delta = calculator.calculateDelta(previous, current);
      expect(delta).toHaveProperty("data");
    });

    it("should handle Set objects", () => {
      const prevSet = new Set([1, 2, 3]);
      const currSet = new Set([1, 2, 4]);
      const previous = { data: prevSet };
      const current = { data: currSet };
      const delta = calculator.calculateDelta(previous, current);
      expect(delta).toHaveProperty("data");
    });
  });

  describe("applyDelta", () => {
    it("should apply changes to base state", () => {
      const base = { a: 1, b: 2, c: 3 };
      const delta = { b: { from: 2, to: 99 } };
      const result = calculator.applyDelta(base, delta);
      expect(result).toEqual({ a: 1, b: 99, c: 3 });
    });

    it("should delete fields with undefined value", () => {
      const base = { a: 1, b: 2 };
      const delta = { b: { from: 2, to: undefined } };
      const result = calculator.applyDelta(base, delta);
      expect(result).toEqual({ a: 1 });
      expect(result).not.toHaveProperty("b");
    });

    it("should add new fields", () => {
      const base = { a: 1 };
      const delta = { b: { from: undefined, to: 2 } };
      const result = calculator.applyDelta(base, delta);
      expect(result).toEqual({ a: 1, b: 2 });
    });

    it("should not mutate original base state", () => {
      const base = { a: 1, b: 2 };
      const delta = { b: { from: 2, to: 99 } };
      calculator.applyDelta(base, delta);
      expect(base).toEqual({ a: 1, b: 2 });
    });

    it("should handle empty delta", () => {
      const base = { a: 1, b: 2 };
      const result = calculator.applyDelta(base, {});
      expect(result).toEqual(base);
    });
  });

  describe("mergeDeltas", () => {
    it("should merge two deltas", () => {
      const delta1 = { a: { from: 1, to: 2 }, b: { from: 10, to: 20 } };
      const delta2 = { b: { from: 20, to: 30 }, c: { from: 0, to: 1 } };
      const merged = calculator.mergeDeltas(delta1, delta2);
      expect(merged["a"]).toEqual({ from: 1, to: 2 });
      expect(merged["b"]).toEqual({ from: 10, to: 30 });
      expect(merged["c"]).toEqual({ from: 0, to: 1 });
    });

    it("should return second delta when first is empty", () => {
      const delta2 = { a: { from: 1, to: 2 } };
      const merged = calculator.mergeDeltas({}, delta2);
      expect(merged).toEqual(delta2);
    });

    it("should return first delta when second is empty", () => {
      const delta1 = { a: { from: 1, to: 2 } };
      const merged = calculator.mergeDeltas(delta1, {});
      expect(merged).toEqual(delta1);
    });
  });
});
