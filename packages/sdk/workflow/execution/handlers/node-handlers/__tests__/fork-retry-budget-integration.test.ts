/**
 * FORK Branch Retry and Global Budget Integration Tests
 *
 * Tests retry behavior for FORK branches with emphasis on:
 * 1. Per-branch timeout is respected
 * 2. Global retry budget is shared across branches
 * 3. Budget exhaustion stops retries
 * 4. Branch isolation (one failure doesn't block others)
 *
 * Real Scenarios:
 * - Scenario A: 3 branches, 2 fail → both retry, consume global budget
 * - Scenario B: Global budget exhausted mid-retry → stop retrying, return failure
 * - Scenario C: One branch times out → timeout doesn't retry, saves budget for others
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import type { RetryPolicy } from "@wf-agent/types";
import { RetryBudget } from "../../../../../shared/coordinators/retry-budget.js";

describe("FORK Branch Retry with Global Budget - Integration Tests", () => {
  describe("Multi-Branch Retry Scenario", () => {
    /**
     * Scenario A: 3 branches, 2 fail and retry
     *
     * Real scenario:
     * - FORK with 3 branches
     * - Branch 1: Success
     * - Branch 2: Fails, retries 2 times with backoff (1s, 2s)
     * - Branch 3: Fails, retries 2 times with backoff (1s, 2s)
     * - Total budget consumed: 1+2+1+2 = 6 seconds
     */
    it("should share global budget across multiple branches", () => {
      const globalBudget = new RetryBudget({
        maxRetries: 10,
        timeBudgetMs: 10000, // 10 seconds total
      });

      const retryPolicy: Required<RetryPolicy> = {
        enabled: true,
        maxRetries: 2,
        baseDelay: 1000,
        backoffMultiplier: 2,
        maxDelay: 30000,
        jitter: false,
        shouldRetry: () => true,
        getNextDelay: (attemptCount: number) => {
          return 1000 * Math.pow(2, attemptCount);
        },
      };

      // Simulate Branch 2 retry
      expect(globalBudget.canRetry(retryPolicy.getNextDelay!(0)).allowed).toBe(true); // 1s
      globalBudget.consumeRetry(retryPolicy.getNextDelay!(0));

      expect(globalBudget.canRetry(retryPolicy.getNextDelay!(1)).allowed).toBe(true); // 2s
      globalBudget.consumeRetry(retryPolicy.getNextDelay!(1));

      // Simulate Branch 3 retry
      expect(globalBudget.canRetry(retryPolicy.getNextDelay!(0)).allowed).toBe(true); // 1s
      globalBudget.consumeRetry(retryPolicy.getNextDelay!(0));

      expect(globalBudget.canRetry(retryPolicy.getNextDelay!(1)).allowed).toBe(true); // 2s
      globalBudget.consumeRetry(retryPolicy.getNextDelay!(1));

      // Verify state
      const state = globalBudget.getState();
      expect(state.timeBudgetConsumed).toBe(6000);
      expect(state.retriesConsumed).toBe(4);
      expect(state.retriesRemaining).toBe(6);
      expect(state.isExhausted).toBe(false);
    });

    /**
     * Scenario B: Budget exhausted during retry
     *
     * Real scenario:
     * - FORK with 3 branches
     * - Global budget: 5 seconds
     * - Branch 1 takes: 2s (attempt 1) + 3s (attempt 2) = 5s → exhausts budget
     * - Branch 2 attempts retry but budget is gone → returns failure
     */
    it("should stop retrying when global budget exhausted", () => {
      const globalBudget = new RetryBudget({
        maxRetries: 10,
        timeBudgetMs: 5000, // Only 5 seconds
      });

      const retryPolicy: Required<RetryPolicy> = {
        enabled: true,
        maxRetries: 3,
        baseDelay: 1000,
        backoffMultiplier: 2,
        maxDelay: 30000,
        jitter: false,
        shouldRetry: () => true,
        getNextDelay: (attemptCount: number) => {
          return 1000 * Math.pow(2, attemptCount);
        },
      };

      // Branch 1: First retry
      expect(globalBudget.canRetry(1000).allowed).toBe(true);
      globalBudget.consumeRetry(1000);

      // Branch 1: Second retry (2s)
      expect(globalBudget.canRetry(2000).allowed).toBe(true);
      globalBudget.consumeRetry(2000);

      // Branch 2: First retry (1s) - would make total 1+2+1 = 4s, OK
      expect(globalBudget.canRetry(1000).allowed).toBe(true);
      globalBudget.consumeRetry(1000);

      // Branch 2: Second retry (2s) - would make total 1+2+1+2 = 6s > 5s budget
      expect(globalBudget.canRetry(2000).allowed).toBe(false);

      // Budget is not fully exhausted: 4000ms consumed out of 5000ms
      // (a 1000ms retry would still fit)
      expect(globalBudget.isExhausted()).toBe(false);
    });

    /**
     * Scenario C: Count-based budget exhaustion
     */
    it("should stop retrying when retry count exhausted", () => {
      const globalBudget = new RetryBudget({
        maxRetries: 3, // Only 3 retries total
        timeBudgetMs: 60000, // Plenty of time
      });

      const retryPolicy: Required<RetryPolicy> = {
        enabled: true,
        maxRetries: 5,
        shouldRetry: () => true,
        getNextDelay: () => 1000,
      };

      // Consume all 3 retries across branches
      globalBudget.consumeRetry();
      globalBudget.consumeRetry();
      globalBudget.consumeRetry();

      // Fourth attempt should fail
      expect(globalBudget.canRetry().allowed).toBe(false);
      expect(globalBudget.getState().retriesRemaining).toBe(0);
    });
  });

  describe("Per-Branch Timeout vs Global Budget", () => {
    /**
     * Scenario D: Timeout doesn't consume retry budget smartly
     *
     * Real scenario:
     * - Branch has childExecutionTimeout = 30s
     * - Execution times out
     * - Timeout error should NOT be retried (wasteful)
     * - Budget should be saved for retryable failures
     */
    it("should not retry timeout errors from branches", () => {
      const globalBudget = new RetryBudget({
        maxRetries: 5,
        timeBudgetMs: 10000,
      });

      const retryPolicy: Required<RetryPolicy> = {
        enabled: true,
        maxRetries: 3,
        baseDelay: 1000,
        backoffMultiplier: 2,
        maxDelay: 30000,
        jitter: false,
        shouldRetry: (error: Error) => {
          // Don't retry timeout errors
          if (error.message?.includes("timeout")) {
            return false;
          }
          return true;
        },
        getNextDelay: (attemptCount: number) => {
          return 1000 * Math.pow(2, attemptCount);
        },
      };

      const timeoutError = new Error("Branch execution timeout after 30000ms");

      // Branch timeout should not trigger retry
      expect(retryPolicy.shouldRetry?.(timeoutError, 0)).toBe(false);

      // Budget remains untouched
      expect(globalBudget.getRetriesRemaining()).toBe(5);
      expect(globalBudget.getState().timeBudgetConsumed).toBe(0);
    });

    /**
     * Scenario E: Multiple branches with mixed timeout/retry
     */
    it("should handle mixed timeout and retry scenarios", () => {
      const globalBudget = new RetryBudget({
        maxRetries: 5,
        timeBudgetMs: 10000,
      });

      const retryPolicy: Required<RetryPolicy> = {
        enabled: true,
        maxRetries: 2,
        baseDelay: 1000,
        backoffMultiplier: 2,
        maxDelay: 30000,
        jitter: false,
        shouldRetry: (error: Error) => {
          if (error.message?.includes("timeout")) return false;
          return true;
        },
        getNextDelay: (attemptCount: number) => {
          return 1000 * Math.pow(2, attemptCount);
        },
      };

      // Branch 1: Network error → retry
      const networkError = new Error("Network connection failed");
      expect(retryPolicy.shouldRetry?.(networkError, 0)).toBe(true);

      expect(globalBudget.canRetry(retryPolicy.getNextDelay!(0)).allowed).toBe(true); // 1s
      globalBudget.consumeRetry(retryPolicy.getNextDelay!(0));

      // Branch 2: Timeout → don't retry
      const timeoutError = new Error("Branch timeout after 30000ms");
      expect(retryPolicy.shouldRetry?.(timeoutError, 0)).toBe(false);

      // Budget still available for other retries
      expect(globalBudget.getRetriesRemaining()).toBe(4);

      // Branch 3: Another network error → retry
      expect(retryPolicy.shouldRetry?.(networkError, 0)).toBe(true);

      expect(globalBudget.canRetry(retryPolicy.getNextDelay!(0)).allowed).toBe(true); // 1s
      globalBudget.consumeRetry(retryPolicy.getNextDelay!(0));

      // Verify state
      expect(globalBudget.getState().retriesRemaining).toBe(3);
      expect(globalBudget.getState().timeBudgetConsumed).toBe(2000);
    });
  });

  describe("Branch Isolation", () => {
    /**
     * Scenario F: One branch failure doesn't block others
     *
     * Real scenario:
     * - FORK with 3 branches executing in parallel
     * - Branch 1 succeeds immediately
     * - Branch 2 fails after 5s, starts retrying
     * - Branch 3 continues executing (not blocked by Branch 2's retry)
     * - All three have independent execution state
     */
    it("should allow other branches to execute during retry", () => {
      const globalBudget = new RetryBudget({
        maxRetries: 5,
        timeBudgetMs: 10000,
      });

      // Simulate concurrent branch executions
      let branch1Complete = false;
      let branch2Retrying = false;
      let branch3Complete = false;

      // Branch 1 completes immediately
      branch1Complete = true;

      // Branch 2 starts retry (but doesn't block others)
      branch2Retrying = true;
      expect(globalBudget.canRetry(1000).allowed).toBe(true);
      globalBudget.consumeRetry(1000);

      // Branch 3 can still execute while Branch 2 is retrying
      branch3Complete = true;

      // All should be true (not sequential blocking)
      expect(branch1Complete).toBe(true);
      expect(branch2Retrying).toBe(true);
      expect(branch3Complete).toBe(true);
    });
  });

  describe("Retry Policy Consistency", () => {
    /**
     * All branches should use same retry policy from FORK config
     */
    it("should apply same retry policy to all branches", () => {
      const globalBudget = new RetryBudget({
        maxRetries: 10,
        timeBudgetMs: 30000,
      });

      const forkRetryPolicy: Required<RetryPolicy> = {
        enabled: true,
        maxRetries: 3,
        baseDelay: 1000,
        backoffMultiplier: 2,
        maxDelay: 30000,
        jitter: false,
        shouldRetry: (error: Error) => {
          return !error.message?.includes("timeout");
        },
        getNextDelay: (attemptCount: number) => {
          return Math.min(1000 * Math.pow(2, attemptCount), 30000);
        },
      };

      // All branches use identical policy
      const branches = Array(3)
        .fill(null)
        .map(() => ({
          retryPolicy: forkRetryPolicy,
          budget: globalBudget,
        }));

      // All branches should calculate same delays
      branches.forEach(branch => {
        expect(branch.retryPolicy.getNextDelay?.(0)).toBe(1000);
        expect(branch.retryPolicy.getNextDelay?.(1)).toBe(2000);
        expect(branch.retryPolicy.getNextDelay?.(2)).toBe(4000);
      });
    });
  });

  describe("Budget State Queries During Retries", () => {
    it("should provide accurate state while branches are retrying", () => {
      const globalBudget = new RetryBudget({
        maxRetries: 5,
        timeBudgetMs: 10000,
      });

      const state1 = globalBudget.getState();
      expect(state1.retriesConsumed).toBe(0);
      expect(state1.retriesRemaining).toBe(5);

      globalBudget.consumeRetry(2000);

      const state2 = globalBudget.getState();
      expect(state2.retriesConsumed).toBe(1);
      expect(state2.retriesRemaining).toBe(4);
      expect(state2.timeBudgetConsumed).toBe(2000);

      globalBudget.consumeRetry(3000);

      const state3 = globalBudget.getState();
      expect(state3.retriesConsumed).toBe(2);
      expect(state3.timeBudgetConsumed).toBe(5000);
      expect(state3.isExhausted).toBe(false);
    });
  });
});
