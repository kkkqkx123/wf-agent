import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { ToolFailureProtectionState } from "../tool-failure-protection-state.js";

describe("ToolFailureProtectionState", () => {
  let state: ToolFailureProtectionState;

  beforeEach(() => {
    vi.useFakeTimers();
    state = new ToolFailureProtectionState();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe("constructor", () => {
    it("should create with default config", () => {
      const config = state.getConfig();
      expect(config.maxConsecutiveFailures).toBe(3);
      expect(config.cooldownPeriod).toBe(60000);
      expect(config.enabled).toBe(true);
    });

    it("should create with custom config", () => {
      const customState = new ToolFailureProtectionState({
        maxConsecutiveFailures: 5,
        cooldownPeriod: 120000,
        enabled: false,
      });
      const config = customState.getConfig();
      expect(config.maxConsecutiveFailures).toBe(5);
      expect(config.cooldownPeriod).toBe(120000);
      expect(config.enabled).toBe(false);
    });

    it("should create with partial custom config", () => {
      const customState = new ToolFailureProtectionState({
        maxConsecutiveFailures: 2,
      });
      const config = customState.getConfig();
      expect(config.maxConsecutiveFailures).toBe(2);
      expect(config.cooldownPeriod).toBe(60000);
      expect(config.enabled).toBe(true);
    });
  });

  describe("size and isEmpty", () => {
    it("should be empty initially", () => {
      expect(state.size()).toBe(0);
      expect(state.isEmpty()).toBe(true);
    });

    it("should reflect tracked tools after failures", () => {
      state.recordFailure("tool-1", "error 1");
      state.recordFailure("tool-2", "error 2");
      expect(state.size()).toBe(2);
      expect(state.isEmpty()).toBe(false);
    });

    it("should update size after recording success", () => {
      state.recordFailure("tool-1", "error 1");
      expect(state.size()).toBe(1);
      state.recordSuccess("tool-1");
      expect(state.size()).toBe(0);
      expect(state.isEmpty()).toBe(true);
    });
  });

  describe("cleanup and reset", () => {
    it("should clear all tracking on cleanup", () => {
      state.recordFailure("tool-1", "error 1");
      state.recordFailure("tool-2", "error 2");
      state.cleanup();
      expect(state.size()).toBe(0);
      expect(state.isEmpty()).toBe(true);
    });

    it("should clear all tracking on reset", () => {
      state.recordFailure("tool-1", "error 1");
      state.recordFailure("tool-2", "error 2");
      state.reset();
      expect(state.size()).toBe(0);
      expect(state.isEmpty()).toBe(true);
    });
  });

  describe("canExecuteTool", () => {
    it("should allow execution for tool with no failure history", () => {
      const result = state.canExecuteTool("tool-1");
      expect(result.allowed).toBe(true);
      expect(result.failureCount).toBe(0);
      expect(result.reason).toBeUndefined();
    });

    it("should allow execution when failures are below threshold", () => {
      state.recordFailure("tool-1", "error 1");
      state.recordFailure("tool-1", "error 2");
      const result = state.canExecuteTool("tool-1");
      expect(result.allowed).toBe(true);
      expect(result.failureCount).toBe(2);
    });

    it("should block execution when failures reach threshold", () => {
      state.recordFailure("tool-1", "error 1");
      state.recordFailure("tool-1", "error 2");
      state.recordFailure("tool-1", "error 3");
      const result = state.canExecuteTool("tool-1");
      expect(result.allowed).toBe(false);
      expect(result.failureCount).toBe(3);
      expect(result.reason).toContain("blocked");
      expect(result.remainingCooldown).toBeGreaterThan(0);
    });

    it("should allow execution after cooldown period elapses", () => {
      state.recordFailure("tool-1", "error 1");
      state.recordFailure("tool-1", "error 2");
      state.recordFailure("tool-1", "error 3");

      const blockedResult = state.canExecuteTool("tool-1");
      expect(blockedResult.allowed).toBe(false);

      vi.advanceTimersByTime(60001);

      const allowedResult = state.canExecuteTool("tool-1");
      expect(allowedResult.allowed).toBe(true);
      expect(allowedResult.failureCount).toBe(3);
    });

    it("should always allow execution when protection is disabled", () => {
      const disabledState = new ToolFailureProtectionState({ enabled: false });
      disabledState.recordFailure("tool-1", "error 1");
      disabledState.recordFailure("tool-1", "error 2");
      disabledState.recordFailure("tool-1", "error 3");

      const result = disabledState.canExecuteTool("tool-1");
      expect(result.allowed).toBe(true);
      expect(result.failureCount).toBe(0);
    });

    it("should return lastError when available", () => {
      state.recordFailure("tool-1", "connection timeout");
      const result = state.canExecuteTool("tool-1");
      expect(result.lastError).toBe("connection timeout");
    });
  });

  describe("recordSuccess", () => {
    it("should reset failure count for a tool", () => {
      state.recordFailure("tool-1", "error 1");
      state.recordFailure("tool-1", "error 2");
      expect(state.getFailureCount("tool-1")).toBe(2);

      state.recordSuccess("tool-1");
      expect(state.getFailureCount("tool-1")).toBe(0);
    });

    it("should do nothing for tool with no failure history", () => {
      state.recordSuccess("tool-1");
      expect(state.getFailureCount("tool-1")).toBe(0);
    });
  });

  describe("recordFailure", () => {
    it("should increment failure count on consecutive failures", () => {
      state.recordFailure("tool-1", "error 1");
      expect(state.getFailureCount("tool-1")).toBe(1);

      state.recordFailure("tool-1", "error 2");
      expect(state.getFailureCount("tool-1")).toBe(2);

      state.recordFailure("tool-1", "error 3");
      expect(state.getFailureCount("tool-1")).toBe(3);
    });

    it("should track failures independently per tool", () => {
      state.recordFailure("tool-1", "error 1");
      state.recordFailure("tool-2", "error 1");
      state.recordFailure("tool-2", "error 2");

      expect(state.getFailureCount("tool-1")).toBe(1);
      expect(state.getFailureCount("tool-2")).toBe(2);
    });

    it("should update lastError on each failure", () => {
      state.recordFailure("tool-1", "first error");
      let result = state.canExecuteTool("tool-1");
      expect(result.lastError).toBe("first error");

      state.recordFailure("tool-1", "second error");
      result = state.canExecuteTool("tool-1");
      expect(result.lastError).toBe("second error");
    });
  });

  describe("resetTool", () => {
    it("should reset failure tracking for a specific tool", () => {
      state.recordFailure("tool-1", "error 1");
      state.recordFailure("tool-1", "error 2");
      expect(state.getFailureCount("tool-1")).toBe(2);

      state.resetTool("tool-1");
      expect(state.getFailureCount("tool-1")).toBe(0);
      expect(state.size()).toBe(0);
    });

    it("should not affect other tools", () => {
      state.recordFailure("tool-1", "error 1");
      state.recordFailure("tool-2", "error 1");
      state.resetTool("tool-1");

      expect(state.getFailureCount("tool-1")).toBe(0);
      expect(state.getFailureCount("tool-2")).toBe(1);
      expect(state.size()).toBe(1);
    });

    it("should do nothing for non-existent tool", () => {
      expect(() => state.resetTool("non-existent")).not.toThrow();
    });
  });

  describe("getFailureCount", () => {
    it("should return 0 for tool with no failures", () => {
      expect(state.getFailureCount("tool-1")).toBe(0);
    });

    it("should return correct count after failures", () => {
      state.recordFailure("tool-1", "error 1");
      state.recordFailure("tool-1", "error 2");
      expect(state.getFailureCount("tool-1")).toBe(2);
    });
  });

  describe("getConfig", () => {
    it("should return a copy of the config", () => {
      const config = state.getConfig();
      config.maxConsecutiveFailures = 10;
      const configAgain = state.getConfig();
      expect(configAgain.maxConsecutiveFailures).toBe(3);
    });
  });

  describe("updateConfig", () => {
    it("should update config values", () => {
      state.updateConfig({ maxConsecutiveFailures: 5 });
      expect(state.getConfig().maxConsecutiveFailures).toBe(5);
      expect(state.getConfig().cooldownPeriod).toBe(60000);
      expect(state.getConfig().enabled).toBe(true);
    });

    it("should affect protection behavior after update", () => {
      state.recordFailure("tool-1", "error 1");
      state.recordFailure("tool-1", "error 2");
      state.recordFailure("tool-1", "error 3");

      expect(state.canExecuteTool("tool-1").allowed).toBe(false);

      state.updateConfig({ maxConsecutiveFailures: 5 });
      expect(state.canExecuteTool("tool-1").allowed).toBe(true);
    });
  });

  describe("snapshot and restore", () => {
    it("should create a snapshot with current state", () => {
      state.recordFailure("tool-1", "error 1");
      state.recordFailure("tool-2", "error 2");
      state.recordFailure("tool-2", "error 3");

      const snapshot = state.createSnapshot();

      expect(snapshot.failureMap).toHaveLength(2);
      expect(snapshot.config.maxConsecutiveFailures).toBe(3);
      expect(snapshot.config.cooldownPeriod).toBe(60000);
      expect(snapshot.config.enabled).toBe(true);

      const tool1Info = snapshot.failureMap.find(([name]) => name === "tool-1")!;
      expect(tool1Info[1].failureCount).toBe(1);
      expect(tool1Info[1].lastError).toBe("error 1");

      const tool2Info = snapshot.failureMap.find(([name]) => name === "tool-2")!;
      expect(tool2Info[1].failureCount).toBe(2);
      expect(tool2Info[1].lastError).toBe("error 3");
    });

    it("should restore state from a snapshot", () => {
      state.recordFailure("tool-1", "error 1");
      state.recordFailure("tool-2", "error 2");
      const snapshot = state.createSnapshot();

      const newState = new ToolFailureProtectionState();
      newState.restoreFromSnapshot(snapshot);

      expect(newState.size()).toBe(2);
      expect(newState.getFailureCount("tool-1")).toBe(1);
      expect(newState.getFailureCount("tool-2")).toBe(1);
      expect(newState.getConfig().maxConsecutiveFailures).toBe(3);
    });

    it("should restore config from snapshot", () => {
      const customState = new ToolFailureProtectionState({
        maxConsecutiveFailures: 5,
        cooldownPeriod: 30000,
      });
      customState.recordFailure("tool-1", "error");
      const snapshot = customState.createSnapshot();

      const newState = new ToolFailureProtectionState();
      newState.restoreFromSnapshot(snapshot);

      expect(newState.getConfig().maxConsecutiveFailures).toBe(5);
      expect(newState.getConfig().cooldownPeriod).toBe(30000);
    });

    it("should restore blocked state correctly", () => {
      state.recordFailure("tool-1", "error 1");
      state.recordFailure("tool-1", "error 2");
      state.recordFailure("tool-1", "error 3");

      const snapshot = state.createSnapshot();

      const newState = new ToolFailureProtectionState();
      newState.restoreFromSnapshot(snapshot);

      const result = newState.canExecuteTool("tool-1");
      expect(result.allowed).toBe(false);
      expect(result.failureCount).toBe(3);
    });
  });

  describe("cooldown behavior", () => {
    it("should track remaining cooldown decreasing over time", () => {
      state.recordFailure("tool-1", "error 1");
      state.recordFailure("tool-1", "error 2");
      state.recordFailure("tool-1", "error 3");

      const result1 = state.canExecuteTool("tool-1");
      expect(result1.remainingCooldown).toBe(60000);

      vi.advanceTimersByTime(30000);

      const result2 = state.canExecuteTool("tool-1");
      expect(result2.remainingCooldown).toBe(30000);
    });

    it("should not block tools with failures below threshold even if cooldown period has not elapsed", () => {
      state.recordFailure("tool-1", "error 1");
      state.recordFailure("tool-1", "error 2");

      const result = state.canExecuteTool("tool-1");
      expect(result.allowed).toBe(true);
    });

    it("should allow immediate execution after reset while in cooldown", () => {
      state.recordFailure("tool-1", "error 1");
      state.recordFailure("tool-1", "error 2");
      state.recordFailure("tool-1", "error 3");

      expect(state.canExecuteTool("tool-1").allowed).toBe(false);

      state.resetTool("tool-1");
      expect(state.canExecuteTool("tool-1").allowed).toBe(true);
    });
  });

  describe("edge cases", () => {
    it("should handle empty tool name", () => {
      state.recordFailure("", "error");
      expect(state.getFailureCount("")).toBe(1);
    });

    it("should handle special characters in tool name", () => {
      state.recordFailure("tool@#$%", "error");
      expect(state.getFailureCount("tool@#$%")).toBe(1);
    });

    it("should handle empty error message", () => {
      state.recordFailure("tool-1", "");
      const result = state.canExecuteTool("tool-1");
      expect(result.lastError).toBe("");
    });
  });
});
