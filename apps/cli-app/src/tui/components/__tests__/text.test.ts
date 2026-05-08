import { describe, it, expect, beforeEach } from "vitest";
import { Text } from "../text.js";

describe("Text Component", () => {
  let text: Text;

  beforeEach(() => {
    text = new Text();
  });

  describe("Initialization", () => {
    it("should initialize with empty text", () => {
      const result = text.render(80);
      expect(result).toEqual([]);
    });

    it("should initialize with custom text", () => {
      const customText = new Text("Hello World");
      const result = customText.render(80);
      expect(result.length).toBeGreaterThan(0);
    });

    it("should accept padding parameters", () => {
      const paddedText = new Text("Test", 2, 1);
      const result = paddedText.render(40);
      expect(result.length).toBeGreaterThan(0);
    });

    it("should accept custom background function", () => {
      const bgFn = (str: string) => `\x1b[41m${str}\x1b[0m`;
      const textWithBg = new Text("Test", 1, 1, bgFn);
      const result = textWithBg.render(40);
      expect(result.length).toBeGreaterThan(0);
    });
  });

  describe("setText", () => {
    it("should update text content", () => {
      text.setText("New Text");
      const result = text.render(80);
      expect(result.some(line => line.includes("New Text"))).toBe(true);
    });

    it("should clear cache when text changes", () => {
      text.setText("First");
      const first = text.render(80);
      
      text.setText("Second");
      const second = text.render(80);
      
      expect(first).not.toEqual(second);
    });

    it("should handle empty string", () => {
      text.setText("");
      const result = text.render(80);
      expect(result).toEqual([]);
    });

    it("should handle whitespace-only text", () => {
      text.setText("   ");
      const result = text.render(80);
      expect(result).toEqual([]);
    });
  });

  describe("setCustomBgFn", () => {
    it("should set background function", () => {
      const bgFn = (str: string) => `\x1b[42m${str}\x1b[0m`;
      text.setCustomBgFn(bgFn);
      text.setText("Test");
      const result = text.render(40);
      expect(result.length).toBeGreaterThan(0);
    });

    it("should clear background function", () => {
      const bgFn = (str: string) => `\x1b[42m${str}\x1b[0m`;
      text.setCustomBgFn(bgFn);
      text.setCustomBgFn(undefined);
      text.setText("Test");
      const result = text.render(40);
      expect(result.length).toBeGreaterThan(0);
    });

    it("should invalidate cache when bgFn changes", () => {
      text.setText("Test");
      const bgFn1 = (str: string) => `\x1b[41m${str}\x1b[0m`;
      const bgFn2 = (str: string) => `\x1b[42m${str}\x1b[0m`;
      
      text.setCustomBgFn(bgFn1);
      const first = text.render(40);
      
      text.setCustomBgFn(bgFn2);
      const second = text.render(40);
      
      expect(first).not.toEqual(second);
    });
  });

  describe("render", () => {
    it("should render simple text", () => {
      text.setText("Hello");
      const result = text.render(80);
      expect(result.some(line => line.includes("Hello"))).toBe(true);
    });

    it("should handle multi-line text", () => {
      text.setText("Line 1\nLine 2\nLine 3");
      const result = text.render(80);
      expect(result.length).toBeGreaterThan(3); // Includes padding
    });

    it("should wrap long lines", () => {
      text.setText("This is a very long line that should be wrapped when rendered with a narrow width");
      const narrowResult = text.render(20);
      const wideResult = text.render(80);
      
      expect(narrowResult.length).toBeGreaterThan(wideResult.length);
    });

    it("should apply horizontal padding", () => {
      const paddedText = new Text("Test", 2, 0);
      const result = paddedText.render(40);
      // Each line should have padding on both sides
      result.forEach(line => {
        if (line.trim().length > 0) {
          expect(line.startsWith("  ")).toBe(true);
        }
      });
    });

    it("should apply vertical padding", () => {
      const paddedText = new Text("Test", 0, 2);
      const result = paddedText.render(40);
      // Should have empty lines at top and bottom
      expect(result.length).toBeGreaterThan(1);
    });

    it("should replace tabs with spaces", () => {
      text.setText("Hello\tWorld");
      const result = text.render(80);
      expect(result.some(line => line.includes("Hello   World"))).toBe(true);
    });

    it("should handle ANSI codes in text", () => {
      text.setText("\x1b[31mRed Text\x1b[0m");
      const result = text.render(80);
      expect(result.length).toBeGreaterThan(0);
    });

    it("should respect width parameter", () => {
      text.setText("A".repeat(100));
      const result = text.render(20);
      // Lines should not exceed width significantly
      result.forEach(line => {
        // Allow some margin for ANSI codes
        expect(line.length).toBeLessThanOrEqual(30);
      });
    });

    it("should return at least one line for non-empty text", () => {
      text.setText("Test");
      const result = text.render(80);
      expect(result.length).toBeGreaterThanOrEqual(1);
    });
  });

  describe("invalidate", () => {
    it("should not throw error when called", () => {
      expect(() => text.invalidate()).not.toThrow();
    });

    it("should force re-render", () => {
      text.setText("Test");
      const before = text.render(80);
      text.invalidate();
      const after = text.render(80);
      expect(before).toEqual(after);
    });
  });

  describe("Caching", () => {
    it("should cache rendered output", () => {
      text.setText("Test");
      const first = text.render(80);
      const second = text.render(80);
      expect(first).toEqual(second);
    });

    it("should invalidate cache on text change", () => {
      text.setText("First");
      const first = text.render(80);
      
      text.setText("Second");
      const second = text.render(80);
      
      expect(first).not.toEqual(second);
    });

    it("should invalidate cache on width change", () => {
      text.setText("Test");
      const narrow = text.render(20);
      const wide = text.render(80);
      
      expect(narrow).not.toEqual(wide);
    });
  });

  describe("Edge Cases", () => {
    it("should handle very long single word", () => {
      text.setText("A".repeat(500));
      const result = text.render(40);
      expect(result.length).toBeGreaterThan(0);
    });

    it("should handle special characters", () => {
      text.setText("!@#$%^&*()_+-=[]{}|;':\",./<>?");
      const result = text.render(80);
      expect(result.length).toBeGreaterThan(0);
    });

    it("should handle unicode characters", () => {
      text.setText("Hello 世界 🌍");
      const result = text.render(80);
      expect(result.length).toBeGreaterThan(0);
    });

    it("should handle empty lines in multi-line text", () => {
      text.setText("Line 1\n\nLine 3");
      const result = text.render(80);
      expect(result.length).toBeGreaterThan(0);
    });

    it("should maintain consistent rendering", () => {
      text.setText("Consistent Test");
      const renders = Array(5).fill(null).map(() => text.render(80));
      renders.forEach(render => {
        expect(render).toEqual(renders[0]);
      });
    });
  });
});
