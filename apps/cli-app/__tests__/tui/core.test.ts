/**
 * TUI core integration tests
 *
 * This file tests TUI core functionality that is NOT covered by
 * src/tui/core/__tests__/ unit tests (fuzzy, undo-stack, kill-ring,
 * keybindings, autocomplete). Those unit tests are the canonical
 * source for the respective modules.
 *
 * Tests kept here cover higher-level integration between TUI core
 * components: Container, ProcessTerminal, TUI engine, and utility
 * functions that are not separately unit-tested.
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
} from "../../src/tui/core/index.js";

describe("TUI Core - Integration", () => {
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