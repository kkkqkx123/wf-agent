import { describe, it, expect, beforeEach } from "vitest";
import { DeltaCalculator, type DeltaCalculatorContext } from "../delta-calculator.js";

interface TestSnapshot {
  name?: string;
  value?: number;
  items?: string[];
  metadata?: Record<string, unknown>;
}

interface TestDelta {
  added: string[];
  modified: Map<string, { from: unknown; to: unknown }>;
  removed: string[];
}

class TestDeltaCalculator extends DeltaCalculator<TestSnapshot, TestDelta> {
  calculateDelta(
    previous: TestSnapshot,
    current: TestSnapshot,
    _context?: DeltaCalculatorContext,
  ): TestDelta {
    return this.calculateObjectDelta(
      previous as Record<string, unknown>,
      current as Record<string, unknown>,
    );
  }

  extractSnapshot(_checkpoint: unknown): TestSnapshot {
    return {};
  }
}

describe("DeltaCalculator", () => {
  let calculator: TestDeltaCalculator;

  beforeEach(() => {
    calculator = new TestDeltaCalculator();
  });

  describe("isEqual", () => {
    it("should return true for identical primitives", () => {
      expect(calculator["isEqual"](1, 1)).toBe(true);
      expect(calculator["isEqual"]("a", "a")).toBe(true);
      expect(calculator["isEqual"](true, true)).toBe(true);
      expect(calculator["isEqual"](null, null)).toBe(true);
      expect(calculator["isEqual"](undefined, undefined)).toBe(true);
    });

    it("should return false for different primitives", () => {
      expect(calculator["isEqual"](1, 2)).toBe(false);
      expect(calculator["isEqual"]("a", "b")).toBe(false);
      expect(calculator["isEqual"](true, false)).toBe(false);
    });

    it("should return true for identical objects", () => {
      expect(calculator["isEqual"]({ a: 1 }, { a: 1 })).toBe(true);
    });

    it("should return false for different objects", () => {
      expect(calculator["isEqual"]({ a: 1 }, { a: 2 })).toBe(false);
    });

    it("should return false when one value is null and the other is not", () => {
      expect(calculator["isEqual"](null, undefined)).toBe(false);
      expect(calculator["isEqual"](null, {})).toBe(false);
    });

    it("should return false for different types", () => {
      expect(calculator["isEqual"]("42", 42)).toBe(false);
    });

    it("should compare Dates by time", () => {
      const d1 = new Date("2024-01-01");
      const d2 = new Date("2024-01-01");
      const d3 = new Date("2024-06-15");
      expect(calculator["isEqual"](d1, d2)).toBe(true);
      expect(calculator["isEqual"](d1, d3)).toBe(false);
    });

    it("should compare arrays by element", () => {
      expect(calculator["isEqual"]([1, 2, 3], [1, 2, 3])).toBe(true);
      expect(calculator["isEqual"]([1, 2, 3], [1, 2, 4])).toBe(false);
      expect(calculator["isEqual"]([1, 2], [1, 2, 3])).toBe(false);
    });

    it("should compare Maps", () => {
      const m1 = new Map([["a", 1]]);
      const m2 = new Map([["a", 1]]);
      const m3 = new Map([["a", 2]]);
      expect(calculator["isEqual"](m1, m2)).toBe(true);
      expect(calculator["isEqual"](m1, m3)).toBe(false);
    });

    it("should compare Sets", () => {
      const s1 = new Set([1, 2, 3]);
      const s2 = new Set([1, 2, 3]);
      const s3 = new Set([1, 2, 4]);
      expect(calculator["isEqual"](s1, s2)).toBe(true);
      expect(calculator["isEqual"](s1, s3)).toBe(false);
    });

    it("should use JSON.stringify when strictCompare is disabled", () => {
      const shallowCalc = new TestDeltaCalculator({ strictCompare: false });
      expect(shallowCalc["isEqual"]({ a: 1 }, { a: 1 })).toBe(true);
      expect(shallowCalc["isEqual"]({ a: 1 }, { a: 2 })).toBe(false);
    });
  });

  describe("calculateObjectDelta", () => {
    it("should detect added keys", () => {
      const result = calculator["calculateObjectDelta"]({ a: 1 }, { a: 1, b: 2 });
      expect(result.added).toEqual(["b"]);
      expect(result.modified.size).toBe(0);
      expect(result.removed).toEqual([]);
    });

    it("should detect removed keys", () => {
      const result = calculator["calculateObjectDelta"]({ a: 1, b: 2 }, { a: 1 });
      expect(result.added).toEqual([]);
      expect(result.modified.size).toBe(0);
      expect(result.removed).toEqual(["b"]);
    });

    it("should detect modified keys", () => {
      const result = calculator["calculateObjectDelta"]({ a: 1, b: 2 }, { a: 10, b: 2 });
      expect(result.added).toEqual([]);
      expect(result.modified.size).toBe(1);
      expect(result.modified.get("a")).toEqual({ from: 1, to: 10 });
      expect(result.removed).toEqual([]);
    });

    it("should ignore fields in ignoreFields", () => {
      const calc = new TestDeltaCalculator({ ignoreFields: ["ignored"] });
      const result = calc["calculateObjectDelta"](
        { a: 1, ignored: "old" },
        { a: 1, ignored: "new" },
      );
      expect(result.modified.size).toBe(0);
    });

    it("should handle empty objects", () => {
      const result = calculator["calculateObjectDelta"]({}, {});
      expect(result.added).toEqual([]);
      expect(result.modified.size).toBe(0);
      expect(result.removed).toEqual([]);
    });

    it("should detect all types of changes simultaneously", () => {
      const result = calculator["calculateObjectDelta"](
        { a: 1, b: 2, c: 3 },
        { a: 10, b: 2, d: 4 },
      );
      expect(result.added).toEqual(["d"]);
      expect(result.modified.size).toBe(1);
      expect(result.modified.get("a")).toEqual({ from: 1, to: 10 });
      expect(result.removed).toEqual(["c"]);
    });
  });

  describe("calculateAppendedDelta", () => {
    it("should return all current items when previous is empty", () => {
      const result = calculator["calculateAppendedDelta"]([], ["a", "b"]);
      expect(result).toEqual(["a", "b"]);
    });

    it("should return empty when current is empty", () => {
      const result = calculator["calculateAppendedDelta"](["a", "b"], []);
      expect(result).toEqual([]);
    });

    it("should return newly appended items", () => {
      const result = calculator["calculateAppendedDelta"](["a", "b"], ["a", "b", "c", "d"]);
      expect(result).toEqual(["c", "d"]);
    });

    it("should return empty when no new items added", () => {
      const result = calculator["calculateAppendedDelta"](["a", "b"], ["a", "b"]);
      expect(result).toEqual([]);
    });

    it("should return empty when current is shorter than previous", () => {
      const result = calculator["calculateAppendedDelta"](["a", "b", "c"], ["a", "b"]);
      expect(result).toEqual([]);
    });

    it("should use keyExtractor for deduplication", () => {
      const items = [
        { id: "1", name: "a" },
        { id: "2", name: "b" },
      ];
      const newItems = [
        { id: "1", name: "a" },
        { id: "3", name: "c" },
      ];
      const result = calculator["calculateAppendedDelta"](items, newItems, item => item.id);
      expect(result).toEqual([{ id: "3", name: "c" }]);
    });

    it("should handle null/undefined previous", () => {
      const result = calculator["calculateAppendedDelta"](null as unknown as string[], ["a", "b"]);
      expect(result).toEqual(["a", "b"]);
    });
  });

  describe("calculateDelta", () => {
    it("should return empty modifications for identical states", () => {
      const result = calculator.calculateDelta(
        { name: "test", value: 42 },
        { name: "test", value: 42 },
      );
      expect(result.added).toEqual([]);
      expect(result.modified.size).toBe(0);
      expect(result.removed).toEqual([]);
    });

    it("should detect changes", () => {
      const result = calculator.calculateDelta(
        { name: "foo", value: 1 },
        { name: "bar", value: 2 },
      );
      expect(result.modified.get("name")).toEqual({ from: "foo", to: "bar" });
      expect(result.modified.get("value")).toEqual({ from: 1, to: 2 });
    });
  });

  describe("constructor options", () => {
    it("should use default options", () => {
      const calc = new TestDeltaCalculator();
      expect(calc["options"].strictCompare).toBe(true);
      expect(calc["options"].ignoreFields).toEqual([]);
    });

    it("should merge provided options with defaults", () => {
      const calc = new TestDeltaCalculator({ strictCompare: false, ignoreFields: ["timestamp"] });
      expect(calc["options"].strictCompare).toBe(false);
      expect(calc["options"].ignoreFields).toEqual(["timestamp"]);
    });
  });
});
