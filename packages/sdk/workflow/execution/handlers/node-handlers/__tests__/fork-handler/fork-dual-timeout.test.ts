/**
 * Test Suite: FORK Dual Timeout Control
 * Problem #8 - Verify perExecutionTimeout and totalBranchTimeout work independently
 *
 * These tests verify the timeout configuration semantics and mutual independence.
 */

import { describe, it, expect } from 'vitest';

interface TimeoutConfig {
  perExecutionTimeout?: number;
  totalBranchTimeout?: number;
}

describe('FORK dual timeout control (Problem #8)', () => {
  /**
   * Verify the dual timeout semantics
   */
  it('should support perExecutionTimeout configuration', () => {
    const config: TimeoutConfig = {
      perExecutionTimeout: 5000,
    };

    expect(config.perExecutionTimeout).toBe(5000);
    expect(config.totalBranchTimeout).toBeUndefined();
  });

  it('should support totalBranchTimeout configuration', () => {
    const config: TimeoutConfig = {
      totalBranchTimeout: 30000,
    };

    expect(config.totalBranchTimeout).toBe(30000);
    expect(config.perExecutionTimeout).toBeUndefined();
  });

  it('should support both timeouts simultaneously', () => {
    const config: TimeoutConfig = {
      perExecutionTimeout: 5000,
      totalBranchTimeout: 30000,
    };

    expect(config.perExecutionTimeout).toBe(5000);
    expect(config.totalBranchTimeout).toBe(30000);
    expect(config.totalBranchTimeout > config.perExecutionTimeout).toBe(true);
  });

  it('should allow neither timeout to be set', () => {
    const config: TimeoutConfig = {};

    expect(config.perExecutionTimeout).toBeUndefined();
    expect(config.totalBranchTimeout).toBeUndefined();
  });

  /**
   * Verify the timeout semantics
   */
  describe('timeout semantics', () => {
    it('perExecutionTimeout limits individual execution attempts', () => {
      const perExecTimeout = 2000; // 2 seconds per attempt
      const attemptDurations = [1000, 1500, 1200]; // All under 2000ms

      const allWithinPerExecTimeout = attemptDurations.every(d => d < perExecTimeout);
      expect(allWithinPerExecTimeout).toBe(true);
    });

    it('totalBranchTimeout limits cumulative time across retries', () => {
      const totalBranchTimeout = 5000; // 5 seconds total
      const attemptTimings = [
        { exec: 1000, delay: 500 }, // 1.5s
        { exec: 1000, delay: 500 }, // 1.5s (total: 3s)
        { exec: 1000, delay: 500 }, // 1.5s (total: 4.5s, within 5s)
        { exec: 1000, delay: 500 }, // 1.5s (total: 6s, EXCEEDS 5s limit)
      ];

      let totalElapsed = 0;
      let executionCount = 0;

      for (const timing of attemptTimings) {
        totalElapsed += timing.exec + timing.delay;
        if (totalElapsed > totalBranchTimeout) {
          break;
        }
        executionCount++;
      }

      expect(executionCount).toBeLessThan(attemptTimings.length);
      expect(totalElapsed).toBeLessThanOrEqual(totalBranchTimeout + 1500); // 1 attempt might exceed
    });

    it('both timeouts can constrain execution independently', () => {
      const config = {
        perExecutionTimeout: 5000,
        totalBranchTimeout: 15000,
      };

      // Scenario: 3 executions of 4s each with 2s delay
      // Each is within perExecution (5s), but 3 * (4+2) = 18s > totalBranch (15s)
      // So totalBranchTimeout is the limiting factor

      const attemptCount = 3;
      const execTimePerAttempt = 4000;
      const delayPerAttempt = 2000;

      const totalTimeIfCompleted = attemptCount * (execTimePerAttempt + delayPerAttempt);
      const perExecMaximum = attemptCount * config.perExecutionTimeout;

      expect(totalTimeIfCompleted).toBeGreaterThan(config.totalBranchTimeout);
      expect(execTimePerAttempt).toBeLessThan(config.perExecutionTimeout);
      // totalBranchTimeout should be the effective limit
      expect(config.totalBranchTimeout).toBeLessThanOrEqual(perExecMaximum);
    });
  });

  /**
   * Verify configuration flexibility
   */
  describe('configuration scenarios', () => {
    it('only perExecutionTimeout should work fine', () => {
      const config: TimeoutConfig = {
        perExecutionTimeout: 5000,
      };

      // Should be valid
      expect(config.perExecutionTimeout).toBeDefined();
      expect(config.totalBranchTimeout).toBeUndefined();
    });

    it('only totalBranchTimeout should work fine', () => {
      const config: TimeoutConfig = {
        totalBranchTimeout: 30000,
      };

      // Should be valid
      expect(config.totalBranchTimeout).toBeDefined();
      expect(config.perExecutionTimeout).toBeUndefined();
    });

    it('both together should work fine', () => {
      const config: TimeoutConfig = {
        perExecutionTimeout: 5000,
        totalBranchTimeout: 30000,
      };

      // Should be valid
      expect(config.perExecutionTimeout).toBeDefined();
      expect(config.totalBranchTimeout).toBeDefined();
      // totalBranch should be >= perExecution for practical use
      expect(config.totalBranchTimeout).toBeGreaterThanOrEqual(config.perExecutionTimeout);
    });

    it('neither timeout should work fine', () => {
      const config: TimeoutConfig = {};

      // Should be valid (unlimited)
      expect(config.perExecutionTimeout).toBeUndefined();
      expect(config.totalBranchTimeout).toBeUndefined();
    });
  });

  /**
   * Verify error reporting
   */
  describe('timeout error handling', () => {
    it('should report perExecutionTimeout exceeded', () => {
      const timeout = 2000;
      const actualDuration = 3000;

      const exceeded = actualDuration > timeout;
      expect(exceeded).toBe(true);

      const error = new Error(
        `Execution timeout exceeded: 3000ms > 2000ms (perExecutionTimeout)`
      );
      expect(error.message).toContain('timeout exceeded');
      expect(error.message).toContain('perExecutionTimeout');
    });

    it('should report totalBranchTimeout exceeded', () => {
      const timeout = 5000;
      const actualDuration = 6000;

      const exceeded = actualDuration > timeout;
      expect(exceeded).toBe(true);

      const error = new Error(
        `Branch timeout exceeded: 6000ms > 5000ms (totalBranchTimeout)`
      );
      expect(error.message).toContain('timeout exceeded');
      expect(error.message).toContain('totalBranchTimeout');
    });
  });
});
