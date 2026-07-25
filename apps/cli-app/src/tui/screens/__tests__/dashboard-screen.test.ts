/**
 * Unit tests for DashboardScreen
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import { DashboardScreen } from "../dashboard-screen.js";
import { Container, Text, SelectList } from "../../index.js";

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

    it("should render menu list as first child", () => {
      const container = screen.render() as Container;
      const menuList = container.children[0] as SelectList;
      expect(menuList).toBeInstanceOf(SelectList);
    });

    it("should render status bar as last child", () => {
      const container = screen.render() as Container;
      const statusBar = container.children[container.children.length - 1] as Text;
      expect(statusBar).toBeInstanceOf(Text);
      const rendered = statusBar.render(80);
      expect(rendered[0]).toContain("navigate");
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
  });

  describe("menu navigation", () => {
    it("should call onNavigate when menu item is selected", () => {
      const container = screen.render() as Container;
      const menuList = container.children[0] as SelectList;

      if (menuList.onSelect) {
        menuList.onSelect({ value: "workflow", label: "Workflows", description: "" });
      }

      expect(onNavigateMock).toHaveBeenCalledWith("workflow");
    });

    it("should navigate to agent screen", () => {
      const container = screen.render() as Container;
      const menuList = container.children[0] as SelectList;

      if (menuList.onSelect) {
        menuList.onSelect({ value: "agent", label: "Agent Loops", description: "" });
      }

      expect(onNavigateMock).toHaveBeenCalledWith("agent");
    });
  });

  describe("Screen interface compliance", () => {
    it("should implement render method", () => {
      expect(typeof screen.render).toBe("function");
    });

    it("should implement handleInput method", () => {
      expect(typeof screen.handleInput).toBe("function");
    });

    it("should have onActivate method", () => {
      expect(typeof (screen as any).onActivate).toBe("function");
    });

    it("should have destroy method", () => {
      expect(typeof (screen as any).destroy).toBe("function");
    });
  });
});
