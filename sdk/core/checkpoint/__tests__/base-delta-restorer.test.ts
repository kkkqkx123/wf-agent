/**
 * BaseDeltaRestorer Tests
 * Tests for restoration of full state from delta checkpoint chains
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { BaseDeltaRestorer } from '../base-delta-restorer.js';
import type { BaseCheckpoint } from '@wf-agent/types';

describe('BaseDeltaRestorer', () => {
  let mockLoadCheckpoint: ReturnType<typeof vi.fn>;
  let restorer: BaseDeltaRestorer<BaseCheckpoint<Record<string, { from: unknown; to: unknown }>, Record<string, unknown>>, Record<string, unknown>>;

  beforeEach(() => {
    mockLoadCheckpoint = vi.fn<(id: string) => Promise<BaseCheckpoint<Record<string, { from: unknown; to: unknown }>, Record<string, unknown>> | null>>();
    restorer = new BaseDeltaRestorer(mockLoadCheckpoint as unknown as (id: string) => Promise<BaseCheckpoint<Record<string, { from: unknown; to: unknown }>, Record<string, unknown>> | null>);
  });

  function createFullCheckpoint(id: string, snapshot: Record<string, unknown>): BaseCheckpoint<Record<string, { from: unknown; to: unknown }>, Record<string, unknown>> {
    return {
      id,
      type: 'FULL',
      snapshot,
      timestamp: Date.now(),
    };
  }

  function createDeltaCheckpoint(
    id: string,
    previousCheckpointId: string,
    baseCheckpointId: string,
    delta: Record<string, { from: unknown; to: unknown }>
  ): BaseCheckpoint<Record<string, { from: unknown; to: unknown }>, Record<string, unknown>> {
    return {
      id,
      type: 'DELTA',
      previousCheckpointId,
      baseCheckpointId,
      delta,
      timestamp: Date.now(),
    };
  }

  describe('restore from FULL checkpoint', () => {
    it('should return snapshot directly for full checkpoint', async () => {
      const snapshot = { name: 'test', value: 42 };
      mockLoadCheckpoint.mockResolvedValue(createFullCheckpoint('cp-1', snapshot));

      const result = await restorer.restore('cp-1');

      expect(result.snapshot).toEqual(snapshot);
      expect(result.metadata.checkpointChain).toEqual(['cp-1']);
      expect(result.metadata.baseCheckpointId).toBe('cp-1');
    });

    it('should throw error when checkpoint is not found', async () => {
      mockLoadCheckpoint.mockResolvedValue(null);

      await expect(restorer.restore('missing-cp')).rejects.toThrow('Checkpoint not found');
    });
  });

  describe('restore from DELTA checkpoint chain', () => {
    it('should traverse chain and apply deltas', async () => {
      // Chain: cp-base (FULL) → cp-d1 (DELTA) → cp-d2 (DELTA)
      const baseSnapshot = { a: 1, b: 2, c: 3 };
      const fullCp = createFullCheckpoint('cp-base', baseSnapshot);
      const delta1 = createDeltaCheckpoint('cp-d1', 'cp-base', 'cp-base', { a: { from: 1, to: 10 } });
      const delta2 = createDeltaCheckpoint('cp-d2', 'cp-d1', 'cp-base', { b: { from: 2, to: 20 } });

      mockLoadCheckpoint.mockImplementation(async (id: string) => {
        const map: Record<string, typeof fullCp> = {
          'cp-base': fullCp,
          'cp-d1': delta1,
          'cp-d2': delta2,
        };
        return map[id] || null;
      });

      const result = await restorer.restore('cp-d2');

      expect(result.snapshot).toEqual({ a: 10, b: 20, c: 3 });
      expect(result.metadata.checkpointChain).toEqual(['cp-base', 'cp-d1', 'cp-d2']);
      expect(result.metadata.baseCheckpointId).toBe('cp-base');
    });

    it('should handle chain with single delta', async () => {
      const baseSnapshot = { x: 1, y: 2 };
      const fullCp = createFullCheckpoint('cp-base', baseSnapshot);
      const delta1 = createDeltaCheckpoint('cp-d1', 'cp-base', 'cp-base', { x: { from: 1, to: 99 } });

      mockLoadCheckpoint.mockImplementation(async (id: string) => {
        const map: Record<string, typeof fullCp> = {
          'cp-base': fullCp,
          'cp-d1': delta1,
        };
        return map[id] || null;
      });

      const result = await restorer.restore('cp-d1');
      expect(result.snapshot).toEqual({ x: 99, y: 2 });
    });

    it('should handle deltas that delete fields', async () => {
      const baseSnapshot = { a: 1, b: 2, c: 3 };
      const fullCp = createFullCheckpoint('cp-base', baseSnapshot);
      const delta1 = createDeltaCheckpoint('cp-d1', 'cp-base', 'cp-base', { b: { from: 2, to: undefined } });

      mockLoadCheckpoint.mockImplementation(async (id: string) => {
        const map: Record<string, typeof fullCp> = {
          'cp-base': fullCp,
          'cp-d1': delta1,
        };
        return map[id] || null;
      });

      const result = await restorer.restore('cp-d1');
      expect(result.snapshot).toEqual({ a: 1, c: 3 });
    });

    it('should throw error if base checkpoint is missing', async () => {
      mockLoadCheckpoint.mockImplementation(async (id: string) => {
        if (id === 'cp-d1') {
          return createDeltaCheckpoint('cp-d1', 'cp-base', 'cp-base', { a: { from: 1, to: 2 } });
        }
        return null;
      });

      await expect(restorer.restore('cp-d1')).rejects.toThrow('Checkpoint not found in chain');
    });

    it('should throw error if base checkpoint is not FULL type', async () => {
      const baseDelta = createDeltaCheckpoint('cp-base', '', '', {});
      baseDelta.type = 'DELTA' as any;
      const delta1 = createDeltaCheckpoint('cp-d1', 'cp-base', 'cp-base', { a: { from: 1, to: 2 } });

      mockLoadCheckpoint.mockImplementation(async (id: string) => {
        const map: Record<string, typeof baseDelta> = {
          'cp-base': baseDelta,
          'cp-d1': delta1,
        };
        return map[id] || null;
      });

      await expect(restorer.restore('cp-d1')).rejects.toThrow('Base checkpoint not found or invalid');
    });
  });

  describe('buildCheckpointChain', () => {
    it('should return single-element chain for full checkpoint', async () => {
      mockLoadCheckpoint.mockResolvedValue(createFullCheckpoint('cp-1', { a: 1 }));

      const result = await restorer.restore('cp-1');
      expect(result.metadata.checkpointChain).toEqual(['cp-1']);
    });

    it('should throw error for circular references', async () => {
      // Create a cycle: cp-a → cp-b → cp-a
      const deltaA = createDeltaCheckpoint('cp-a', 'cp-b', 'cp-base', {});
      const deltaB = createDeltaCheckpoint('cp-b', 'cp-a', 'cp-base', {});

      mockLoadCheckpoint.mockImplementation(async (id: string) => {
        const map: Record<string, typeof deltaA> = {
          'cp-base': createFullCheckpoint('cp-base', {}),
          'cp-a': deltaA,
          'cp-b': deltaB,
        };
        return map[id] || null;
      });

      await expect(restorer.restore('cp-a')).rejects.toThrow('Circular reference detected');
    });

    it('should throw error if delta checkpoint has no previousCheckpointId', async () => {
      const badDelta = createDeltaCheckpoint('cp-bad', 'cp-base', 'cp-base', {});
      delete (badDelta as any).previousCheckpointId;

      mockLoadCheckpoint.mockImplementation(async (id: string) => {
        if (id === 'cp-bad') return badDelta;
        return createFullCheckpoint('cp-base', {});
      });

      // Chain trace follows previousCheckpointId, which is undefined,
      // so only cp-bad is in the chain. Since cp-bad is DELTA (not FULL),
      // restore throws "Base checkpoint not found or invalid"
      await expect(restorer.restore('cp-bad')).rejects.toThrow();
    });
  });
});
