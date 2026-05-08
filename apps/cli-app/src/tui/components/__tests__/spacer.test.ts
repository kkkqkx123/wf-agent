import { describe, it, expect, beforeEach } from "vitest";
import { Spacer } from "../spacer.js";

describe("Spacer Component", () => {
  let spacer: Spacer;

  beforeEach(() => {
    spacer = new Spacer();
  });

  describe("Initialization", () => {
    it("should initialize with default 1 line", () => {
      const result = spacer.render(80);
      expect(result).toEqual([""]);
    });

    it("should initialize with custom number of lines", () => {
      const customSpacer = new Spacer(3);
      const result = customSpacer.render(80);
      expect(result).toEqual(["", "", ""]);
    });

    it("should handle zero lines", () => {
      const zeroSpacer = new Spacer(0);
      const result = zeroSpacer.render(80);
      expect(result).toEqual([]);
    });

    it("should handle negative lines (treated as 0)", () => {
      const negativeSpacer = new Spacer(-5);
      const result = negativeSpacer.render(80);
      expect(result).toEqual([]);
    });
  });

  describe("setLines", () => {
    it("should update the number of lines", () => {
      spacer.setLines(5);
      const result = spacer.render(80);
      expect(result.length).toBe(5);
      expect(result).toEqual(["", "", "", "", ""]);
    });

    it("should set to zero lines", () => {
      spacer.setLines(0);
      const result = spacer.render(80);
      expect(result).toEqual([]);
    });

    it("should allow changing lines multiple times", () => {
      spacer.setLines(2);
      expect(spacer.render(80).length).toBe(2);
      
      spacer.setLines(4);
      expect(spacer.render(80).length).toBe(4);
      
      spacer.setLines(1);
      expect(spacer.render(80).length).toBe(1);
    });
  });

  describe("render", () => {
    it("should return empty strings for each line", () => {
      spacer.setLines(3);
      const result = spacer.render(80);
      result.forEach(line => {
        expect(line).toBe("");
      });
    });

    it("should ignore width parameter", () => {
      spacer.setLines(2);
      const narrowResult = spacer.render(10);
      const wideResult = spacer.render(100);
      
      expect(narrowResult).toEqual(wideResult);
      expect(narrowResult).toEqual(["", ""]);
    });

    it("should always return array of empty strings", () => {
      spacer.setLines(5);
      const result = spacer.render(80);
      expect(Array.isArray(result)).toBe(true);
      expect(result.every(line => line === "")).toBe(true);
    });
  });

  describe("invalidate", () => {
    it("should not throw error when called", () => {
      expect(() => spacer.invalidate()).not.toThrow();
    });

    it("should not affect render output", () => {
      spacer.setLines(3);
      const before = spacer.render(80);
      spacer.invalidate();
      const after = spacer.render(80);
      expect(before).toEqual(after);
    });
  });

  describe("Edge Cases", () => {
    it("should handle very large number of lines", () => {
      spacer.setLines(1000);
      const result = spacer.render(80);
      expect(result.length).toBe(1000);
    });

    it("should maintain state between renders", () => {
      spacer.setLines(4);
      const first = spacer.render(80);
      const second = spacer.render(80);
      const third = spacer.render(80);
      
      expect(first).toEqual(second);
      expect(second).toEqual(third);
    });
  });
});
