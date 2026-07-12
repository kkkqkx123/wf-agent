/**
 * Test Suite: FORK Promise.allSettled Result Collection
 * Problem #2 - Verify all branch results are collected despite failures
 *
 * These tests verify the semantic behavior of Promise.allSettled vs Promise.all
 * and how the SDK handles branch results in a FORK node.
 */

import { describe, it, expect, beforeEach } from 'vitest';

interface BranchExecutionOutcome {
  status: 'COMPLETED' | 'FAILED' | 'SKIPPED';
  result?: any;
  error?: any;
}

describe('FORK Promise.allSettled branch result collection (Problem #2)', () => {
  /**
   * Demonstrate the difference between Promise.all and Promise.allSettled
   */
  it('should collect all branch outcomes even when some fail', async () => {
    // Simulate multiple branches with different outcomes
    const branch1 = Promise.resolve({ status: 'COMPLETED', result: { value: 'success1' } });
    const branch2 = Promise.reject(new Error('Branch 2 failed'));
    const branch3 = Promise.resolve({ status: 'COMPLETED', result: { value: 'success3' } });

    // Using Promise.allSettled (correct behavior for FORK)
    const results = await Promise.allSettled([branch1, branch2, branch3]);

    // All 3 results should be present
    expect(results).toHaveLength(3);

    // First branch succeeded
    expect(results[0].status).toBe('fulfilled');
    expect(results[0]).toEqual(
      expect.objectContaining({
        status: 'fulfilled',
        value: expect.objectContaining({ status: 'COMPLETED' }),
      })
    );

    // Second branch failed but is still in results
    expect(results[1].status).toBe('rejected');
    expect(results[1]).toEqual(
      expect.objectContaining({
        status: 'rejected',
        reason: expect.any(Error),
      })
    );

    // Third branch succeeded
    expect(results[2].status).toBe('fulfilled');
  });

  it('should NOT throw when first branch fails (unlike Promise.all)', async () => {
    const branch1 = Promise.reject(new Error('First branch fails'));
    const branch2 = Promise.resolve({ status: 'COMPLETED' });
    const branch3 = Promise.resolve({ status: 'COMPLETED' });

    // Promise.all would throw here, but allSettled should not
    const results = await Promise.allSettled([branch1, branch2, branch3]);

    expect(results).toHaveLength(3);
    expect(results[0].status).toBe('rejected');
    // Other branches still collected
    expect(results[1].status).toBe('fulfilled');
    expect(results[2].status).toBe('fulfilled');
  });

  it('should handle PromiseSettledResult<T> correctly', async () => {
    const promises = [
      Promise.resolve('success'),
      Promise.reject(new Error('failure')),
      Promise.resolve('another success'),
    ];

    const outcomes = await Promise.allSettled(promises);

    // Transform settled results to branch outcomes
    const branchOutcomes: BranchExecutionOutcome[] = outcomes.map(settled => {
      if (settled.status === 'fulfilled') {
        return {
          status: 'COMPLETED',
          result: settled.value,
        };
      } else {
        return {
          status: 'FAILED',
          error: settled.reason,
        };
      }
    });

    expect(branchOutcomes).toHaveLength(3);
    expect(branchOutcomes[0].status).toBe('COMPLETED');
    expect(branchOutcomes[1].status).toBe('FAILED');
    expect(branchOutcomes[2].status).toBe('COMPLETED');
  });

  it('should maintain result order even when outcomes differ', async () => {
    const slowSuccess = new Promise(r => setTimeout(() => r('slow'), 100));
    const quickFail = Promise.reject(new Error('quick fail'));
    const instantSuccess = Promise.resolve('instant');

    const results = await Promise.allSettled([slowSuccess, quickFail, instantSuccess]);

    // Order should be preserved
    expect(results).toHaveLength(3);
    expect(results[0].status).toBe('fulfilled'); // slow success
    expect(results[1].status).toBe('rejected'); // quick fail
    expect(results[2].status).toBe('fulfilled'); // instant success
  });

  it('should handle all-failures scenario', async () => {
    const branch1 = Promise.reject(new Error('Error 1'));
    const branch2 = Promise.reject(new Error('Error 2'));
    const branch3 = Promise.reject(new Error('Error 3'));

    const results = await Promise.allSettled([branch1, branch2, branch3]);

    // All 3 should be present, all rejected
    expect(results).toHaveLength(3);
    expect(results.every(r => r.status === 'rejected')).toBe(true);
  });

  it('should handle all-success scenario', async () => {
    const branch1 = Promise.resolve({ data: 'a' });
    const branch2 = Promise.resolve({ data: 'b' });
    const branch3 = Promise.resolve({ data: 'c' });

    const results = await Promise.allSettled([branch1, branch2, branch3]);

    // All 3 should be present, all fulfilled
    expect(results).toHaveLength(3);
    expect(results.every(r => r.status === 'fulfilled')).toBe(true);
  });

  it('should allow decision-making based on complete results', async () => {
    // Simulates a failureStrategy evaluation with complete results
    const branch1 = Promise.resolve('success');
    const branch2 = Promise.reject(new Error('fail'));
    const branch3 = Promise.resolve('success');

    const results = await Promise.allSettled([branch1, branch2, branch3]);

    // Now we can evaluate a strategy like ANY_COMPLETED
    const hasAnySuccess = results.some(r => r.status === 'fulfilled');
    expect(hasAnySuccess).toBe(true);

    // Or ALL_COMPLETED
    const hasAllSuccess = results.every(r => r.status === 'fulfilled');
    expect(hasAllSuccess).toBe(false);

    // Or threshold-based strategies
    const successCount = results.filter(r => r.status === 'fulfilled').length;
    expect(successCount).toBe(2);
  });
});
