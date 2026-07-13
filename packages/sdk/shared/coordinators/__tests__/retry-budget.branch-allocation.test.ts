/**
 * Test Suite: RetryBudget Per-Branch Budget Allocation
 * Problem #4 - Verify fair budget distribution prevents starvation
 * Also tests pool borrowing: when a branch exhausts its allocation,
 * it can borrow from the global pool if global budget remains.
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { RetryBudget } from '../retry-budget';

describe('RetryBudget per-branch allocation (Problem #4)', () => {
  let budget: RetryBudget;

  beforeEach(() => {
    budget = new RetryBudget({
      maxRetries: 12,
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

    // Each branch gets 4 (12/3). Pool = 0 (fully allocated, no remainder).
    // branch1 tries to consume beyond its allocation
    expect(budget.canRetry(0, 'b1').allowed).toBe(true);
    budget.consumeRetry(0, 'b1');

    expect(budget.canRetry(0, 'b1').allowed).toBe(true);
    budget.consumeRetry(0, 'b1');

    expect(budget.canRetry(0, 'b1').allowed).toBe(true);
    budget.consumeRetry(0, 'b1');

    expect(budget.canRetry(0, 'b1').allowed).toBe(true);
    budget.consumeRetry(0, 'b1');

    // branch1 exhausted (4/4), and pool = 0 (12 fully allocated), so denied
    expect(budget.canRetry(0, 'b1').allowed).toBe(false);

    // But branch2 should still have budget (4 retries)
    expect(budget.canRetry(0, 'b2').allowed).toBe(true);
    budget.consumeRetry(0, 'b2');
    expect(budget.canRetry(0, 'b2').allowed).toBe(true);
    budget.consumeRetry(0, 'b2');
    expect(budget.canRetry(0, 'b2').allowed).toBe(true);
    budget.consumeRetry(0, 'b2');
    expect(budget.canRetry(0, 'b2').allowed).toBe(true);
    budget.consumeRetry(0, 'b2');
    expect(budget.canRetry(0, 'b2').allowed).toBe(false);

    // And branch3 should also have budget (4 retries)
    expect(budget.canRetry(0, 'b3').allowed).toBe(true);
  });

  it('should handle uneven division of budget', () => {
    const unevenBudget = new RetryBudget({
      maxRetries: 10,
      timeBudgetMs: 60000,
      verbose: false,
    });

    // 10 / 3 = 3.33... -> floor(3) each
    const allocated = unevenBudget.allocateBranchBudgets(['b1', 'b2', 'b3']);
    expect(allocated).toBe(3);

    // Before any consumption, retriesRemaining = totalRetries = 10
    const globalState = unevenBudget.getState();
    expect(globalState.retriesRemaining).toBe(10);
    expect(globalState.retriesConsumed).toBe(0);
  });

  it('should support zero branches gracefully', () => {
    const allocated = budget.allocateBranchBudgets([]);
    expect(allocated).toBe(0);

    const state = budget.getState();
    expect(state.totalRetries).toBe(12);
  });

  it('should skip re-allocation if branches already exist', () => {
    // First allocation: 12 / 2 = 6 per branch
    budget.allocateBranchBudgets(['b1', 'b2']);

    // Consume some from b1
    budget.consumeRetry(0, 'b1');

    // Re-allocate with more branches (b1, b2 already exist → skipped)
    // Only b3, b4 get allocated: 12 / 4 = 3 each
    budget.allocateBranchBudgets(['b1', 'b2', 'b3', 'b4']);

    // b1 keeps its original allocation (6), consumption intact
    const b1State = budget.getBranchBudgetState('b1');
    expect(b1State?.allocatedRetries).toBe(6); // From first allocation (12/2)
    expect(b1State?.retriesConsumed).toBe(1);  // Not reset

    // b3 is new, gets new allocation
    const b3State = budget.getBranchBudgetState('b3');
    expect(b3State?.allocatedRetries).toBe(3); // 12/4 = 3
    expect(b3State?.retriesConsumed).toBe(0);
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
    expect(budget.canRetry(0, 'b1').allowed).toBe(false);

    // But continue with b2 until its budget exhausted (4 retries)
    for (let i = 0; i < 4; i++) {
      budget.consumeRetry(0, 'b2');
    }

    // And b3 until its budget exhausted (4 retries)
    for (let i = 0; i < 4; i++) {
      budget.consumeRetry(0, 'b3');
    }

    // Now all global budget consumed (4+4+4=12)
    expect(budget.canRetry(0, 'b2').allowed).toBe(false);
    expect(budget.canRetry(0, 'b3').allowed).toBe(false);

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
      if (budget.canRetry(0, 'fast-fail').allowed) {
        budget.consumeRetry(0, 'fast-fail');
      }
    }

    const fastFailState = budget.getBranchBudgetState('fast-fail');
    expect(fastFailState?.retriesConsumed).toBe(4); // Allocated 4

    // But slow-fail can still use its allocation
    expect(budget.canRetry(0, 'slow-fail').allowed).toBe(true);

    // And normal can also use its allocation
    expect(budget.canRetry(0, 'normal').allowed).toBe(true);
  });

  it('should work with time budget constraints simultaneously', () => {
    const budgetWithTime = new RetryBudget({
      maxRetries: 12,
      timeBudgetMs: 30000, // 30 seconds
      verbose: false,
    });

    budgetWithTime.allocateBranchBudgets(['b1', 'b2', 'b3']);

    // Check branch budget
    expect(budgetWithTime.canRetry(1000, 'b1').allowed).toBe(true);

    // Consume with delay
    budgetWithTime.consumeRetry(5000, 'b1'); // 5s delay
    budgetWithTime.consumeRetry(5000, 'b1');
    budgetWithTime.consumeRetry(5000, 'b1');
    budgetWithTime.consumeRetry(5000, 'b1'); // 20s total delay

    // b1 exhausted on retries (4/4)
    expect(budgetWithTime.canRetry(0, 'b1').allowed).toBe(false);

    const state = budgetWithTime.getState();
    expect(state.timeBudgetConsumed).toBe(20000);
    expect(state.retriesConsumed).toBe(4);
  });

  describe('Pool borrowing (Problem #4 enhancement)', () => {
    it('should allow borrowing from global pool when branch exhausts its allocation', () => {
      const poolBudget = new RetryBudget({
        maxRetries: 10,
        timeBudgetMs: 60000,
        verbose: false,
      });

      // 10 / 3 = 3 per branch (floor) → total allocated = 9, 1 in pool
      poolBudget.allocateBranchBudgets(['b1', 'b2', 'b3']);

      // Consume all 3 of b1's allocation
      poolBudget.consumeRetry(0, 'b1');
      poolBudget.consumeRetry(0, 'b1');
      poolBudget.consumeRetry(0, 'b1');

      // b1 exhausted local allocation, but can borrow from pool
      expect(poolBudget.canRetry(0, 'b1').allowed).toBe(true);
      poolBudget.consumeRetry(0, 'b1');

      // Now b1 consumed 4 (3 allocated + 1 borrowed)
      const b1State = poolBudget.getBranchBudgetState('b1');
      expect(b1State?.retriesConsumed).toBe(4);

      // Global consumed: 3 (b1) + 1 (borrowed) = 4
      expect(poolBudget.getState().retriesConsumed).toBe(4);
    });

    it('should deny borrowing when global pool also exhausted', () => {
      const poolBudget = new RetryBudget({
        maxRetries: 4,
        timeBudgetMs: 60000,
        verbose: false,
      });

      // 4 / 3 = 1 per branch (floor) → total = 3, 1 in pool
      poolBudget.allocateBranchBudgets(['b1', 'b2', 'b3']);

      // b1 uses its 1 allocation + borrows 1 from pool = 2
      poolBudget.consumeRetry(0, 'b1');
      expect(poolBudget.canRetry(0, 'b1').allowed).toBe(true); // borrow
      poolBudget.consumeRetry(0, 'b1');

      // b2 uses its 1 allocation
      poolBudget.consumeRetry(0, 'b2');

      // b3 uses its 1 allocation
      poolBudget.consumeRetry(0, 'b3');

      // Total consumed: 2 + 1 + 1 = 4/4 — now truly exhausted
      expect(poolBudget.canRetry(0, 'b1').allowed).toBe(false);
      expect(poolBudget.canRetry(0, 'b2').allowed).toBe(false);
      expect(poolBudget.canRetry(0, 'b3').allowed).toBe(false);
      expect(poolBudget.isExhausted()).toBe(true);
    });

    it('should handle all branches borrowing from pool', () => {
      const poolBudget = new RetryBudget({
        maxRetries: 6,
        timeBudgetMs: 60000,
        verbose: false,
      });

      // 6 / 3 = 2 per branch → fully allocated (no pool leftover)
      poolBudget.allocateBranchBudgets(['b1', 'b2', 'b3']);

      // Each branch uses its 2
      poolBudget.consumeRetry(0, 'b1');
      poolBudget.consumeRetry(0, 'b1');
      poolBudget.consumeRetry(0, 'b2');
      poolBudget.consumeRetry(0, 'b2');
      poolBudget.consumeRetry(0, 'b3');
      poolBudget.consumeRetry(0, 'b3');

      // All branches should be denied (no pool — all allocated)
      expect(poolBudget.canRetry(0, 'b1').allowed).toBe(false);
      expect(poolBudget.canRetry(0, 'b2').allowed).toBe(false);
      expect(poolBudget.canRetry(0, 'b3').allowed).toBe(false);
    });

    it('should borrow correctly with uneven division', () => {
      const poolBudget = new RetryBudget({
        maxRetries: 7,
        timeBudgetMs: 60000,
        verbose: false,
      });

      // 7 / 3 = 2 per branch (floor) → total = 6, 1 in pool
      poolBudget.allocateBranchBudgets(['a', 'b', 'c']);

      // a: 2 allocated + can borrow 1 from pool = 3 max
      poolBudget.consumeRetry(0, 'a');
      poolBudget.consumeRetry(0, 'a');
      expect(poolBudget.canRetry(0, 'a').allowed).toBe(true); // borrow
      poolBudget.consumeRetry(0, 'a');

      // b: 2 allocated + can borrow? No, pool gone
      poolBudget.consumeRetry(0, 'b');
      poolBudget.consumeRetry(0, 'b');
      expect(poolBudget.canRetry(0, 'b').allowed).toBe(false);

      // c: 2 allocated + can borrow? No, pool gone
      poolBudget.consumeRetry(0, 'c');
      poolBudget.consumeRetry(0, 'c');
      expect(poolBudget.canRetry(0, 'c').allowed).toBe(false);
    });
  });
});
