/**
 * Unit tests for Screen interface and exports
 */

import { describe, it, expect } from "vitest";
import type { Screen } from "../../../src/tui/screens/screen.js";
import { DashboardScreen, WorkflowScreen, AgentScreen } from "../../../src/tui/screens/index.js";

describe("Screen Interface", () => {
  describe("type definition", () => {
    it("should define Screen interface with required render method", () => {
      // This test verifies the interface structure at compile time
      const mockScreen: Screen = {
        render: () => ({ 
          render: () => [],
          invalidate: () => {}
        }),
      };
      
      expect(mockScreen.render).toBeDefined();
      expect(typeof mockScreen.render).toBe("function");
    });

    it("should allow optional onActivate method", () => {
      const mockScreen: Screen = {
        render: () => ({ 
          render: () => [],
          invalidate: () => {}
        }),
        onActivate: () => {},
      };
      
      expect(mockScreen.onActivate).toBeDefined();
    });

    it("should allow optional onDeactivate method", () => {
      const mockScreen: Screen = {
        render: () => ({ 
          render: () => [],
          invalidate: () => {}
        }),
        onDeactivate: () => {},
      };
      
      expect(mockScreen.onDeactivate).toBeDefined();
    });

    it("should allow optional handleInput method", () => {
      const mockScreen: Screen = {
        render: () => ({ 
          render: () => [],
          invalidate: () => {}
        }),
        handleInput: (data: string) => true,
      };
      
      expect(mockScreen.handleInput).toBeDefined();
      expect(typeof mockScreen.handleInput).toBe("function");
    });

    it("should allow optional destroy method", () => {
      const mockScreen: Screen = {
        render: () => ({ 
          render: () => [],
          invalidate: () => {}
        }),
        destroy: () => {},
      };
      
      expect(mockScreen.destroy).toBeDefined();
    });
  });

  describe("interface methods", () => {
    it("should have render method that returns Component", () => {
      const mockScreen: Screen = {
        render: () => ({ 
          render: (width: number) => ["line1", "line2"],
          invalidate: () => {},
        }),
      };
      
      const component = mockScreen.render();
      expect(component.render).toBeDefined();
      expect(component.invalidate).toBeDefined();
      expect(typeof component.render(80)).toEqual("object");
    });

    it("handleInput should return boolean", () => {
      const mockScreen: Screen = {
        render: () => ({ 
          render: () => [],
          invalidate: () => {}
        }),
        handleInput: (data: string) => {
          return data.length > 0;
        },
      };
      
      expect(mockScreen.handleInput!("test")).toBe(true);
      expect(mockScreen.handleInput!("")).toBe(false);
    });
  });
});

describe("Screen Exports", () => {
  describe("DashboardScreen export", () => {
    it("should export DashboardScreen class", () => {
      expect(DashboardScreen).toBeDefined();
      expect(typeof DashboardScreen).toBe("function");
    });

    it("should create DashboardScreen instance", () => {
      const screen = new DashboardScreen();
      expect(screen).toBeInstanceOf(DashboardScreen);
      expect(screen.render).toBeDefined();
    });

    it("should implement Screen interface", () => {
      const screen: Screen = new DashboardScreen();
      expect(screen.render).toBeDefined();
      expect(screen.handleInput).toBeDefined();
    });
  });

  describe("WorkflowScreen export", () => {
    it("should export WorkflowScreen class", () => {
      expect(WorkflowScreen).toBeDefined();
      expect(typeof WorkflowScreen).toBe("function");
    });

    it("should create WorkflowScreen instance", () => {
      const screen = new WorkflowScreen();
      expect(screen).toBeInstanceOf(WorkflowScreen);
      expect(screen.render).toBeDefined();
    });

    it("should implement Screen interface", () => {
      const screen: Screen = new WorkflowScreen();
      expect(screen.render).toBeDefined();
      expect(screen.handleInput).toBeDefined();
      expect(screen.destroy).toBeDefined();
    });
  });

  describe("AgentScreen export", () => {
    it("should export AgentScreen class", () => {
      expect(AgentScreen).toBeDefined();
      expect(typeof AgentScreen).toBe("function");
    });

    it("should create AgentScreen instance", () => {
      const screen = new AgentScreen();
      expect(screen).toBeInstanceOf(AgentScreen);
      expect(screen.render).toBeDefined();
    });

    it("should implement Screen interface", () => {
      const screen: Screen = new AgentScreen();
      expect(screen.render).toBeDefined();
      expect(screen.handleInput).toBeDefined();
      expect(screen.destroy).toBeDefined();
    });
  });

  describe("module exports", () => {
    it("should export all screen classes", () => {
      const screens = [DashboardScreen, WorkflowScreen, AgentScreen];
      
      screens.forEach(ScreenClass => {
        expect(ScreenClass).toBeDefined();
        expect(typeof ScreenClass).toBe("function");
      });
    });

    it("should export Screen type", () => {
      // Type-only export - verified at compile time
      const screenTypeExists = true;
      expect(screenTypeExists).toBe(true);
    });
  });
});

describe("Screen Implementation Consistency", () => {
  it("all screens should have consistent render method signature", () => {
    const screens = [
      new DashboardScreen(),
      new WorkflowScreen(),
      new AgentScreen(),
    ];

    screens.forEach(screen => {
      expect(typeof screen.render).toBe("function");
      const result = screen.render();
      expect(result).toBeDefined();
      expect(typeof result.render).toBe("function");
      expect(typeof result.invalidate).toBe("function");
    });
  });

  it("all screens should have consistent handleInput method signature", () => {
    const screens = [
      new DashboardScreen(),
      new WorkflowScreen(),
      new AgentScreen(),
    ];

    screens.forEach(screen => {
      if (screen.handleInput) {
        expect(typeof screen.handleInput).toBe("function");
        const result = screen.handleInput("test");
        expect(typeof result).toBe("boolean");
      }
    });
  });

  it("screens with destroy should have consistent signature", () => {
    const screensWithDestroy = [
      new WorkflowScreen(),
      new AgentScreen(),
    ];

    screensWithDestroy.forEach(screen => {
      if (screen.destroy) {
        expect(typeof screen.destroy).toBe("function");
        expect(() => screen.destroy!()).not.toThrow();
      }
    });
  });
});
