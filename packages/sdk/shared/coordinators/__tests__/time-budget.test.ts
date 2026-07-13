/**
 * TimeBudget Unit Tests
 *
 * Tests for the Time Budget management system supporting delay-only and total-time modes.
 */

import { describe, it, expect, beforeEach } from "vitest";
import { TimeBudget, createTimeBudget } from "../time-budget.js";

describe("TimeBudget", () => {
  let budget: TimeBudget;

  beforeEach(() => {
    budget = new TimeBudget({
      totalBudgetMs: 10000,
      mode: "delay-only",
      name: "test-budget",
    });
  });

  describe("delay-only mode", () => {
    it("should allow delay consumption within budget", () => {
      const check = budget.canConsumeDelay(2000);
      expect(check.allowed).toBe(true);
      expect(check.remaining).toBe(8000);
    });

    it("should reject delay consumption exceeding budget", () => {
      budget.consumeDelay(8000);
      const check = budget.canConsumeDelay(3000);
      expect(check.allowed).toBe(false);
      expect(check.remaining).toBe(2000);
    });

    it("should track cumulative delays", () => {
      budget.consumeDelay(2000);
      budget.consumeDelay(3000);
      expect(budget.getTotalTimeConsumed()).toBe(5000);
      expect(budget.getRemaining()).toBe(5000);
    });

    it("should not track execution time in delay-only mode", () => {
      budget.consumeDelay(2000);
      budget.recordExecutionTime(5000);
      // Execution time should not be counted
      expect(budget.getTotalTimeConsumed()).toBe(2000);
      expect(budget.getRemaining()).toBe(8000);
    });

    it("should return false when exhausted", () => {
      budget.consumeDelay(10000);
      expect(budget.isExhausted()).toBe(true);
      expect(budget.getRemaining()).toBe(0);
    });

    it("should track retry count", () => {
      budget.consumeDelay(1000);
      budget.consumeDelay(1000);
      budget.consumeDelay(1000);
      const stats = budget.getStats();
      expect(stats.retryCount).toBe(3);
    });
  });

  describe("total-time mode", () => {
    beforeEach(() => {
      budget = new TimeBudget({
        totalBudgetMs: 10000,
        mode: "total-time",
        name: "test-budget-totaltime",
      });
    });

    it("should track both delays and execution time", () => {
      budget.consumeDelay(2000);
      budget.recordExecutionTime(3000);
      expect(budget.getTotalTimeConsumed()).toBe(5000);
      expect(budget.getRemaining()).toBe(5000);
    });

    it("should reject delay if total (delay + execution) would exceed budget", () => {
      budget.recordExecutionTime(8000);
      const check = budget.canConsumeDelay(3000);
      expect(check.allowed).toBe(false);
    });

    it("should calculate remaining based on total time", () => {
      budget.consumeDelay(2000);
      budget.recordExecutionTime(3000);
      budget.consumeDelay(1000);
      budget.recordExecutionTime(2000);
      expect(budget.getTotalTimeConsumed()).toBe(8000);
      expect(budget.getRemaining()).toBe(2000);
    });

    it("should reject delay consumption if total would exceed", () => {
      budget.recordExecutionTime(7000);
      budget.consumeDelay(2000);
      const check = budget.canConsumeDelay(2000);
      expect(check.allowed).toBe(false);
      expect(check.remaining).toBe(1000);
    });
  });

  describe("budget statistics", () => {
    it("should provide comprehensive stats", () => {
      budget.consumeDelay(2000);
      budget.consumeDelay(1000);
      const stats = budget.getStats();

      expect(stats.mode).toBe("delay-only");
      expect(stats.totalBudgetMs).toBe(10000);
      expect(stats.totalDelayConsumedMs).toBe(3000);
      expect(stats.totalTimeConsumedMs).toBe(3000);
      expect(stats.remainingMs).toBe(7000);
      expect(stats.retryCount).toBe(2);
      expect(stats.exhausted).toBe(false);
    });

    it("should include elapsed time in stats", () => {
      const stats = budget.getStats();
      expect(stats.elapsedTimeMs).toBeGreaterThanOrEqual(0);
    });
  });

  describe("edge cases", () => {
    it("should handle zero budget", () => {
      budget = new TimeBudget({
        totalBudgetMs: 0,
        mode: "delay-only",
      });
      const check = budget.canConsumeDelay(1);
      expect(check.allowed).toBe(false);
      expect(budget.isExhausted()).toBe(true);
    });

    it("should handle exact budget match", () => {
      const check = budget.canConsumeDelay(10000);
      expect(check.allowed).toBe(true);
      expect(budget.consumeDelay(10000)).toBe(true);
      expect(budget.isExhausted()).toBe(true);
    });

    it("should reset correctly", () => {
      budget.consumeDelay(5000);
      budget.reset();
      expect(budget.getTotalTimeConsumed()).toBe(0);
      expect(budget.getRemaining()).toBe(10000);
    });
  });

  describe("factory function", () => {
    it("should create budget with default name", () => {
      const b = createTimeBudget(5000, "delay-only");
      expect(b.getRemaining()).toBe(5000);
      const stats = b.getStats();
      expect(stats.totalBudgetMs).toBe(5000);
    });

    it("should create budget with custom name", () => {
      const b = createTimeBudget(5000, "total-time", "custom-budget");
      expect(b.getStats().mode).toBe("total-time");
    });
  });

  describe("concurrent consumptions", () => {
    it("should handle sequential consumptions correctly", () => {
      expect(budget.consumeDelay(2000)).toBe(true);
      expect(budget.consumeDelay(3000)).toBe(true);
      expect(budget.consumeDelay(4000)).toBe(true);
      expect(budget.consumeDelay(2000)).toBe(false); // Exceeds budget
    });

    it("should provide consistent remaining after failed consumption", () => {
      budget.consumeDelay(9000);
      const beforeCheck = budget.getRemaining();
      budget.canConsumeDelay(2000); // This will fail but shouldn't consume
      const afterCheck = budget.getRemaining();
      expect(beforeCheck).toBe(afterCheck);
    });
  });

  describe("error messages", () => {
    it("should provide informative reason for rejection", () => {
      budget.consumeDelay(9500);
      const check = budget.canConsumeDelay(1000);
      expect(check.reason).toContain("would exceed budget");
      expect(check.reason).toContain("1000ms");
      expect(check.reason).toContain("500ms");
    });
  });
});
