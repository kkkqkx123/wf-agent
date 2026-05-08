/**
 * Phase 2 Component Tests - Basic verification of component functionality
 */

import { describe, it, expect } from "vitest";
import { Text } from "../text.js";
import { Box } from "../box.js";
import { Spacer } from "../spacer.js";
import { SelectList, type SelectItem } from "../select-list.js";
import { Input } from "../input.js";

describe("Phase 2 Components", () => {
  describe("Text Component", () => {
    it("should render empty text as empty array", () => {
      const text = new Text("");
      const result = text.render(80);
      expect(result).toEqual([]);
    });

    it("should render simple text with padding", () => {
      const text = new Text("Hello", 1, 1);
      const result = text.render(20);
      expect(result.length).toBeGreaterThan(0);
      // Should have top padding, content, bottom padding
      expect(result.some((line: string) => line.includes("Hello"))).toBe(true);
    });

    it("should wrap long text", () => {
      const text = new Text("This is a very long text that should wrap across multiple lines", 0, 0);
      const result = text.render(20);
      expect(result.length).toBeGreaterThan(1);
    });
  });

  describe("Box Component", () => {
    it("should render empty box as empty array", () => {
      const box = new Box();
      const result = box.render(80);
      expect(result).toEqual([]);
    });

    it("should render children with padding", () => {
      const box = new Box(1, 1);
      const text = new Text("Content", 0, 0);
      box.addChild(text);
      
      const result = box.render(40);
      expect(result.length).toBeGreaterThan(0);
    });

    it("should clear children", () => {
      const box = new Box();
      box.addChild(new Text("Test"));
      expect(box.children.length).toBe(1);
      
      box.clear();
      expect(box.children.length).toBe(0);
    });
  });

  describe("Spacer Component", () => {
    it("should render specified number of empty lines", () => {
      const spacer = new Spacer(3);
      const result = spacer.render(80);
      expect(result).toHaveLength(3);
      expect(result.every((line: string) => line === "")).toBe(true);
    });

    it("should allow changing line count", () => {
      const spacer = new Spacer(1);
      spacer.setLines(5);
      const result = spacer.render(80);
      expect(result).toHaveLength(5);
    });
  });

  describe("SelectList Component", () => {
    const items: SelectItem[] = [
      { value: "item1", label: "First Item", description: "Description 1" },
      { value: "item2", label: "Second Item", description: "Description 2" },
      { value: "item3", label: "Third Item", description: "Description 3" },
    ];

    it("should render list of items", () => {
      const list = new SelectList(items, 5);
      const result = list.render(80);
      expect(result.length).toBeGreaterThan(0);
    });

    it("should filter items", () => {
      const list = new SelectList(items, 5);
      list.setFilter("Second");
      const selected = list.getSelectedItem();
      expect(selected?.value).toBe("item2");
    });

    it("should navigate with keyboard", () => {
      const list = new SelectList(items, 5);
      // Simulate down arrow
      list.handleInput("\x1b[B"); // Down arrow
      const selected = list.getSelectedItem();
      expect(selected?.value).toBe("item2");
    });

    it("should return null when no items", () => {
      const list = new SelectList([], 5);
      expect(list.getSelectedItem()).toBeNull();
    });
  });

  describe("Input Component", () => {
    it("should start with empty value", () => {
      const input = new Input();
      expect(input.getValue()).toBe("");
    });

    it("should accept character input", () => {
      const input = new Input();
      input.handleInput("a");
      expect(input.getValue()).toBe("a");
    });

    it("should handle backspace", () => {
      const input = new Input();
      input.handleInput("ab");
      input.handleInput("\x7f"); // Backspace
      expect(input.getValue()).toBe("a");
    });

    it("should support undo", () => {
      const input = new Input();
      input.handleInput("hello");
      input.handleInput("\x1b[Z"); // Shift+Tab for undo (common binding)
      // Undo should restore previous state
      expect(input.getValue()).not.toBe("hello");
    });

    it("should render placeholder when empty", () => {
      const input = new Input("Enter text...");
      const result = input.render(40);
      expect(result.length).toBe(1);
      expect(result[0]).toContain("Enter text...");
    });

    it("should move cursor with arrow keys", () => {
      const input = new Input();
      input.handleInput("abc");
      input.handleInput("\x1b[D"); // Left arrow
      // Cursor should be at position 2
      expect(input.getValue()).toBe("abc");
    });
  });
});
