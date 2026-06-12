/**
 * Unit Tests for Token Usage Tracker
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import type { LLMUsage } from "@wf-agent/types";
import { TokenUsageTracker } from "../token-usage-tracker.js";

// Mock generateId and now for deterministic tests
vi.mock("../../../../utils/id-utils.js", () => ({
  generateId: vi.fn(() => "test-request-id"),
}));

vi.mock("@wf-agent/common-utils", () => ({
  now: vi.fn(() => 1000000),
}));

describe("TokenUsageTracker", () => {
  let tracker: TokenUsageTracker;

  beforeEach(() => {
    tracker = new TokenUsageTracker({
      tokenLimit: 4000,
      enableHistory: true,
      maxHistorySize: 100,
    });
  });

  describe("constructor", () => {
    it("should use default values when no options provided", () => {
      const defaultTracker = new TokenUsageTracker();
      expect(defaultTracker).toBeDefined();
    });

    it("should use custom token limit", () => {
      const customTracker = new TokenUsageTracker({ tokenLimit: 8000 });
      expect(customTracker).toBeDefined();
    });

    it("should disable history when configured", () => {
      const noHistory = new TokenUsageTracker({ enableHistory: false });
      // Just verify no errors when calling methods with history disabled
      noHistory.updateApiUsage({
        promptTokens: 10,
        completionTokens: 20,
        totalTokens: 30,
      });
      noHistory.finalizeCurrentRequest();
      expect(noHistory.getUsageHistory()).toHaveLength(0);
    });

    it("should use custom max history size", () => {
      const smallHistory = new TokenUsageTracker({
        enableHistory: true,
        maxHistorySize: 2,
      });

      for (let i = 0; i < 5; i++) {
        smallHistory.updateApiUsage({
          promptTokens: 10,
          completionTokens: 20,
          totalTokens: 30,
        });
        smallHistory.finalizeCurrentRequest();
      }

      expect(smallHistory.getUsageHistory()).toHaveLength(2);
    });
  });

  describe("updateApiUsage and finalizeCurrentRequest", () => {
    it("should accumulate usage across multiple requests", () => {
      tracker.updateApiUsage({
        promptTokens: 10,
        completionTokens: 20,
        totalTokens: 30,
      });
      tracker.finalizeCurrentRequest();

      tracker.updateApiUsage({
        promptTokens: 5,
        completionTokens: 15,
        totalTokens: 20,
      });
      tracker.finalizeCurrentRequest();

      const cumulative = tracker.getCumulativeUsage();
      expect(cumulative?.promptTokens).toBe(15);
      expect(cumulative?.completionTokens).toBe(35);
      expect(cumulative?.totalTokens).toBe(50);
    });

    it("should not update cumulative before finalize", () => {
      tracker.updateApiUsage({
        promptTokens: 100,
        completionTokens: 200,
        totalTokens: 300,
      });

      expect(tracker.getCumulativeUsage()).toBeNull();

      tracker.finalizeCurrentRequest();
      expect(tracker.getCumulativeUsage()?.totalTokens).toBe(300);
    });

    it("should clear current request usage after finalize", () => {
      tracker.updateApiUsage({
        promptTokens: 10,
        completionTokens: 20,
        totalTokens: 30,
      });
      expect(tracker.getCurrentRequestUsage()).not.toBeNull();

      tracker.finalizeCurrentRequest();
      expect(tracker.getCurrentRequestUsage()).toBeNull();
    });

    it("should handle finalize with no current request", () => {
      // Should not throw
      tracker.finalizeCurrentRequest();
      expect(tracker.getCumulativeUsage()).toBeNull();
    });
  });

  describe("accumulateStreamUsage", () => {
    it("should set current request usage on first call", () => {
      tracker.accumulateStreamUsage({
        promptTokens: 10,
        completionTokens: 0,
        totalTokens: 10,
      });

      const current = tracker.getCurrentRequestUsage();
      expect(current?.promptTokens).toBe(10);
      expect(current?.completionTokens).toBe(0);
      expect(current?.totalTokens).toBe(10);
    });

    it("should update current request usage on subsequent calls", () => {
      tracker.accumulateStreamUsage({
        promptTokens: 10,
        completionTokens: 0,
        totalTokens: 10,
      });

      tracker.accumulateStreamUsage({
        promptTokens: 10,
        completionTokens: 15,
        totalTokens: 25,
      });

      const current = tracker.getCurrentRequestUsage();
      expect(current?.promptTokens).toBe(10);
      expect(current?.completionTokens).toBe(15);
      expect(current?.totalTokens).toBe(25);
    });

    it("should not affect cumulative usage during streaming", () => {
      tracker.accumulateStreamUsage({
        promptTokens: 10,
        completionTokens: 20,
        totalTokens: 30,
      });

      expect(tracker.getCumulativeUsage()).toBeNull();

      tracker.finalizeCurrentRequest();
      expect(tracker.getCumulativeUsage()?.totalTokens).toBe(30);
    });

    it("should accumulate streaming data correctly after multiple finalize", () => {
      // First streaming request
      tracker.accumulateStreamUsage({
        promptTokens: 10,
        completionTokens: 0,
        totalTokens: 10,
      });
      tracker.accumulateStreamUsage({
        promptTokens: 10,
        completionTokens: 20,
        totalTokens: 30,
      });
      tracker.finalizeCurrentRequest();

      // Second streaming request
      tracker.accumulateStreamUsage({
        promptTokens: 5,
        completionTokens: 0,
        totalTokens: 5,
      });
      tracker.accumulateStreamUsage({
        promptTokens: 5,
        completionTokens: 10,
        totalTokens: 15,
      });
      tracker.finalizeCurrentRequest();

      const cumulative = tracker.getCumulativeUsage();
      expect(cumulative?.totalTokens).toBe(45);
      expect(cumulative?.promptTokens).toBe(15);
      expect(cumulative?.completionTokens).toBe(30);
    });
  });

  describe("getCumulativeUsage and getTotalLifetimeUsage", () => {
    it("should return null when no usage recorded", () => {
      expect(tracker.getCumulativeUsage()).toBeNull();
      expect(tracker.getTotalLifetimeUsage()).toBeNull();
    });

    it("should return copies that don't affect internal state", () => {
      tracker.updateApiUsage({
        promptTokens: 10,
        completionTokens: 20,
        totalTokens: 30,
      });
      tracker.finalizeCurrentRequest();

      const cumulative = tracker.getCumulativeUsage()!;
      cumulative.totalTokens = 999;

      // Internal state should not be modified
      expect(tracker.getCumulativeUsage()?.totalTokens).toBe(30);
    });

    it("should track lifetime usage independently of rollback", () => {
      tracker.updateApiUsage({ promptTokens: 10, completionTokens: 20, totalTokens: 30 });
      tracker.finalizeCurrentRequest();

      tracker.updateApiUsage({ promptTokens: 5, completionTokens: 5, totalTokens: 10 });
      tracker.finalizeCurrentRequest();

      // Rollback to first request
      tracker.rollbackToRequest(1);

      expect(tracker.getCumulativeUsage()?.totalTokens).toBe(30);
      expect(tracker.getTotalLifetimeUsage()?.totalTokens).toBe(40);
    });
  });

  describe("getFullUsageStats", () => {
    it("should return null when no usage recorded", () => {
      expect(tracker.getFullUsageStats()).toBeNull();
    });

    it("should return both current and lifetime stats", () => {
      tracker.updateApiUsage({ promptTokens: 10, completionTokens: 20, totalTokens: 30 });
      tracker.finalizeCurrentRequest();

      const stats = tracker.getFullUsageStats();
      expect(stats).not.toBeNull();
      expect(stats!.current.totalTokens).toBe(30);
      expect(stats!.lifetime.totalTokens).toBe(30);
    });
  });

  describe("getCurrentRequestUsage", () => {
    it("should return null when no current request", () => {
      expect(tracker.getCurrentRequestUsage()).toBeNull();
    });

    it("should return current request usage", () => {
      tracker.updateApiUsage({ promptTokens: 1, completionTokens: 2, totalTokens: 3 });
      const usage = tracker.getCurrentRequestUsage();
      expect(usage?.totalTokens).toBe(3);
    });
  });

  describe("estimateTokens", () => {
    it("should delegate to token-utils estimateTokens", () => {
      const tokens = tracker.estimateTokens([{ role: "user", content: "test" }]);
      expect(tokens).toBeGreaterThan(0);
    });
  });

  describe("getTokenUsage", () => {
    it("should prefer cumulative usage over estimation", () => {
      tracker.updateApiUsage({ promptTokens: 100, completionTokens: 200, totalTokens: 300 });
      tracker.finalizeCurrentRequest();

      const usage = tracker.getTokenUsage([{ role: "user", content: "short" }]);
      expect(usage).toBe(300);
    });

    it("should fall back to estimation when no cumulative usage", () => {
      const usage = tracker.getTokenUsage([{ role: "user", content: "test message" }]);
      expect(usage).toBeGreaterThan(0);
    });
  });

  describe("isTokenLimitExceeded", () => {
    it("should return false when under limit", () => {
      tracker.updateApiUsage({ promptTokens: 10, completionTokens: 10, totalTokens: 20 });
      tracker.finalizeCurrentRequest();
      expect(tracker.isTokenLimitExceeded([])).toBe(false);
    });

    it("should return true when over limit", () => {
      tracker.updateApiUsage({
        promptTokens: 3000,
        completionTokens: 2000,
        totalTokens: 5000,
      });
      tracker.finalizeCurrentRequest();
      expect(tracker.isTokenLimitExceeded([])).toBe(true);
    });
  });

  describe("reset", () => {
    it("should reset cumulative usage but not lifetime usage", () => {
      tracker.updateApiUsage({ promptTokens: 10, completionTokens: 20, totalTokens: 30 });
      tracker.finalizeCurrentRequest();

      tracker.reset();

      expect(tracker.getCumulativeUsage()).toBeNull();
      expect(tracker.getTotalLifetimeUsage()?.totalTokens).toBe(30);
      expect(tracker.getUsageHistory()).toHaveLength(0);
    });
  });

  describe("fullReset", () => {
    it("should reset everything including lifetime", () => {
      tracker.updateApiUsage({ promptTokens: 10, completionTokens: 20, totalTokens: 30 });
      tracker.finalizeCurrentRequest();

      tracker.fullReset();

      expect(tracker.getCumulativeUsage()).toBeNull();
      expect(tracker.getTotalLifetimeUsage()).toBeNull();
      expect(tracker.getUsageHistory()).toHaveLength(0);
    });
  });

  describe("clone", () => {
    it("should create an independent copy", () => {
      tracker.updateApiUsage({ promptTokens: 10, completionTokens: 20, totalTokens: 30 });
      tracker.finalizeCurrentRequest();

      const cloned = tracker.clone();

      expect(cloned.getCumulativeUsage()?.totalTokens).toBe(30);
      expect(cloned.getTotalLifetimeUsage()?.totalTokens).toBe(30);

      // Modifying original should not affect clone
      tracker.updateApiUsage({ promptTokens: 5, completionTokens: 5, totalTokens: 10 });
      tracker.finalizeCurrentRequest();

      expect(cloned.getCumulativeUsage()?.totalTokens).toBe(30);
      expect(tracker.getCumulativeUsage()?.totalTokens).toBe(40);
    });

    it("should clone empty tracker", () => {
      const cloned = tracker.clone();
      expect(cloned.getCumulativeUsage()).toBeNull();
      expect(cloned.getCurrentRequestUsage()).toBeNull();
      expect(cloned.getUsageHistory()).toHaveLength(0);
    });
  });

  describe("setState and getState", () => {
    it("should restore state from checkpoint", () => {
      const state = {
        cumulativeUsage: {
          promptTokens: 100,
          completionTokens: 200,
          totalTokens: 300,
        },
      };

      tracker.setState(state.cumulativeUsage);

      expect(tracker.getCumulativeUsage()?.totalTokens).toBe(300);
      expect(tracker.getState().cumulativeUsage?.totalTokens).toBe(300);
    });

    it("should set current request usage when provided", () => {
      tracker.setState(
        { promptTokens: 100, completionTokens: 200, totalTokens: 300 },
        { promptTokens: 10, completionTokens: 20, totalTokens: 30 },
      );

      expect(tracker.getCurrentRequestUsage()?.totalTokens).toBe(30);
    });

    it("should clear current request usage when explicitly set to null", () => {
      // First set a current request
      tracker.updateApiUsage({ promptTokens: 1, completionTokens: 2, totalTokens: 3 });

      // Then clear it via setState
      tracker.setState(null, null);
      expect(tracker.getCurrentRequestUsage()).toBeNull();
    });

    it("should not modify current request when not provided", () => {
      tracker.updateApiUsage({ promptTokens: 1, completionTokens: 2, totalTokens: 3 });

      tracker.setState(
        { promptTokens: 100, completionTokens: 200, totalTokens: 300 },
        // Not provided - should keep existing
      );

      expect(tracker.getCurrentRequestUsage()?.totalTokens).toBe(3);
    });

    it("should not affect totalLifetimeUsage", () => {
      tracker.updateApiUsage({ promptTokens: 10, completionTokens: 20, totalTokens: 30 });
      tracker.finalizeCurrentRequest();

      tracker.setState({ promptTokens: 0, completionTokens: 0, totalTokens: 0 });

      expect(tracker.getTotalLifetimeUsage()?.totalTokens).toBe(30);
    });
  });

  describe("setTotalLifetimeUsage and getTotalLifetimeUsageState", () => {
    it("should set and get lifetime usage", () => {
      tracker.setTotalLifetimeUsage({
        promptTokens: 500,
        completionTokens: 500,
        totalTokens: 1000,
      });

      expect(tracker.getTotalLifetimeUsageState()?.totalTokens).toBe(1000);
    });

    it("should set lifetime to null", () => {
      tracker.setTotalLifetimeUsage(null);
      expect(tracker.getTotalLifetimeUsageState()).toBeNull();
    });
  });

  describe("getUsageHistory and getRecentHistory", () => {
    it("should return empty array when no history", () => {
      expect(tracker.getUsageHistory()).toHaveLength(0);
    });

    it("should record history entries", () => {
      tracker.updateApiUsage({
        promptTokens: 10,
        completionTokens: 20,
        totalTokens: 30,
      });
      tracker.finalizeCurrentRequest();

      const history = tracker.getUsageHistory();
      expect(history).toHaveLength(1);
      expect(history[0]!.requestId).toBe("test-request-id");
      expect(history[0]!.promptTokens).toBe(10);
      expect(history[0]!.totalTokens).toBe(30);
    });

    it("should return last N entries from getRecentHistory", () => {
      for (let i = 0; i < 5; i++) {
        tracker.updateApiUsage({
          promptTokens: 1,
          completionTokens: 2,
          totalTokens: 3,
        });
        tracker.finalizeCurrentRequest();
      }

      expect(tracker.getRecentHistory(3)).toHaveLength(3);
      expect(tracker.getRecentHistory(10)).toHaveLength(5);
    });

    it("should not include cost and model when rawUsage has no totalCost/model", () => {
      tracker.updateApiUsage({
        promptTokens: 10,
        completionTokens: 20,
        totalTokens: 30,
      });
      tracker.finalizeCurrentRequest();

      const history = tracker.getUsageHistory();
      expect(history[0]!.cost).toBeUndefined();
      expect(history[0]!.model).toBeUndefined();
    });

    it("should include cost and model from rawUsage", () => {
      const usage: LLMUsage = {
        promptTokens: 10,
        completionTokens: 20,
        totalTokens: 30,
        totalCost: 0.05,
      };
      (usage as any).model = "gpt-4";

      tracker.updateApiUsage(usage);
      tracker.finalizeCurrentRequest();

      const history = tracker.getUsageHistory();
      expect(history[0]!.cost).toBe(0.05);
      expect(history[0]!.model).toBe("gpt-4");
    });
  });

  describe("getStatistics", () => {
    it("should return zeros when no history", () => {
      const stats = tracker.getStatistics();
      expect(stats.totalRequests).toBe(0);
      expect(stats.averageTokens).toBe(0);
      expect(stats.maxTokens).toBe(0);
      expect(stats.minTokens).toBe(0);
      expect(stats.totalCost).toBe(0);
    });

    it("should calculate correct statistics from history", () => {
      tracker.updateApiUsage({ promptTokens: 10, completionTokens: 20, totalTokens: 30 });
      tracker.finalizeCurrentRequest();

      tracker.updateApiUsage({ promptTokens: 20, completionTokens: 30, totalTokens: 50 });
      tracker.finalizeCurrentRequest();

      tracker.updateApiUsage({ promptTokens: 5, completionTokens: 5, totalTokens: 10 });
      tracker.finalizeCurrentRequest();

      const stats = tracker.getStatistics();
      expect(stats.totalRequests).toBe(3);
      expect(stats.totalPromptTokens + stats.totalCompletionTokens).toBe(90);
      expect(stats.averageTokens).toBe(30);
      expect(stats.maxTokens).toBe(50);
      expect(stats.minTokens).toBe(10);
    });

    it("should calculate totalCost correctly", () => {
      const usage1: LLMUsage = {
        promptTokens: 10,
        completionTokens: 20,
        totalTokens: 30,
        totalCost: 0.01,
      };
      tracker.updateApiUsage(usage1);
      tracker.finalizeCurrentRequest();

      const usage2: LLMUsage = {
        promptTokens: 20,
        completionTokens: 30,
        totalTokens: 50,
        totalCost: 0.02,
      };
      tracker.updateApiUsage(usage2);
      tracker.finalizeCurrentRequest();

      const stats = tracker.getStatistics();
      expect(stats.totalCost).toBe(0.03);
    });
  });

  describe("rollbackToRequest", () => {
    it("should throw on invalid index", () => {
      expect(() => tracker.rollbackToRequest(-1)).toThrow("Invalid request index");
      expect(() => tracker.rollbackToRequest(1)).toThrow("Invalid request index");
    });

    it("should rollback to request index 0 (before all)", () => {
      tracker.updateApiUsage({ promptTokens: 10, completionTokens: 20, totalTokens: 30 });
      tracker.finalizeCurrentRequest();

      tracker.updateApiUsage({ promptTokens: 5, completionTokens: 5, totalTokens: 10 });
      tracker.finalizeCurrentRequest();

      tracker.rollbackToRequest(0);

      expect(tracker.getCumulativeUsage()?.totalTokens).toBe(0);
      expect(tracker.getUsageHistory()).toHaveLength(0);
    });

    it("should rollback to specific request index", () => {
      tracker.updateApiUsage({ promptTokens: 10, completionTokens: 20, totalTokens: 30 });
      tracker.finalizeCurrentRequest();

      tracker.updateApiUsage({ promptTokens: 20, completionTokens: 30, totalTokens: 50 });
      tracker.finalizeCurrentRequest();

      tracker.updateApiUsage({ promptTokens: 5, completionTokens: 5, totalTokens: 10 });
      tracker.finalizeCurrentRequest();

      // Rollback to index 2 (keep first 2, remove last)
      tracker.rollbackToRequest(2);

      expect(tracker.getCumulativeUsage()?.totalTokens).toBe(80);
      expect(tracker.getUsageHistory()).toHaveLength(2);
    });
  });

  describe("rollbackToRequestId", () => {
    it("should throw when request ID not found", () => {
      expect(() => tracker.rollbackToRequestId("non-existent")).toThrow("Request ID not found");
    });

    it("should rollback to specific request ID", () => {
      const customTracker = new TokenUsageTracker({ enableHistory: true });

      customTracker.updateApiUsage({ promptTokens: 10, completionTokens: 20, totalTokens: 30 });
      customTracker.finalizeCurrentRequest();

      customTracker.updateApiUsage({ promptTokens: 5, completionTokens: 5, totalTokens: 10 });
      customTracker.finalizeCurrentRequest();

      const history = customTracker.getUsageHistory();
      // mock generates the same ID for all calls, so both entries have same requestId.
      // rollbackToRequestId finds the first match (index 0), rolling back before all requests.
      const requestId = history[0]!.requestId;

      customTracker.rollbackToRequestId(requestId);

      expect(customTracker.getCumulativeUsage()?.totalTokens).toBe(0);
      expect(customTracker.getUsageHistory()).toHaveLength(0);
    });
  });

  describe("rollbackToTimestamp", () => {
    it("should do nothing when all records are before timestamp", () => {
      tracker.updateApiUsage({ promptTokens: 10, completionTokens: 20, totalTokens: 30 });
      tracker.finalizeCurrentRequest();

      // Timestamp is mocked to 1000000, so 2000000 is after all records
      tracker.rollbackToTimestamp(2000000);
      expect(tracker.getUsageHistory()).toHaveLength(1);
    });

    it("should rollback records after timestamp", () => {
      tracker.updateApiUsage({ promptTokens: 10, completionTokens: 20, totalTokens: 30 });
      tracker.finalizeCurrentRequest();

      tracker.updateApiUsage({ promptTokens: 5, completionTokens: 5, totalTokens: 10 });
      tracker.finalizeCurrentRequest();

      // Since now() is mocked to 1000000, all records have timestamp 1000000
      // rollbackToTimestamp(1000000) will find the first record with timestamp >= 1000000
      // and rollback to that index (which is index 0, removing all)
      tracker.rollbackToTimestamp(1000000);

      expect(tracker.getUsageHistory()).toHaveLength(0);
    });
  });

  describe("clearHistory", () => {
    it("should clear history records", () => {
      tracker.updateApiUsage({ promptTokens: 10, completionTokens: 20, totalTokens: 30 });
      tracker.finalizeCurrentRequest();

      expect(tracker.getUsageHistory()).toHaveLength(1);

      tracker.clearHistory();
      expect(tracker.getUsageHistory()).toHaveLength(0);
    });

    it("should not affect cumulative usage", () => {
      tracker.updateApiUsage({ promptTokens: 10, completionTokens: 20, totalTokens: 30 });
      tracker.finalizeCurrentRequest();

      tracker.clearHistory();

      expect(tracker.getCumulativeUsage()?.totalTokens).toBe(30);
    });
  });
});
