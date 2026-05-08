/**
 * Simple test to verify TUI core functionality
 */

import { describe, it, expect } from "vitest";
import {
  TUI,
  ProcessTerminal,
  Container,
  Component,
  visibleWidth,
  truncateToWidth,
  wrapTextWithAnsi,
  fuzzyMatch,
  fuzzyFilter,
  KeybindingsManager,
  TUI_KEYBINDINGS,
  UndoStack,
  KillRing,
} from "../../src/tui/core/index.js";

describe("TUI Core - Phase 1", () => {
  describe("Utility Functions", () => {
    it("should calculate visible width correctly", () => {
      expect(visibleWidth("hello")).toBe(5);
      expect(visibleWidth("")).toBe(0);
      expect(visibleWidth("你好")).toBeGreaterThanOrEqual(4); // CJK characters are wider
    });

    it("should truncate text to width", () => {
      const result = truncateToWidth("hello world", 5, "...");
      expect(visibleWidth(result)).toBeLessThanOrEqual(5);
      expect(result).toContain("...");
    });

    it("should wrap text", () => {
      const lines = wrapTextWithAnsi("hello world foo bar", 10);
      expect(lines.length).toBeGreaterThan(1);
      lines.forEach((line) => {
        expect(visibleWidth(line)).toBeLessThanOrEqual(10);
      });
    });
  });

  describe("Fuzzy Matching", () => {
    it("should match fuzzy queries", () => {
      const result = fuzzyMatch("abc", "aXbXc");
      expect(result.matches).toBe(true);
      // Score can be negative (better match) or positive
      expect(typeof result.score).toBe("number");
    });

    it("should not match when characters missing", () => {
      const result = fuzzyMatch("xyz", "abc");
      expect(result.matches).toBe(false);
    });

    it("should filter and sort items", () => {
      const items = [
        { name: "apple" },
        { name: "banana" },
        { name: "apricot" },
      ];
      
      const filtered = fuzzyFilter(items, "app", (item) => item.name);
      // At least apple should match
      expect(filtered.length).toBeGreaterThanOrEqual(1);
      expect(filtered[0]?.name).toBe("apple");
    });
  });

  describe("UndoStack", () => {
    it("should push and pop states", () => {
      const stack = new UndoStack<string>();
      stack.push("state1");
      stack.push("state2");
      
      expect(stack.length).toBe(2);
      expect(stack.pop()).toBe("state2");
      expect(stack.pop()).toBe("state1");
      expect(stack.pop()).toBeUndefined();
    });

    it("should deep clone states", () => {
      interface State {
        value: number;
        nested: { data: string };
      }
      
      const stack = new UndoStack<State>();
      const state: State = { value: 1, nested: { data: "test" } };
      
      stack.push(state);
      state.value = 2;
      state.nested.data = "modified";
      
      const popped = stack.pop();
      expect(popped?.value).toBe(1);
      expect(popped?.nested.data).toBe("test");
    });
  });

  describe("KillRing", () => {
    it("should push and peek text", () => {
      const ring = new KillRing();
      ring.push("hello", { prepend: false });
      
      expect(ring.peek()).toBe("hello");
      expect(ring.length).toBe(1);
    });

    it("should accumulate consecutive kills", () => {
      const ring = new KillRing();
      ring.push("hello", { prepend: false });
      ring.push(" world", { prepend: false, accumulate: true });
      
      expect(ring.peek()).toBe("hello world");
      expect(ring.length).toBe(1);
    });

    it("should rotate entries", () => {
      const ring = new KillRing();
      ring.push("first", { prepend: false });
      ring.push("second", { prepend: false });
      ring.push("third", { prepend: false });
      
      ring.rotate();
      expect(ring.peek()).toBe("second");
    });
  });

  describe("KeybindingsManager", () => {
    it("should initialize with default bindings", () => {
      const manager = new KeybindingsManager(TUI_KEYBINDINGS);
      expect(manager).toBeDefined();
    });

    it("should match keybindings", () => {
      const manager = new KeybindingsManager(TUI_KEYBINDINGS);
      
      // Test that 'enter' keybinding exists
      const enterKeys = manager.getKeys("tui.input.submit");
      expect(enterKeys.length).toBeGreaterThan(0);
      expect(enterKeys).toContain("enter");
    });

    it("should detect conflicts", () => {
      const manager = new KeybindingsManager(TUI_KEYBINDINGS, {
        "tui.editor.cursorUp": "ctrl+a",
        "tui.select.up": "ctrl+a", // Conflict!
      });
      
      const conflicts = manager.getConflicts();
      expect(conflicts.length).toBeGreaterThan(0);
    });
  });

  describe("Container Component", () => {
    it("should add and render children", () => {
      const container = new Container();
      
      const mockComponent: Component = {
        render: (width: number) => ["line1", "line2"],
        invalidate: () => {},
      };
      
      container.addChild(mockComponent);
      const lines = container.render(80);
      
      expect(lines).toEqual(["line1", "line2"]);
    });

    it("should clear children", () => {
      const container = new Container();
      
      const mockComponent: Component = {
        render: () => ["test"],
        invalidate: () => {},
      };
      
      container.addChild(mockComponent);
      container.clear();
      
      expect(container.children.length).toBe(0);
    });
  });

  describe("Terminal Interface", () => {
    it("should create ProcessTerminal instance", () => {
      const terminal = new ProcessTerminal();
      expect(terminal).toBeDefined();
      expect(terminal.columns).toBeGreaterThan(0);
      expect(terminal.rows).toBeGreaterThan(0);
    });
  });

  describe("TUI Engine", () => {
    it("should create TUI instance", () => {
      const terminal = new ProcessTerminal();
      const tui = new TUI(terminal);
      
      expect(tui).toBeDefined();
      expect(tui.terminal).toBe(terminal);
    });

    it("should manage focus", () => {
      const terminal = new ProcessTerminal();
      const tui = new TUI(terminal);
      
      const component1: Component & { focused?: boolean } = {
        render: () => [],
        invalidate: () => {},
        focused: false,
      };
      
      const component2: Component & { focused?: boolean } = {
        render: () => [],
        invalidate: () => {},
        focused: false,
      };
      
      tui.setFocus(component1);
      expect(component1.focused).toBe(true);
      
      tui.setFocus(component2);
      expect(component1.focused).toBe(false);
      expect(component2.focused).toBe(true);
    });
  });
});
