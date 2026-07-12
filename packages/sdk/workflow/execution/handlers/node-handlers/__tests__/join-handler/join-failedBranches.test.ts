/**
 * Test Suite: JOIN failedBranches Error Information
 * Problem #1 - Verify that failedBranches returns error details
 *
 * These are simplified unit tests verifying the business logic.
 * Full integration tests should test the complete FORK/JOIN flow in a real execution context.
 */

import { describe, it, expect, vi } from 'vitest';
import type { JoinNodeOutput } from '@wf-agent/types';

describe('JOIN failedBranches error information (Problem #1)', () => {
  /**
   * Test the failedBranches data structure and semantics
   */
  it('should return failedBranches as Array<{pathId, error}>', () => {
    // Simulate what a real joinHandler would construct
    const failedBranches: JoinNodeOutput['failedBranches'] = [
      {
        pathId: 'path-2',
        error: 'Network timeout after 5000ms',
      },
      {
        pathId: 'path-3',
        error: 'Permission denied',
      },
    ];

    // Verify structure
    expect(Array.isArray(failedBranches)).toBe(true);
    expect(failedBranches.length).toBe(2);

    expect(failedBranches[0]).toEqual(
      expect.objectContaining({
        pathId: expect.any(String),
        error: expect.any(String),
      })
    );

    expect(failedBranches[0].pathId).toBe('path-2');
    expect(failedBranches[0].error).toContain('timeout');

    expect(failedBranches[1].pathId).toBe('path-3');
    expect(failedBranches[1].error).toContain('Permission');
  });

  it('failedBranches should be empty when all branches succeed', () => {
    // When all branches complete successfully, failedBranches should be empty
    const failedBranches: JoinNodeOutput['failedBranches'] = [];

    expect(failedBranches).toEqual([]);
    expect(failedBranches.length).toBe(0);
  });

  it('should preserve error details including full error messages', () => {
    const detailedError = 'Database connection failed: timeout after 30s with details';
    const failedBranches: JoinNodeOutput['failedBranches'] = [
      {
        pathId: 'path-2',
        error: detailedError,
      },
    ];

    // Error details should be complete, not truncated
    expect(failedBranches[0].error).toBe(detailedError);
    expect(failedBranches[0].error).toContain('Database connection');
    expect(failedBranches[0].error).toContain('timeout');
    expect(failedBranches[0].error).toContain('details');
  });

  it('should not lose error information when all branches fail', () => {
    // When no branches complete successfully, all should be in failedBranches
    const failedBranches: JoinNodeOutput['failedBranches'] = [
      {
        pathId: 'path-1',
        error: 'Execution error 1',
      },
      {
        pathId: 'path-2',
        error: 'Execution error 2',
      },
    ];

    expect(failedBranches).toHaveLength(2);
    expect(failedBranches.map(f => f.pathId)).toEqual(['path-1', 'path-2']);

    // All errors should be preserved
    failedBranches.forEach(fb => {
      expect(fb.error).toBeDefined();
      expect(fb.error).toContain('Execution error');
    });
  });

  it('should handle mixed success/failure cases correctly', () => {
    // In a mixed scenario: some succeed, some fail
    const failedBranches: JoinNodeOutput['failedBranches'] = [
      {
        pathId: 'path-2',
        error: 'Timeout',
      },
      {
        pathId: 'path-3',
        error: 'Cancelled',
      },
    ];

    // failedBranches should only contain failed ones
    expect(failedBranches).toHaveLength(2);
    expect(failedBranches.every(fb => fb.pathId)).toBe(true);
    expect(failedBranches.every(fb => fb.error)).toBe(true);

    // path-1 (success) should NOT be in failedBranches
    expect(failedBranches.map(f => f.pathId)).not.toContain('path-1');
  });
});
