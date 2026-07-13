/**
 * Test Suite: RetryBudget Time Budget Modes
 * Problem #5 - Verify two time budget modes (delay-only vs total-time)
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { RetryBudget } from '../retry-budget';

describe('RetryBudget time budget modes (Problem #5)', () => {
  it('delay-only mode: only counts retry delays', () => {
    const budget = new RetryBudget({
      maxRetries: 10,
      timeBudgetMs: 60000,
      timeBudgetMode: 'delay-only',
      verbose: false,
    });

    // Execution: 20s, Delay: 10s
    expect(budget.canRetry(10000, 'b1', 20000).allowed).toBe(true);
    budget.consumeRetry(10000, 'b1', 20000);

    const state = budget.getState();
    // Should only count 10s (delay), not 20s (execution)
    expect(state.timeBudgetConsumed).toBe(10000);
  });

  it('total-time mode: counts delays + execution time', () => {
    const budget = new RetryBudget({
      maxRetries: 10,
      timeBudgetMs: 60000,
      timeBudgetMode: 'total-time',
      verbose: false,
    });

    // Execution: 20s, Delay: 10s
    expect(budget.canRetry(10000, 'b1', 20000).allowed).toBe(true);
    budget.consumeRetry(10000, 'b1', 20000);

    const state = budget.getState();
    // Should count both: 10s (delay) + 20s (execution) = 30s
    expect(state.timeBudgetConsumed).toBe(30000);
  });

  it('should default to delay-only mode', () => {
    const budget = new RetryBudget({
      maxRetries: 10,
      timeBudgetMs: 60000,
      // Not specifying timeBudgetMode
      verbose: false,
    });

    const state = budget.getState();
    expect(state.timeBudgetMode).toBe('delay-only');
  });

  it('delay-only: allows long execution as long as delays are limited', () => {
    const budget = new RetryBudget({
      maxRetries: 10,
      timeBudgetMs: 10000, // Only 10s delay budget
      timeBudgetMode: 'delay-only',
      verbose: false,
    });

    // 5s delay, 100s execution - should be OK in delay-only
    expect(budget.canRetry(5000, 'b1', 100000).allowed).toBe(true);
    budget.consumeRetry(5000, 'b1', 100000);

    // Still have 5s delay budget remaining
    expect(budget.canRetry(5000, 'b1', 100000).allowed).toBe(true);
    budget.consumeRetry(5000, 'b1', 100000);

    // No more delay budget
    expect(budget.canRetry(1, 'b1', 100000).allowed).toBe(false);
  });

  it('total-time: limits both delays and execution', () => {
    const budget = new RetryBudget({
      maxRetries: 10,
      timeBudgetMs: 30000, // 30s total budget
      timeBudgetMode: 'total-time',
      verbose: false,
    });

    // First attempt: 5s delay + 20s execution = 25s
    expect(budget.canRetry(5000, 'b1', 20000).allowed).toBe(true);
    budget.consumeRetry(5000, 'b1', 20000);

    // Second attempt: need 10s delay + any execution, but only 5s left
    expect(budget.canRetry(10000, 'b1', 0).allowed).toBe(false);
    expect(budget.canRetry(5000, 'b1', 0).allowed).toBe(true);
    budget.consumeRetry(5000, 'b1', 0);

    // Now exhausted
    expect(budget.canRetry(0, 'b1', 1).allowed).toBe(false);
  });

  it('should pass timeBudgetMode through getState', () => {
    const delayOnlyBudget = new RetryBudget({
      maxRetries: 10,
      timeBudgetMs: 60000,
      timeBudgetMode: 'delay-only',
      verbose: false,
    });

    const totalTimeBudget = new RetryBudget({
      maxRetries: 10,
      timeBudgetMs: 60000,
      timeBudgetMode: 'total-time',
      verbose: false,
    });

    expect(delayOnlyBudget.getState().timeBudgetMode).toBe('delay-only');
    expect(totalTimeBudget.getState().timeBudgetMode).toBe('total-time');
  });

  it('delay-only: execution time parameter should be ignored', () => {
    const budget = new RetryBudget({
      maxRetries: 10,
      timeBudgetMs: 10000,
      timeBudgetMode: 'delay-only',
      verbose: false,
    });

    // Same result regardless of execution time
    const result1 = budget.canRetry(5000, 'b1', 1000);
    const result2 = budget.canRetry(5000, 'b1', 100000);

    expect(result1.allowed).toBe(result2.allowed);
    expect(result1.allowed).toBe(true);

    budget.consumeRetry(5000, 'b1', 1000);

    const state = budget.getState();
    // Should only count 5s delay
    expect(state.timeBudgetConsumed).toBe(5000);
  });

  it('total-time: multiple attempts accumulate correctly', () => {
    const budget = new RetryBudget({
      maxRetries: 10,
      timeBudgetMs: 100000, // 100s total
      timeBudgetMode: 'total-time',
      verbose: false,
    });

    // Attempt 1: 2s delay + 10s execution = 12s
    budget.consumeRetry(2000, 'b1', 10000);

    // Attempt 2: 2s delay + 10s execution = 12s (total: 24s)
    budget.consumeRetry(2000, 'b1', 10000);

    // Attempt 3: 4s delay + 10s execution = 14s (total: 38s)
    budget.consumeRetry(4000, 'b1', 10000);

    // Attempt 4: 8s delay + 10s execution = 18s (total: 56s)
    budget.consumeRetry(8000, 'b1', 10000);

    // Attempt 5: 16s delay + 10s execution = 26s (total: 82s)
    budget.consumeRetry(16000, 'b1', 10000);

    // Attempt 6: would be 32s delay + 10s = 42s (total: 124s) - exceeds 100s
    expect(budget.canRetry(32000, 'b1', 10000).allowed).toBe(false);

    // But can still fit smaller attempt
    expect(budget.canRetry(10000, 'b1', 5000).allowed).toBe(true);

    const state = budget.getState();
    expect(state.timeBudgetConsumed).toBe(82000);
  });

  it('should reset modes correctly', () => {
    const budget = new RetryBudget({
      maxRetries: 10,
      timeBudgetMs: 60000,
      timeBudgetMode: 'total-time',
      verbose: false,
    });

    // Consume some budget
    budget.consumeRetry(5000, 'b1', 10000);
    expect(budget.getState().timeBudgetConsumed).toBe(15000);

    // Reset
    budget.reset();

    // Should be back to zero
    expect(budget.getState().timeBudgetConsumed).toBe(0);
    expect(budget.getState().timeBudgetMode).toBe('total-time'); // Mode persists
  });

  it('no time budget: both modes should allow unlimited time', () => {
    const delayOnlyBudget = new RetryBudget({
      maxRetries: 10,
      // timeBudgetMs not specified = unlimited
      timeBudgetMode: 'delay-only',
      verbose: false,
    });

    const totalTimeBudget = new RetryBudget({
      maxRetries: 10,
      // timeBudgetMs not specified = unlimited
      timeBudgetMode: 'total-time',
      verbose: false,
    });

    // Should allow any delays/execution times
    expect(delayOnlyBudget.canRetry(1000000, 'b1', 1000000).allowed).toBe(true);
    expect(totalTimeBudget.canRetry(1000000, 'b1', 1000000).allowed).toBe(true);

    delayOnlyBudget.consumeRetry(1000000, 'b1', 1000000);
    totalTimeBudget.consumeRetry(1000000, 'b1', 1000000);

    // Can keep going
    expect(delayOnlyBudget.canRetry(1000000, 'b1', 1000000).allowed).toBe(true);
    expect(totalTimeBudget.canRetry(1000000, 'b1', 1000000).allowed).toBe(true);
  });

  it('mode impacts effective retry count', () => {
    const delayOnlyBudget = new RetryBudget({
      maxRetries: 100,
      timeBudgetMs: 100000,
      timeBudgetMode: 'delay-only',
      verbose: false,
    });

    const totalTimeBudget = new RetryBudget({
      maxRetries: 100,
      timeBudgetMs: 100000,
      timeBudgetMode: 'total-time',
      verbose: false,
    });

    // In delay-only: can have many retries (limited by 100s of delays)
    let delayOnlyCount = 0;
    while (delayOnlyBudget.canRetry(10000, 'b1', 20000).allowed) {
      delayOnlyBudget.consumeRetry(10000, 'b1', 20000);
      delayOnlyCount++;
    }

    // In total-time: fewer retries (100s / (10s delay + 20s execution) = ~3)
    let totalTimeCount = 0;
    while (totalTimeBudget.canRetry(10000, 'b1', 20000).allowed) {
      totalTimeBudget.consumeRetry(10000, 'b1', 20000);
      totalTimeCount++;
    }

    // delay-only should allow many more attempts
    expect(delayOnlyCount).toBeGreaterThan(totalTimeCount);
    expect(delayOnlyCount).toBe(10); // 100s / 10s = 10 retries
    expect(totalTimeCount).toBeLessThanOrEqual(3); // ~30s / retry, so 3 attempts
  });
});
