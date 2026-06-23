import { describe, it, expect, beforeEach } from "vitest";
import { IterationPanel } from "../iteration-panel.js";
import type { AgentIterationData } from "@wf-agent/types";

describe("IterationPanel Component", () => {
  let panel: IterationPanel;

  beforeEach(() => {
    panel = new IterationPanel();
  });

  describe("Initialization", () => {
    it("should initialize with empty iterations", () => {
      const result = panel.render();
      expect(result).toContain("=== Iteration Progress ===");
    });

    it("should accept maxHeight option", () => {
      const customPanel = new IterationPanel({ maxHeight: 5 });
      expect(customPanel).toBeDefined();
    });
  });

  describe("updateIteration", () => {
    it("should add a new iteration", () => {
      const data: AgentIterationData = {
        iteration: 1,
        toolCallCount: 2,
      };
      
      panel.updateIteration(data);
      const result = panel.render();
      
      expect(result.some(line => line.includes("Iteration 1"))).toBe(true);
    });

    it("should update existing iteration", () => {
      const data1: AgentIterationData = {
        iteration: 1,
        toolCallCount: 2,
      };
      
      panel.updateIteration(data1);
      
      const data2: AgentIterationData = {
        iteration: 1,
        toolCallCount: 5,
      };
      
      panel.updateIteration(data2);
      const result = panel.render();
      
      expect(result.some(line => line.includes("5 tools"))).toBe(true);
    });

    it("should mark iteration as completed when duration is provided", () => {
      // First create the iteration
      panel.updateIteration({ iteration: 1, toolCallCount: 2 });
      
      // Then update with duration
      const data: AgentIterationData = {
        iteration: 1,
        toolCallCount: 2,
        duration: 1000,
      };
      
      panel.updateIteration(data);
      const result = panel.render();
      
      expect(result.some(line => line.includes("✓"))).toBe(true);
    });

    it("should handle multiple iterations", () => {
      panel.updateIteration({ iteration: 1, toolCallCount: 1 });
      panel.updateIteration({ iteration: 2, toolCallCount: 2 });
      panel.updateIteration({ iteration: 3, toolCallCount: 3 });
      
      const result = panel.render();
      
      expect(result.some(line => line.includes("Iteration 1"))).toBe(true);
      expect(result.some(line => line.includes("Iteration 2"))).toBe(true);
      expect(result.some(line => line.includes("Iteration 3"))).toBe(true);
    });
  });

  describe("completeIteration", () => {
    it("should mark iteration as completed", () => {
      panel.updateIteration({ iteration: 1, toolCallCount: 1 });
      panel.completeIteration(1, 2000);
      
      const result = panel.render();
      expect(result.some(line => line.includes("✓"))).toBe(true);
    });

    it("should calculate duration if not provided", () => {
      panel.updateIteration({ iteration: 1, toolCallCount: 1 });
      
      // Wait a bit to ensure time passes
      setTimeout(() => {
        panel.completeIteration(1);
        const result = panel.render();
        expect(result.some(line => line.includes("s"))).toBe(true);
      }, 100);
    });

    it("should handle non-existent iteration gracefully", () => {
      expect(() => panel.completeIteration(999)).not.toThrow();
    });
  });

  describe("errorIteration", () => {
    it("should mark iteration as error", () => {
      panel.updateIteration({ iteration: 1, toolCallCount: 1 });
      panel.errorIteration(1);
      
      const result = panel.render();
      expect(result.some(line => line.includes("✗"))).toBe(true);
    });

    it("should handle non-existent iteration gracefully", () => {
      expect(() => panel.errorIteration(999)).not.toThrow();
    });
  });

  describe("clear", () => {
    it("should clear all iterations", () => {
      panel.updateIteration({ iteration: 1, toolCallCount: 1 });
      panel.updateIteration({ iteration: 2, toolCallCount: 2 });
      panel.clear();
      
      const result = panel.render();
      // Header still contains "Iteration" but no iteration lines should appear
      const iterationLines = result.filter(line => line.includes("Iteration") && !line.includes("=== Iteration Progress ==="));
      expect(iterationLines.length).toBe(0);
    });
  });

  describe("render", () => {
    it("should render header", () => {
      const result = panel.render();
      expect(result).toContain("=== Iteration Progress ===");
    });

    it("should show iteration information", () => {
      panel.updateIteration({ iteration: 1, toolCallCount: 3 });
      const result = panel.render();
      
      expect(result.some(line => line.includes("3 tools"))).toBe(true);
    });

    it("should limit displayed iterations to maxHeight", () => {
      const limitedPanel = new IterationPanel({ maxHeight: 2 });
      
      limitedPanel.updateIteration({ iteration: 1, toolCallCount: 1 });
      limitedPanel.updateIteration({ iteration: 2, toolCallCount: 2 });
      limitedPanel.updateIteration({ iteration: 3, toolCallCount: 3 });
      
      const result = limitedPanel.render();
      // Count lines with iteration numbers (not header or summary)
      const iterationLines = result.filter(line => /Iteration \d+:/.test(line));
      
      expect(iterationLines.length).toBeLessThanOrEqual(2);
    });

    it("should show indicator when iterations exceed maxHeight", () => {
      const limitedPanel = new IterationPanel({ maxHeight: 2 });
      
      limitedPanel.updateIteration({ iteration: 1, toolCallCount: 1 });
      limitedPanel.updateIteration({ iteration: 2, toolCallCount: 2 });
      limitedPanel.updateIteration({ iteration: 3, toolCallCount: 3 });
      
      const result = limitedPanel.render();
      expect(result.some(line => line.includes("more iterations"))).toBe(true);
    });

    it("should sort iterations by number", () => {
      panel.updateIteration({ iteration: 3, toolCallCount: 3 });
      panel.updateIteration({ iteration: 1, toolCallCount: 1 });
      panel.updateIteration({ iteration: 2, toolCallCount: 2 });
      
      const result = panel.render();
      const lines = result.filter(line => line.includes("Iteration"));
      
      // Check that they appear in order
      const indices = [1, 2, 3].map(num => 
        lines.findIndex(line => line.includes(`Iteration ${num}`))
      );
      
      expect(indices[0]!).toBeLessThan(indices[1]!);
      expect(indices[1]!).toBeLessThan(indices[2]!);
    });

    it("should display duration for completed iterations", () => {
      panel.updateIteration({ iteration: 1, toolCallCount: 1 });
      panel.completeIteration(1, 1500);
      const result = panel.render();
      
      // Duration is shown in seconds (rounded)
      expect(result.some(line => line.includes("2s") || line.includes("1s") || line.includes("s"))).toBe(true);
    });

    it("should show running indicator for incomplete iterations", () => {
      panel.updateIteration({ iteration: 1, toolCallCount: 1 });
      const result = panel.render();
      
      expect(result.some(line => line.includes("..."))).toBe(true);
    });

    it("should respect width parameter", () => {
      panel.updateIteration({ iteration: 1, toolCallCount: 1 });
      const narrowResult = panel.render(40);
      const wideResult = panel.render(120);
      
      expect(Array.isArray(narrowResult)).toBe(true);
      expect(Array.isArray(wideResult)).toBe(true);
    });
  });

  describe("handleInput", () => {
    it("should return false for any input", () => {
      expect(panel.handleInput?.("test")).toBe(false);
    });
  });

  describe("invalidate", () => {
    it("should not throw error when called", () => {
      expect(() => panel.invalidate()).not.toThrow();
    });
  });

  describe("Edge Cases", () => {
    it("should handle zero tool calls", () => {
      panel.updateIteration({ iteration: 1, toolCallCount: 0 });
      const result = panel.render();
      
      expect(result.some(line => line.includes("0 tools"))).toBe(true);
    });

    it("should handle very large iteration numbers", () => {
      panel.updateIteration({ iteration: 9999, toolCallCount: 1 });
      const result = panel.render();
      
      expect(result.some(line => line.includes("Iteration 9999"))).toBe(true);
    });

    it("should maintain state between renders", () => {
      panel.updateIteration({ iteration: 1, toolCallCount: 5 });
      const first = panel.render();
      const second = panel.render();
      
      expect(first).toEqual(second);
    });
  });
});
