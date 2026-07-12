/**
 * Test Suite: RetryBudget Per-Branch Budget Allocation
 * Problem #4 - Verify fair budget distribution prevents starvation
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { RetryBudget } from '../retry-budget';

describe('RetryBudget per-branch allocation (Problem #4)', () => {
  let budget: RetryBudget;

  beforeEach(() => {
    budget = new RetryBudget({
      totalRetries: 12,
      timeBudgetMs: 60000,
      verbose: false,
    });
  });

  it('should allocate budgets equally among branches', () => {
    const allocatedPerBranch = budget.allocateBranchBudgets([
      'branch1',
      'branch2',
      'branch3',
      'branch4',
    ]);

    // 12 / 4 = 3 retries per branch
    expect(allocatedPerBranch).toBe(3);

    // Verify each branch has correct allocation
    for (let i = 1; i <= 4; i++) {
      const branchId = `branch${i}`;
      const state = budget.getBranchBudgetState(branchId);
      expect(state).toEqual({
        branchId,
        allocatedRetries: 3,
        retriesConsumed: 0,
        retriesRemaining: 3,
      });
    }
  });

  it('should prevent single branch from consuming all budget', () => {
    budget.allocateBranchBudgets(['b1', 'b2', 'b3']);

    // branch1 tries to consume beyond its allocation (4 retries: 12/3=4)
    expect(budget.canRetry(0, 'b1')).toBe(true);
    budget.consumeRetry(0, 'b1');

    expect(budget.canRetry(0, 'b1')).toBe(true);
    budget.consumeRetry(0, 'b1');

    expect(budget.canRetry(0, 'b1')).toBe(true);
    budget.consumeRetry(0, 'b1');

    expect(budget.canRetry(0, 'b1')).toBe(true);
    budget.consumeRetry(0, 'b1');

    // branch1 should be exhausted now (4/4)
    expect(budget.canRetry(0, 'b1')).toBe(false);

    // But branch2 should still have budget (4 retries)
    expect(budget.canRetry(0, 'b2')).toBe(true);
    budget.consumeRetry(0, 'b2');
    expect(budget.canRetry(0, 'b2')).toBe(true);
    budget.consumeRetry(0, 'b2');
    expect(budget.canRetry(0, 'b2')).toBe(true);
    budget.consumeRetry(0, 'b2');
    expect(budget.canRetry(0, 'b2')).toBe(true);
    budget.consumeRetry(0, 'b2');
    expect(budget.canRetry(0, 'b2')).toBe(false);

    // And branch3 should also have budget (4 retries)
    expect(budget.canRetry(0, 'b3')).toBe(true);
  });

  it('should handle uneven division of budget', () => {
    // Test with 10 retries (not 12 from beforeEach)
    const unevenBudget = new RetryBudget({
      totalRetries: 10,
      timeBudgetMs: 60000,
      verbose: false,
    });

    // 10 / 3 = 3.33... -> floor(3) each
    const allocated = unevenBudget.allocateBranchBudgets(['b1', 'b2', 'b3']);
    expect(allocated).toBe(3);

    // Total allocated: 3 * 3 = 9 (1 unallocated, conservative approach)
    const globalState = unevenBudget.getState();
    // Before any consumption, retriesRemaining = totalRetries - 0 = 10
    // (Allocation is internal planning, not consumption)
    expect(globalState.retriesRemaining).toBe(10);
    expect(globalState.retriesConsumed).toBe(0);
  });

  it('should support zero branches gracefully', () => {
    const allocated = budget.allocateBranchBudgets([]);
    expect(allocated).toBe(0);

    // Global budget unaffected
    const state = budget.getState();
    expect(state.totalRetries).toBe(12);
  });

  it('should not allocate if branches already exist', () => {
    // First allocation
    budget.allocateBranchBudgets(['b1', 'b2']);

    // Consume some
    budget.consumeRetry(0, 'b1');

    // Re-allocate (overwrites previous)
    budget.allocateBranchBudgets(['b1', 'b2', 'b3', 'b4']);

    // New allocation: 12 / 4 = 3
    const state = budget.getBranchBudgetState('b1');
    expect(state?.allocatedRetries).toBe(3);
    expect(state?.retriesConsumed).toBe(0); // Reset
  });

  it('should return null for non-allocated branch', () => {
    budget.allocateBranchBudgets(['b1', 'b2']);

    const state = budget.getBranchBudgetState('b3'); // Not allocated
    expect(state).toBeNull();
  });

  it('should check both global and per-branch budgets', () => {
    budget.allocateBranchBudgets(['b1', 'b2', 'b3']);
    // Each branch gets: 12 / 3 = 4 retries

    // Consume all of b1's budget (4 retries)
    budget.consumeRetry(0, 'b1');
    budget.consumeRetry(0, 'b1');
    budget.consumeRetry(0, 'b1');
    budget.consumeRetry(0, 'b1');

    // b1 is exhausted (4/4)
    expect(budget.canRetry(0, 'b1')).toBe(false);

    // But continue with b2 until its budget exhausted (4 retries)
    for (let i = 0; i < 4; i++) {
      budget.consumeRetry(0, 'b2');
    }

    // And b3 until its budget exhausted (4 retries)
    for (let i = 0; i < 4; i++) {
      budget.consumeRetry(0, 'b3');
    }

    // Now all global budget consumed (4+4+4=12)
    expect(budget.canRetry(0, 'b2')).toBe(false);
    expect(budget.canRetry(0, 'b3')).toBe(false);

    // Global state should show everything exhausted
    const globalState = budget.getState();
    expect(globalState.isExhausted).toBe(true);
  });

  it('should support per-branch consumption tracking', () => {
    budget.allocateBranchBudgets(['b1', 'b2']);

    // b1 consumes 2
    budget.consumeRetry(0, 'b1');
    budget.consumeRetry(0, 'b1');

    // b2 consumes 1
    budget.consumeRetry(0, 'b2');

    // Verify per-branch state
    const b1State = budget.getBranchBudgetState('b1');
    expect(b1State?.retriesConsumed).toBe(2);
    expect(b1State?.retriesRemaining).toBe(4); // 6 allocated, 2 consumed

    const b2State = budget.getBranchBudgetState('b2');
    expect(b2State?.retriesConsumed).toBe(1);
    expect(b2State?.retriesRemaining).toBe(5); // 6 allocated, 1 consumed

    // Global state
    const globalState = budget.getState();
    expect(globalState.retriesConsumed).toBe(3);
    expect(globalState.retriesRemaining).toBe(9);
  });

  it('should enforce per-branch limit independently', () => {
    budget.allocateBranchBudgets(['fast-fail', 'slow-fail', 'normal']);

    // fast-fail exhausts its budget quickly
    for (let i = 0; i < 4; i++) {
      // Try to exceed allocation
      if (budget.canRetry(0, 'fast-fail')) {
        budget.consumeRetry(0, 'fast-fail');
      }
    }

    const fastFailState = budget.getBranchBudgetState('fast-fail');
    expect(fastFailState?.retriesConsumed).toBe(4); // Allocated 4

    // But slow-fail can still use its allocation
    expect(budget.canRetry(0, 'slow-fail')).toBe(true);

    // And normal can also use its allocation
    expect(budget.canRetry(0, 'normal')).toBe(true);
  });

  it('should work with time budget constraints simultaneously', () => {
    const budgetWithTime = new RetryBudget({
      totalRetries: 12,
      timeBudgetMs: 30000, // 30 seconds
      verbose: false,
    });

    budgetWithTime.allocateBranchBudgets(['b1', 'b2', 'b3']);

    // Check branch budget
    expect(budgetWithTime.canRetry(1000, 'b1')).toBe(true);

    // Consume with delay
    budgetWithTime.consumeRetry(5000, 'b1'); // 5s delay
    budgetWithTime.consumeRetry(5000, 'b1');
    budgetWithTime.consumeRetry(5000, 'b1');
    budgetWithTime.consumeRetry(5000, 'b1'); // 20s total delay

    // b1 exhausted on retries (4/4)
    expect(budgetWithTime.canRetry(0, 'b1')).toBe(false);

    // But also time budget approaching
    const state = budgetWithTime.getState();
    expect(state.timeBudgetConsumed).toBe(20000);
    expect(state.retriesConsumed).toBe(4);
  });
});
