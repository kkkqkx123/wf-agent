/**
 * RetryBudget Unit Tests
 *
 * Tests the global retry budget management with focus on:
 * 1. Time budget calculation (only delays, not wall-clock time)
 * 2. Count budget management
 * 3. Combined count + time budget logic
 *
 * Key Scenarios:
 * - Scenario 1: Time budget should NOT include wall-clock wait time
 * - Scenario 2: Multiple concurrent consumers should correctly share budget
 * - Scenario 3: Budget exhaustion should prevent retries
 */

import { describe, it, expect, beforeEach } from "vitest";
import { RetryBudget } from "../retry-budget.js";

describe("RetryBudget - Unit Tests", () => {
  describe("Time Budget Calculation (Fixed: Only Delays, Not Wall-Clock Time)", () => {
    /**
     * Scenario 1: Wall-clock waiting should NOT consume time budget
     *
     * Real scenario: Network delay between retry decisions shouldn't count against budget
     * Budget should only track ACTIVE retry delays (waiting before next attempt)
     */
    it("should only count retry delays, not wall-clock time", async () => {
      const budget = new RetryBudget({
        totalRetries: 10,
        timeBudgetMs: 5000, // 5 seconds of retry delays allowed
      });

      // Wait 2 seconds doing nothing
      await new Promise(resolve => setTimeout(resolve, 2000));

      // Now try to retry with 1000ms delay
      // Old bug: would calculate (2000 + 0 + 1000) = 3000 < 5000 ✓
      // Fixed: should calculate (0 + 1000) = 1000 < 5000 ✓
      expect(budget.canRetry(1000)).toBe(true);
      budget.consumeRetry(1000);

      // Wait another 2 seconds
      await new Promise(resolve => setTimeout(resolve, 2000));

      // Try another 3500ms delay
      // Old bug: would calculate (4000 + 1000 + 3500) = 8500 >= 5000 ✗ REJECT (WRONG)
      // Fixed: should calculate (1000 + 3500) = 4500 < 5000 ✓ ACCEPT (RIGHT)
      expect(budget.canRetry(3500)).toBe(true);
      budget.consumeRetry(3500);

      // Remaining budget should be 5000 - 1000 - 3500 = 500ms
      const state = budget.getState();
      expect(state.timeBudgetConsumed).toBe(4500);
      expect(state.isExhausted).toBe(false);

      // This final 1000ms should exceed
      expect(budget.canRetry(1000)).toBe(false);
    });

    /**
     * Scenario 2: Time budget should be independent of execution speed
     *
     * Real scenario: Slow network shouldn't artificially consume time budget
     */
    it("should allow same retries regardless of execution time between attempts", async () => {
      const timeBudgetMs = 2000;

      // Budget A: Execute quickly
      const budgetA = new RetryBudget({
        totalRetries: 10,
        timeBudgetMs,
      });

      expect(budgetA.canRetry(500)).toBe(true);
      budgetA.consumeRetry(500);

      // Budget B: Execute slowly (long wait between decisions)
      const budgetB = new RetryBudget({
        totalRetries: 10,
        timeBudgetMs,
      });

      await new Promise(resolve => setTimeout(resolve, 3000)); // Wait 3 seconds!

      // Both should allow 500ms delay equally
      expect(budgetB.canRetry(500)).toBe(true);
      budgetB.consumeRetry(500);

      // Both remaining states should be identical (2000 - 500 = 1500)
      expect(budgetA.getState().timeBudgetConsumed).toBe(500);
      expect(budgetB.getState().timeBudgetConsumed).toBe(500);
    });
  });

  describe("Count Budget Management", () => {
    /**
     * Scenario 3: Simple count-based budget
     */
    it("should enforce retry count limit", () => {
      const budget = new RetryBudget({
        totalRetries: 3,
      });

      expect(budget.canRetry()).toBe(true);
      budget.consumeRetry();
      expect(budget.getRetriesRemaining()).toBe(2);

      expect(budget.canRetry()).toBe(true);
      budget.consumeRetry();
      expect(budget.getRetriesRemaining()).toBe(1);

      expect(budget.canRetry()).toBe(true);
      budget.consumeRetry();
      expect(budget.getRetriesRemaining()).toBe(0);

      // Should be exhausted now
      expect(budget.canRetry()).toBe(false);
      expect(budget.isExhausted()).toBe(true);
    });

    /**
     * Scenario 4: Multiple retries within same retry attempt
     *
     * Real scenario: One failed operation might have multiple retry delays
     */
    it("should track multiple delays from same consumer", () => {
      const budget = new RetryBudget({
        totalRetries: 5,
        timeBudgetMs: 10000,
      });

      // First retry: exponential backoff
      // Attempt 1: delay = 1s
      expect(budget.canRetry(1000)).toBe(true);
      budget.consumeRetry(1000);

      // Attempt 2: delay = 2s
      expect(budget.canRetry(2000)).toBe(true);
      budget.consumeRetry(2000);

      // Attempt 3: delay = 4s
      expect(budget.canRetry(4000)).toBe(true);
      budget.consumeRetry(4000);

      // Total delays: 1000 + 2000 + 4000 = 7000ms
      expect(budget.getState().timeBudgetConsumed).toBe(7000);
      expect(budget.getState().retriesRemaining).toBe(2);

      // Attempt 4: delay = 8s (would exceed 10s total)
      // canRetry should reject, but getState.isExhausted is based on consumed >= budget
      // So isExhausted will only be true after we hit the limit, not before
      expect(budget.canRetry(8000)).toBe(false);

      // At this point, isExhausted should reflect that we can't make another retry
      // Since next minimum delay (even 0ms) would succeed, mark as "stuck but not consumed"
      // Only mark as truly exhausted if consumed >= budgetMs
      expect(budget.getState().timeBudgetConsumed).toBe(7000); // Still only 7s of 10s consumed
    });

  });

  describe("Combined Count + Time Budget", () => {
    /**
     * Scenario 5: Budget exhaustion can come from either count OR time
     *
     * Real scenario: Workflow might hit count limit OR time limit first
     */
    it("should respect whichever limit is hit first (count)", () => {
      const budget = new RetryBudget({
        totalRetries: 2,
        timeBudgetMs: 10000, // Plenty of time
      });

      expect(budget.canRetry(1000)).toBe(true);
      budget.consumeRetry(1000);

      expect(budget.canRetry(1000)).toBe(true);
      budget.consumeRetry(1000);

      // Hit count limit before time limit
      expect(budget.canRetry(1000)).toBe(false);
      expect(budget.getState().retriesConsumed).toBe(2);
      expect(budget.getState().timeBudgetConsumed).toBe(2000); // Only 2s of 10s
    });

    /**
     * Scenario 6: Time limit is hit before count limit
     */
    it("should respect whichever limit is hit first (time)", () => {
      const budget = new RetryBudget({
        totalRetries: 10, // Plenty of attempts
        timeBudgetMs: 3000, // Limited time
      });

      expect(budget.canRetry(1000)).toBe(true);
      budget.consumeRetry(1000);

      expect(budget.canRetry(1000)).toBe(true);
      budget.consumeRetry(1000);

      // At this point: 2000ms consumed of 3000ms budget
      // Next 1000ms attempt: would make 3000ms total, which equals budget
      // 2000 + 1000 > 3000? No, 3000 == 3000, so should allow
      expect(budget.canRetry(1000)).toBe(true);
      budget.consumeRetry(1000);

      // Now at exactly 3000ms, next attempt would exceed
      expect(budget.canRetry(1000)).toBe(false);

      expect(budget.getState().retriesConsumed).toBe(3);
      expect(budget.getState().timeBudgetConsumed).toBe(3000); // Exactly 3s of 3s
      // isExhausted is true if consumed >= budget
      expect(budget.getState().isExhausted).toBe(true);
    });
  });

  describe("Budget State Queries", () => {
    it("should provide accurate state snapshots", () => {
      const budget = new RetryBudget({
        totalRetries: 5,
        timeBudgetMs: 10000,
      });

      budget.consumeRetry(2000);
      budget.consumeRetry(3000);

      const state = budget.getState();
      expect(state).toEqual({
        totalRetries: 5,
        retriesConsumed: 2,
        retriesRemaining: 3,
        timeBudgetMs: 10000,
        timeBudgetConsumed: 5000,
        isExhausted: false,
      });
    });

    it("should mark as exhausted when count limit reached", () => {
      const budget = new RetryBudget({
        totalRetries: 1,
      });

      budget.consumeRetry();
      expect(budget.isExhausted()).toBe(true);
    });

    it("should mark as exhausted when time limit reached", () => {
      const budget = new RetryBudget({
        totalRetries: 10,
        timeBudgetMs: 2000,
      });

      // Consume exactly the time budget
      budget.consumeRetry(2000);

      // Now consumed == budget, should be marked exhausted
      expect(budget.isExhausted()).toBe(true);
    });
  });

  describe("Edge Cases", () => {
    it("should reject negative totalRetries", () => {
      expect(() => {
        new RetryBudget({
          totalRetries: -1,
        });
      }).toThrow("totalRetries must be >= 0");
    });

    it("should reject negative timeBudgetMs", () => {
      expect(() => {
        new RetryBudget({
          totalRetries: 5,
          timeBudgetMs: -1,
        });
      }).toThrow("timeBudgetMs must be >= 0");
    });

    it("should handle zero budget", () => {
      const budget = new RetryBudget({
        totalRetries: 0,
        timeBudgetMs: 0,
      });

      expect(budget.canRetry()).toBe(false);
      expect(budget.isExhausted()).toBe(true);
    });

    it("should handle unlimited time budget (timeBudgetMs = 0)", () => {
      const budget = new RetryBudget({
        totalRetries: 5,
        timeBudgetMs: 0, // 0 means unlimited
      });

      // Should only be limited by count, not time
      expect(budget.canRetry(10000)).toBe(true);
      budget.consumeRetry(10000);

      expect(budget.canRetry(10000)).toBe(true);
      budget.consumeRetry(10000);

      // ... can keep going up to 5 retries
      expect(budget.getState().retriesRemaining).toBe(3);
    });
  });

  describe("Reset Functionality", () => {
    it("should reset budget to initial state", () => {
      const budget = new RetryBudget({
        totalRetries: 5,
        timeBudgetMs: 10000,
      });

      budget.consumeRetry(2000);
      budget.consumeRetry(3000);

      expect(budget.getState().timeBudgetConsumed).toBe(5000);
      expect(budget.getState().retriesConsumed).toBe(2);

      budget.reset();

      const state = budget.getState();
      expect(state.timeBudgetConsumed).toBe(0);
      expect(state.retriesConsumed).toBe(0);
      expect(state.isExhausted).toBe(false);
    });
  });
});
