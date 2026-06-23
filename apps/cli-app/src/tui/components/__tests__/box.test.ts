import { describe, it, expect, beforeEach, vi } from "vitest";
import { Box } from "../box.js";
import type { Component } from "../../core/tui.js";

// Mock component for testing
class MockComponent implements Component {
  private content: string;
  
  constructor(content: string = "Mock") {
    this.content = content;
  }
  
  render(width: number): string[] {
    return [`${this.content} (${width}px)`];
  }
  
  invalidate(): void {}
}

describe("Box Component", () => {
  let box: Box;

  beforeEach(() => {
    box = new Box();
  });

  describe("Initialization", () => {
    it("should initialize with default padding", () => {
      const result = box.render(80);
      expect(result).toEqual([]);
    });

    it("should accept custom padding", () => {
      const paddedBox = new Box(2, 3);
      expect(paddedBox).toBeDefined();
    });

    it("should accept background function", () => {
      const bgFn = (str: string) => `\x1b[41m${str}\x1b[0m`;
      const boxWithBg = new Box(1, 1, bgFn);
      expect(boxWithBg).toBeDefined();
    });
  });

  describe("addChild", () => {
    it("should add a child component", () => {
      const child = new MockComponent("Child1");
      box.addChild(child);
      
      const result = box.render(80);
      expect(result.length).toBeGreaterThan(0);
    });

    it("should add multiple children", () => {
      const child1 = new MockComponent("Child1");
      const child2 = new MockComponent("Child2");
      box.addChild(child1);
      box.addChild(child2);
      
      const result = box.render(80);
      expect(result.length).toBeGreaterThan(0);
    });

    it("should apply horizontal padding to children", () => {
      const paddedBox = new Box(2, 0);
      const child = new MockComponent("Test");
      paddedBox.addChild(child);
      
      const result = paddedBox.render(40);
      // Lines should have left padding
      result.forEach(line => {
        if (line.trim().length > 0) {
          expect(line.startsWith("  ")).toBe(true);
        }
      });
    });

    it("should apply vertical padding", () => {
      const paddedBox = new Box(0, 2);
      const child = new MockComponent("Test");
      paddedBox.addChild(child);
      
      const result = paddedBox.render(40);
      expect(result.length).toBeGreaterThan(1);
    });
  });

  describe("removeChild", () => {
    it("should remove a child component", () => {
      const child = new MockComponent("Child");
      box.addChild(child);
      box.removeChild(child);
      
      const result = box.render(80);
      expect(result).toEqual([]);
    });

    it("should handle removing non-existent child", () => {
      const child = new MockComponent("Child");
      const otherChild = new MockComponent("Other");
      box.addChild(child);
      
      expect(() => box.removeChild(otherChild)).not.toThrow();
    });

    it("should update rendering after removal", () => {
      const child1 = new MockComponent("Child1");
      const child2 = new MockComponent("Child2");
      box.addChild(child1);
      box.addChild(child2);
      
      const before = box.render(80);
      box.removeChild(child1);
      const after = box.render(80);
      
      expect(before).not.toEqual(after);
    });
  });

  describe("clear", () => {
    it("should remove all children", () => {
      box.addChild(new MockComponent("Child1"));
      box.addChild(new MockComponent("Child2"));
      box.clear();
      
      const result = box.render(80);
      expect(result).toEqual([]);
    });

    it("should clear cache", () => {
      box.addChild(new MockComponent("Test"));
      box.render(80);
      box.clear();
      
      const result = box.render(80);
      expect(result).toEqual([]);
    });
  });

  describe("setBgFn", () => {
    it("should set background function", () => {
      const bgFn = (str: string) => `\x1b[42m${str}\x1b[0m`;
      box.setBgFn(bgFn);
      box.addChild(new MockComponent("Test"));
      
      const result = box.render(40);
      expect(result.length).toBeGreaterThan(0);
    });

    it("should clear background function", () => {
      const bgFn = (str: string) => `\x1b[42m${str}\x1b[0m`;
      box.setBgFn(bgFn);
      box.setBgFn(undefined);
      box.addChild(new MockComponent("Test"));
      
      const result = box.render(40);
      expect(result.length).toBeGreaterThan(0);
    });
  });

  describe("render", () => {
    it("should return empty array when no children", () => {
      const result = box.render(80);
      expect(result).toEqual([]);
    });

    it("should render children with padding", () => {
      const paddedBox = new Box(1, 1);
      paddedBox.addChild(new MockComponent("Content"));
      
      const result = paddedBox.render(40);
      expect(result.length).toBeGreaterThan(1); // Content + top/bottom padding
    });

    it("should respect width parameter", () => {
      box.addChild(new MockComponent("Test"));
      const narrowResult = box.render(20);
      const wideResult = box.render(80);
      
      expect(narrowResult).not.toEqual(wideResult);
    });

    it("should cache rendered output", () => {
      box.addChild(new MockComponent("Test"));
      const first = box.render(80);
      const second = box.render(80);
      
      expect(first).toEqual(second);
    });

    it("should invalidate cache when children change", () => {
      box.addChild(new MockComponent("First"));
      const first = box.render(80);
      
      box.addChild(new MockComponent("Second"));
      const second = box.render(80);
      
      expect(first).not.toEqual(second);
    });

    it("should handle zero width gracefully", () => {
      box.addChild(new MockComponent("Test"));
      const result = box.render(0);
      expect(Array.isArray(result)).toBe(true);
    });

    it("should handle negative width gracefully", () => {
      box.addChild(new MockComponent("Test"));
      const result = box.render(-10);
      expect(Array.isArray(result)).toBe(true);
    });
  });

  describe("invalidate", () => {
    it("should not throw error when called", () => {
      expect(() => box.invalidate()).not.toThrow();
    });

    it("should invalidate cache", () => {
      box.addChild(new MockComponent("Test"));
      box.render(80);
      box.invalidate();
      
      // Should re-render without errors
      const result = box.render(80);
      expect(result.length).toBeGreaterThan(0);
    });

    it("should call invalidate on children", () => {
      const child = new MockComponent("Test");
      const invalidateSpy = vi.spyOn(child, 'invalidate');
      box.addChild(child);
      
      box.invalidate();
      expect(invalidateSpy).toHaveBeenCalled();
    });
  });

  describe("Caching", () => {
    it("should cache based on width", () => {
      box.addChild(new MockComponent("Test"));
      const width20 = box.render(20);
      const width80 = box.render(80);
      
      expect(width20).not.toEqual(width80);
    });

    it("should cache based on child content", () => {
      const child = new MockComponent("Original");
      box.addChild(child);
      const first = box.render(80);
      
      // Modify child by replacing it
      box.clear();
      box.addChild(new MockComponent("Modified"));
      const second = box.render(80);
      
      expect(first).not.toEqual(second);
    });

    it("should cache based on background function", () => {
      box.addChild(new MockComponent("Test"));
      
      const bgFn1 = (str: string) => `\x1b[41m${str}\x1b[0m`;
      box.setBgFn(bgFn1);
      const first = box.render(40);
      
      const bgFn2 = (str: string) => `\x1b[42m${str}\x1b[0m`;
      box.setBgFn(bgFn2);
      const second = box.render(40);
      
      expect(first).not.toEqual(second);
    });
  });

  describe("Edge Cases", () => {
    it("should handle many children", () => {
      for (let i = 0; i < 50; i++) {
        box.addChild(new MockComponent(`Child${i}`));
      }
      
      const result = box.render(80);
      expect(result.length).toBeGreaterThan(0);
    });

    it("should handle very large padding", () => {
      const bigPaddingBox = new Box(10, 10);
      bigPaddingBox.addChild(new MockComponent("Test"));
      
      const result = bigPaddingBox.render(80);
      expect(result.length).toBeGreaterThan(20); // At least 20 lines of padding
    });

    it("should maintain consistent rendering", () => {
      box.addChild(new MockComponent("Consistent"));
      const renders = Array(5).fill(null).map(() => box.render(80));
      
      renders.forEach(render => {
        expect(render).toEqual(renders[0]);
      });
    });

    it("should handle width smaller than content", () => {
      box.addChild(new MockComponent("Very long content that exceeds width"));
      const result = box.render(10);
      expect(Array.isArray(result)).toBe(true);
    });
  });
});
