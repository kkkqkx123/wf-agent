/**
 * Unit tests for DashboardScreen
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import { DashboardScreen } from "../dashboard-screen.js";
import { Container, Box, Text, SelectList } from "../../index.js";

describe("DashboardScreen", () => {
  let screen: DashboardScreen;
  let onNavigateMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    onNavigateMock = vi.fn();
    screen = new DashboardScreen(onNavigateMock);
  });

  describe("constructor", () => {
    it("should create instance without onNavigate callback", () => {
      const screenWithoutCallback = new DashboardScreen();
      expect(screenWithoutCallback).toBeDefined();
      expect(screenWithoutCallback.render()).toBeDefined();
    });

    it("should create instance with onNavigate callback", () => {
      expect(screen).toBeDefined();
      expect(onNavigateMock).toBeDefined();
    });
  });

  describe("render", () => {
    it("should return a Container component", () => {
      const result = screen.render();
      expect(result).toBeInstanceOf(Container);
    });

    it("should render header with title", () => {
      const container = screen.render() as Container;
      expect(container.children.length).toBeGreaterThan(0);
      
      // First child should be header box
      const header = container.children[0] as Box;
      expect(header).toBeInstanceOf(Box);
      
      // Header should contain text components
      const headerChildren = (header as any).children;
      expect(headerChildren.length).toBeGreaterThanOrEqual(2);
      expect(headerChildren[0]).toBeInstanceOf(Text);
      expect(headerChildren[1]).toBeInstanceOf(Text);
    });

    it("should render menu list with correct items", () => {
      const container = screen.render() as Container;
      
      // Second child should be the select list
      const menuList = container.children[1] as SelectList;
      expect(menuList).toBeInstanceOf(SelectList);
    });

    it("should render status panel", () => {
      const container = screen.render() as Container;
      
      // Third child should be status panel
      const statusPanel = container.children[2] as Box;
      expect(statusPanel).toBeInstanceOf(Box);
      
      const statusChildren = (statusPanel as any).children;
      expect(statusChildren.length).toBeGreaterThanOrEqual(3);
      expect(statusChildren[0]).toBeInstanceOf(Text);
      
      // Render with a reasonable width to get actual text
      const renderedLines = (statusChildren[0] as Text).render(80);
      expect(renderedLines.length).toBeGreaterThan(0);
      expect(renderedLines.some((line: string) => line.includes("Active Agents"))).toBe(true);
    });

    it("should render help box with keyboard shortcuts", () => {
      const container = screen.render() as Container;
      
      // Fourth child should be help box
      const helpBox = container.children[3] as Box;
      expect(helpBox).toBeInstanceOf(Box);
      
      const helpChildren = (helpBox as any).children;
      expect(helpChildren.length).toBeGreaterThanOrEqual(4);
      
      // Check for specific shortcuts - render each text component
      const helpText = helpChildren.map((child: any) => child.render(80)).flat();
      expect(helpText.some((line: string) => line.includes("Ctrl+Q"))).toBe(true);
      expect(helpText.some((line: string) => line.includes("Enter"))).toBe(true);
    });
  });

  describe("handleInput", () => {
    it("should delegate input to menu list", () => {
      const result = screen.handleInput!("arrowdown");
      expect(result).toBe(true);
    });

    it("should return true when handling input", () => {
      const result = screen.handleInput!("enter");
      expect(result).toBe(true);
    });

    it("should handle various navigation keys", () => {
      const keys = ["arrowup", "arrowdown", "enter"];
      
      keys.forEach(key => {
        const result = screen.handleInput!(key);
        expect(result).toBe(true);
      });
    });
  });

  describe("menu navigation", () => {
    it("should call onNavigate when menu item is selected", () => {
      const container = screen.render() as Container;
      const menuList = container.children[1] as SelectList;
      
      // Simulate selecting workflow option
      if (menuList.onSelect) {
        menuList.onSelect({ value: "workflow", label: "Workflow", description: "Test" });
      }
      
      expect(onNavigateMock).toHaveBeenCalledWith("workflow");
    });

    it("should navigate to agent screen", () => {
      const container = screen.render() as Container;
      const menuList = container.children[1] as SelectList;
      
      if (menuList.onSelect) {
        menuList.onSelect({ value: "agent", label: "Agent", description: "Test" });
      }
      
      expect(onNavigateMock).toHaveBeenCalledWith("agent");
    });

    it("should navigate to thread screen", () => {
      const container = screen.render() as Container;
      const menuList = container.children[1] as SelectList;
      
      if (menuList.onSelect) {
        menuList.onSelect({ value: "thread", label: "Thread", description: "Test" });
      }
      
      expect(onNavigateMock).toHaveBeenCalledWith("thread");
    });

    it("should navigate to checkpoint screen", () => {
      const container = screen.render() as Container;
      const menuList = container.children[1] as SelectList;
      
      if (menuList.onSelect) {
        menuList.onSelect({ value: "checkpoint", label: "Checkpoint", description: "Test" });
      }
      
      expect(onNavigateMock).toHaveBeenCalledWith("checkpoint");
    });

    it("should navigate to settings screen", () => {
      const container = screen.render() as Container;
      const menuList = container.children[1] as SelectList;
      
      if (menuList.onSelect) {
        menuList.onSelect({ value: "settings", label: "Settings", description: "Test" });
      }
      
      expect(onNavigateMock).toHaveBeenCalledWith("settings");
    });
  });

  describe("Screen interface compliance", () => {
    it("should implement render method", () => {
      expect(typeof screen.render).toBe("function");
    });

    it("should implement handleInput method", () => {
      expect(typeof screen.handleInput).toBe("function");
    });

    it("should not have onActivate method (optional)", () => {
      expect((screen as any).onActivate).toBeUndefined();
    });

    it("should not have onDeactivate method (optional)", () => {
      expect((screen as any).onDeactivate).toBeUndefined();
    });

    it("should not have destroy method (optional)", () => {
      expect((screen as any).destroy).toBeUndefined();
    });
  });
});
