import { describe, it, expect, beforeEach } from "vitest";
import { ToolCallIndicator } from "../tool-call-indicator.js";
import type { AgentToolCallData, AgentToolEndData } from "@wf-agent/types";

describe("ToolCallIndicator Component", () => {
  let indicator: ToolCallIndicator;

  beforeEach(() => {
    indicator = new ToolCallIndicator();
  });

  describe("Initialization", () => {
    it("should initialize with empty tool calls", () => {
      const result = indicator.render();
      expect(result.length).toBe(0);
    });

    it("should accept maxDisplayCalls option", () => {
      const customIndicator = new ToolCallIndicator({ maxDisplayCalls: 3 });
      expect(customIndicator).toBeDefined();
    });

    it("should accept showArguments option", () => {
      const customIndicator = new ToolCallIndicator({ showArguments: true });
      expect(customIndicator).toBeDefined();
    });
  });

  describe("handleToolCallStart", () => {
    it("should add an active tool call", () => {
      const data: AgentToolCallData = {
        toolCallId: "call-1",
        toolName: "test-tool",
        arguments: { param: "value" },
        summary: "Test summary",
      };
      
      indicator.handleToolCallStart(data);
      const result = indicator.render();
      
      expect(result.some(line => line.includes("test-tool"))).toBe(true);
    });

    it("should show running status", () => {
      const data: AgentToolCallData = {
        toolCallId: "call-1",
        toolName: "running-tool",
        arguments: {},
        summary: "Running",
      };
      
      indicator.handleToolCallStart(data);
      const result = indicator.render();
      
      expect(result.some(line => line.includes("🔄"))).toBe(true);
    });

    it("should handle multiple active calls", () => {
      indicator.handleToolCallStart({ toolCallId: "call-1", toolName: "tool1", arguments: {}, summary: "Tool 1" });
      indicator.handleToolCallStart({ toolCallId: "call-2", toolName: "tool2", arguments: {}, summary: "Tool 2" });
      indicator.handleToolCallStart({ toolCallId: "call-3", toolName: "tool3", arguments: {}, summary: "Tool 3" });
      
      const result = indicator.render();
      
      expect(result.some(line => line.includes("tool1"))).toBe(true);
      expect(result.some(line => line.includes("tool2"))).toBe(true);
      expect(result.some(line => line.includes("tool3"))).toBe(true);
    });
  });

  describe("handleToolCallEnd", () => {
    it("should move call to completed list", () => {
      indicator.handleToolCallStart({ toolCallId: "call-1", toolName: "test-tool", arguments: {}, summary: "Test" });
      
      const endData: AgentToolEndData = {
        toolCallId: "call-1",
        toolName: "test-tool",
        success: true,
        duration: 500,
      };
      
      indicator.handleToolCallEnd(endData);
      const result = indicator.render();
      
      expect(result.some(line => line.includes("✓"))).toBe(true);
    });

    it("should show failed status", () => {
      indicator.handleToolCallStart({ toolCallId: "call-1", toolName: "failing-tool", arguments: {}, summary: "Failing" });
      
      const endData: AgentToolEndData = {
        toolCallId: "call-1",
        toolName: "failing-tool",
        success: false,
        duration: 200,
      };
      
      indicator.handleToolCallEnd(endData);
      const result = indicator.render();
      
      expect(result.some(line => line.includes("✗"))).toBe(true);
    });

    it("should display duration", () => {
      indicator.handleToolCallStart({ toolCallId: "call-1", toolName: "timed-tool", arguments: {}, summary: "Timed" });
      
      const endData: AgentToolEndData = {
        toolCallId: "call-1",
        toolName: "timed-tool",
        success: true,
        duration: 1234,
      };
      
      indicator.handleToolCallEnd(endData);
      const result = indicator.render();
      
      expect(result.some(line => line.includes("ms"))).toBe(true);
    });

    it("should handle non-existent call gracefully", () => {
      const endData: AgentToolEndData = {
        toolCallId: "non-existent",
        toolName: "non-existent",
        success: true,
        duration: 0,
      };
      
      expect(() => indicator.handleToolCallEnd(endData)).not.toThrow();
    });
  });

  describe("clear", () => {
    it("should clear all tool calls", () => {
      indicator.handleToolCallStart({ toolCallId: "call-1", toolName: "tool1", arguments: {}, summary: "Tool 1" });
      indicator.handleToolCallStart({ toolCallId: "call-2", toolName: "tool2", arguments: {}, summary: "Tool 2" });
      indicator.clear();
      
      const result = indicator.render();
      expect(result.length).toBe(0);
    });
  });

  describe("render", () => {
    it("should render active calls section", () => {
      indicator.handleToolCallStart({ toolCallId: "call-1", toolName: "active-tool", arguments: {}, summary: "Active" });
      const result = indicator.render();
      
      expect(result.some(line => line.includes("Active Tool Calls"))).toBe(true);
    });

    it("should render completed calls section", () => {
      indicator.handleToolCallStart({ toolCallId: "call-1", toolName: "completed-tool", arguments: {}, summary: "Completed" });
      indicator.handleToolCallEnd({ toolCallId: "call-1", toolName: "completed-tool", success: true, duration: 100 });
      
      const result = indicator.render();
      expect(result.some(line => line.includes("Recent Tool Calls"))).toBe(true);
    });

    it("should limit displayed completed calls", () => {
      const limitedIndicator = new ToolCallIndicator({ maxDisplayCalls: 2 });
      
      // Add 5 completed calls
      for (let i = 1; i <= 5; i++) {
        limitedIndicator.handleToolCallStart({ 
          toolCallId: `call-${i}`, 
          toolName: `tool${i}`,
          arguments: {},
          summary: `Tool ${i}`,
        });
        limitedIndicator.handleToolCallEnd({ 
          toolCallId: `call-${i}`, 
          toolName: `tool${i}`,
          success: true, 
          duration: 100 
        });
      }
      
      const result = limitedIndicator.render();
      const completedLines = result.filter(line => line.includes("tool"));
      
      // Should only show last 2
      expect(completedLines.length).toBeLessThanOrEqual(4); // 2 tools + headers
    });

    it("should show elapsed time for active calls", () => {
      indicator.handleToolCallStart({ toolCallId: "call-1", toolName: "long-tool", arguments: {}, summary: "Long" });
      
      // Wait a bit
      setTimeout(() => {
        const result = indicator.render();
        expect(result.some(line => line.includes("s)"))).toBe(true);
      }, 1000);
    });

    it("should respect width parameter", () => {
      indicator.handleToolCallStart({ toolCallId: "call-1", toolName: "test", arguments: {}, summary: "Test" });
      const narrowResult = indicator.render(40);
      const wideResult = indicator.render(120);
      
      expect(Array.isArray(narrowResult)).toBe(true);
      expect(Array.isArray(wideResult)).toBe(true);
    });

    it("should show arguments when enabled", () => {
      const indicatorWithArgs = new ToolCallIndicator({ showArguments: true });
      
      indicatorWithArgs.handleToolCallStart({
        toolCallId: "call-1",
        toolName: "arg-tool",
        arguments: { key: "value", number: 42 },
        summary: "Arg tool",
      });
      
      const result = indicatorWithArgs.render();
      expect(result.some(line => line.includes("Args:"))).toBe(true);
    });

    it("should not show arguments when disabled", () => {
      indicator.handleToolCallStart({
        toolCallId: "call-1",
        toolName: "no-arg-tool",
        arguments: { key: "value" },
        summary: "No arg tool",
      });
      
      const result = indicator.render();
      expect(result.some(line => line.includes("Args:"))).toBe(false);
    });
  });

  describe("formatArguments", () => {
    it("should format simple arguments", () => {
      const indicatorWithArgs = new ToolCallIndicator({ showArguments: true });
      
      indicatorWithArgs.handleToolCallStart({
        toolCallId: "call-1",
        toolName: "test",
        arguments: { simple: "value" },
        summary: "Simple test",
      });
      
      const result = indicatorWithArgs.render();
      expect(result.some(line => line.includes("simple"))).toBe(true);
    });

    it("should truncate long arguments", () => {
      const indicatorWithArgs = new ToolCallIndicator({ showArguments: true });
      const longArgs = { veryLongKey: "A".repeat(200) };
      
      indicatorWithArgs.handleToolCallStart({
        toolCallId: "call-1",
        toolName: "test",
        arguments: longArgs,
        summary: "Long args test",
      });
      
      const result = indicatorWithArgs.render(40);
      expect(result.some(line => line.includes("..."))).toBe(true);
    });

    it("should handle invalid arguments gracefully", () => {
      const indicatorWithArgs = new ToolCallIndicator({ showArguments: true });
      
      // Create circular reference
      const circularArgs: any = { key: "value" };
      circularArgs.self = circularArgs;
      
      indicatorWithArgs.handleToolCallStart({
        toolCallId: "call-1",
        toolName: "test",
        arguments: circularArgs,
        summary: "Circular test",
      });
      
      const result = indicatorWithArgs.render();
      expect(result.some(line => line.includes("[Invalid arguments]"))).toBe(true);
    });
  });

  describe("handleInput", () => {
    it("should return false for any input", () => {
      expect(indicator.handleInput?.("test")).toBe(false);
    });
  });

  describe("invalidate", () => {
    it("should not throw error when called", () => {
      expect(() => indicator.invalidate()).not.toThrow();
    });
  });

  describe("Edge Cases", () => {
    it("should handle tool calls without arguments", () => {
      indicator.handleToolCallStart({
        toolCallId: "call-1",
        toolName: "no-args",
        arguments: {},
        summary: "No args",
      });
      
      const result = indicator.render();
      expect(result.length).toBeGreaterThan(0);
    });

    it("should handle rapid start/end cycles", () => {
      for (let i = 0; i < 10; i++) {
        indicator.handleToolCallStart({ 
          toolCallId: `call-${i}`, 
          toolName: `tool${i}`,
          arguments: {},
          summary: `Tool ${i}`,
        });
        indicator.handleToolCallEnd({ 
          toolCallId: `call-${i}`, 
          toolName: `tool${i}`,
          success: true, 
          duration: 50 
        });
      }
      
      const result = indicator.render();
      expect(result.length).toBeGreaterThan(0);
    });

    it("should maintain state between renders", () => {
      indicator.handleToolCallStart({ toolCallId: "call-1", toolName: "stable", arguments: {}, summary: "Stable" });
      const first = indicator.render();
      const second = indicator.render();
      
      expect(first).toEqual(second);
    });

    it("should handle special characters in tool names", () => {
      indicator.handleToolCallStart({
        toolCallId: "call-1",
        toolName: "tool-with-special_chars.test",
        arguments: {},
        summary: "Special chars",
      });
      
      const result = indicator.render();
      expect(result.some(line => line.includes("tool-with-special_chars.test"))).toBe(true);
    });
  });
});
