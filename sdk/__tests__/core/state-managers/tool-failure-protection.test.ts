/**
 * ToolFailureProtectionState Unit Tests
 */

import { describe, it, expect, beforeEach } from "vitest";
import { ToolFailureProtectionState } from "../../../core/state-managers/tool-failure-protection-state.js";
import type { ToolFailureProtectionConfig } from "../../../core/state-managers/tool-failure-protection-types.js";

describe("ToolFailureProtectionState", () => {
  let state: ToolFailureProtectionState;

  beforeEach(() => {
    state = new ToolFailureProtectionState();
  });

  describe("Initial State", () => {
    it("should start with empty failure tracking", () => {
      expect(state.isEmpty()).toBe(true);
      expect(state.size()).toBe(0);
    });

    it("should allow all tools initially", () => {
      const result = state.canExecuteTool("test_tool");
      expect(result.allowed).toBe(true);
      expect(result.failureCount).toBe(0);
    });
  });

  describe("Failure Tracking", () => {
    it("should increment failure count on consecutive failures", () => {
      state.recordFailure("test_tool", "Error 1");
      expect(state.getFailureCount("test_tool")).toBe(1);

      state.recordFailure("test_tool", "Error 2");
      expect(state.getFailureCount("test_tool")).toBe(2);

      state.recordFailure("test_tool", "Error 3");
      expect(state.getFailureCount("test_tool")).toBe(3);
    });

    it("should reset failure count on success", () => {
      state.recordFailure("test_tool", "Error 1");
      state.recordFailure("test_tool", "Error 2");
      expect(state.getFailureCount("test_tool")).toBe(2);

      state.recordSuccess("test_tool");
      expect(state.getFailureCount("test_tool")).toBe(0);
    });

    it("should track multiple tools independently", () => {
      state.recordFailure("tool_a", "Error A");
      state.recordFailure("tool_b", "Error B");
      state.recordFailure("tool_a", "Error A2");

      expect(state.getFailureCount("tool_a")).toBe(2);
      expect(state.getFailureCount("tool_b")).toBe(1);
    });

    it("should store last error message", () => {
      state.recordFailure("test_tool", "First error");
      state.recordFailure("test_tool", "Second error");

      const result = state.canExecuteTool("test_tool");
      expect(result.lastError).toBe("Second error");
    });
  });

  describe("Blocking Logic", () => {
    it("should block tool after reaching threshold (default: 3)", () => {
      state.recordFailure("test_tool", "Error 1");
      state.recordFailure("test_tool", "Error 2");
      state.recordFailure("test_tool", "Error 3");

      const result = state.canExecuteTool("test_tool");
      expect(result.allowed).toBe(false);
      expect(result.failureCount).toBe(3);
      expect(result.reason).toContain("blocked");
    });

    it("should not block tool before reaching threshold", () => {
      state.recordFailure("test_tool", "Error 1");
      state.recordFailure("test_tool", "Error 2");

      const result = state.canExecuteTool("test_tool");
      expect(result.allowed).toBe(true);
      expect(result.failureCount).toBe(2);
    });

    it("should provide remaining cooldown time when blocked", () => {
      state.recordFailure("test_tool", "Error 1");
      state.recordFailure("test_tool", "Error 2");
      state.recordFailure("test_tool", "Error 3");

      const result = state.canExecuteTool("test_tool");
      expect(result.allowed).toBe(false);
      expect(result.remainingCooldown).toBeDefined();
      expect(result.remainingCooldown!).toBeGreaterThan(0);
    });
  });

  describe("Configuration", () => {
    it("should respect custom maxConsecutiveFailures", () => {
      const customState = new ToolFailureProtectionState({
        maxConsecutiveFailures: 5,
      });

      for (let i = 0; i < 4; i++) {
        customState.recordFailure("test_tool", `Error ${i}`);
      }

      const result = customState.canExecuteTool("test_tool");
      expect(result.allowed).toBe(true);

      customState.recordFailure("test_tool", "Error 5");
      const blockedResult = customState.canExecuteTool("test_tool");
      expect(blockedResult.allowed).toBe(false);
    });

    it("should allow disabling protection", () => {
      const disabledState = new ToolFailureProtectionState({
        enabled: false,
      });

      for (let i = 0; i < 10; i++) {
        disabledState.recordFailure("test_tool", `Error ${i}`);
      }

      const result = disabledState.canExecuteTool("test_tool");
      expect(result.allowed).toBe(true);
    });

    it("should update configuration at runtime", () => {
      state.updateConfig({ maxConsecutiveFailures: 2 });

      state.recordFailure("test_tool", "Error 1");
      state.recordFailure("test_tool", "Error 2");

      const result = state.canExecuteTool("test_tool");
      expect(result.allowed).toBe(false);
    });
  });

  describe("Reset Operations", () => {
    it("should reset specific tool", () => {
      state.recordFailure("tool_a", "Error A");
      state.recordFailure("tool_b", "Error B");

      state.resetTool("tool_a");

      expect(state.getFailureCount("tool_a")).toBe(0);
      expect(state.getFailureCount("tool_b")).toBe(1);
    });

    it("should reset all tools", () => {
      state.recordFailure("tool_a", "Error A");
      state.recordFailure("tool_b", "Error B");

      state.reset();

      expect(state.isEmpty()).toBe(true);
      expect(state.getFailureCount("tool_a")).toBe(0);
      expect(state.getFailureCount("tool_b")).toBe(0);
    });
  });

  describe("Snapshot and Restoration", () => {
    it("should create and restore snapshot", () => {
      state.recordFailure("tool_a", "Error A");
      state.recordFailure("tool_b", "Error B");
      state.recordFailure("tool_a", "Error A2");

      const snapshot = state.createSnapshot();

      const newState = new ToolFailureProtectionState();
      newState.restoreFromSnapshot(snapshot);

      expect(newState.getFailureCount("tool_a")).toBe(2);
      expect(newState.getFailureCount("tool_b")).toBe(1);
    });

    it("should preserve configuration in snapshot", () => {
      state.updateConfig({
        maxConsecutiveFailures: 5,
        cooldownPeriod: 120000,
      });

      const snapshot = state.createSnapshot();

      const newState = new ToolFailureProtectionState();
      newState.restoreFromSnapshot(snapshot);

      const config = newState.getConfig();
      expect(config.maxConsecutiveFailures).toBe(5);
      expect(config.cooldownPeriod).toBe(120000);
    });

    it("should round-trip snapshot correctly", () => {
      state.recordFailure("test_tool", "Test error");
      state.recordFailure("test_tool", "Another error");

      const snapshot1 = state.createSnapshot();
      state.restoreFromSnapshot(snapshot1);
      const snapshot2 = state.createSnapshot();

      expect(snapshot1).toEqual(snapshot2);
    });
  });

  describe("Edge Cases", () => {
    it("should handle mixed success/failure pattern", () => {
      state.recordFailure("test_tool", "Error 1");
      state.recordFailure("test_tool", "Error 2");
      state.recordSuccess("test_tool"); // Reset
      state.recordFailure("test_tool", "Error 3");

      expect(state.getFailureCount("test_tool")).toBe(1);
    });

    it("should return zero for unknown tool", () => {
      expect(state.getFailureCount("unknown_tool")).toBe(0);
    });

    it("should cleanup properly", () => {
      state.recordFailure("tool_a", "Error A");
      state.recordFailure("tool_b", "Error B");

      state.cleanup();

      expect(state.isEmpty()).toBe(true);
    });
  });
});
