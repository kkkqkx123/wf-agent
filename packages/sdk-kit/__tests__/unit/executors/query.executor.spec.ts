/**
 * Query Executor Tests
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { QueryExecutor, QueryBuilderImpl } from '@/executors/query.executor.js';
import { KitError } from '@/converters/error.converter.js';

describe('QueryExecutor', () => {
  let executor: QueryExecutor;
  let mockSDK: any;
  let mockRegistry: any;

  beforeEach(() => {
    mockRegistry = {
      query: vi.fn(),
    };

    mockSDK = {
      getFactory: vi.fn(() => ({
        getWorkflowExecutionRegistry: vi.fn(() => mockRegistry),
      })),
    };

    executor = new QueryExecutor(mockSDK);
  });

  describe('query', () => {
    it('should convert records correctly', async () => {
      const mockRecords = [
        {
          id: 'exec1',
          executionId: 'exec1',
          workflowId: 'workflow1',
          status: 'completed',
          createdAt: 1000,
          completedAt: 2000,
        },
      ];

      mockRegistry.query.mockResolvedValue(mockRecords);

      const results = await executor.query();

      expect(results).toHaveLength(1);
      expect(results[0].executionId).toBe('exec1');
      expect(results[0].duration).toBe(1000);
    });

    it('should handle empty results', async () => {
      mockRegistry.query.mockResolvedValue([]);

      const results = await executor.query();

      expect(results).toEqual([]);
    });

    it('should handle null results', async () => {
      mockRegistry.query.mockResolvedValue(null);

      const results = await executor.query();

      expect(results).toEqual([]);
    });

    it('should throw on SDK error', async () => {
      mockRegistry.query.mockRejectedValue(
        new Error('SDK error')
      );

      await expect(executor.query()).rejects.toThrow(KitError);
    });

    it('should throw when factory not available', async () => {
      const sdk = { getFactory: vi.fn(() => null) };
      const exec = new QueryExecutor(sdk);

      await expect(exec.query()).rejects.toThrow(KitError);
    });
  });
});

describe('QueryBuilder', () => {
  let builder: QueryBuilderImpl;
  let mockExecutor: any;

  beforeEach(() => {
    mockExecutor = {
      query: vi.fn(),
    };

    builder = new QueryBuilderImpl(mockExecutor);
  });

  describe('filter', () => {
    it('should merge filter criteria', () => {
      builder.filter({ status: 'completed' });
      builder.filter({ workflowId: 'wf1' });

      // Access private property via any for testing
      const builderAny = builder as any;
      expect(builderAny.filters).toEqual({
        status: 'completed',
        workflowId: 'wf1',
      });
    });
  });

  describe('sort', () => {
    it('should set sort options', () => {
      builder.sort('createdAt', 'desc');

      const builderAny = builder as any;
      expect(builderAny.sortOptions).toEqual({
        field: 'createdAt',
        order: 'desc',
      });
    });
  });

  describe('pagination', () => {
    it('should set limit', () => {
      builder.limit(50);

      const builderAny = builder as any;
      expect(builderAny.pagination.limit).toBe(50);
    });

    it('should set offset', () => {
      builder.offset(10);

      const builderAny = builder as any;
      expect(builderAny.pagination.offset).toBe(10);
    });
  });

  describe('get', () => {
    it('should call executor query', async () => {
      mockExecutor.query.mockResolvedValue([]);

      await builder.get();

      expect(mockExecutor.query).toHaveBeenCalled();
    });
  });

  describe('first', () => {
    it('should return first record', async () => {
      const record = { executionId: 'exec1', workflowId: 'wf1', status: 'completed', startTime: 0 };
      mockExecutor.query.mockResolvedValue([record]);

      const result = await builder.first();

      expect(result).toEqual(record);
    });

    it('should return null when no records', async () => {
      mockExecutor.query.mockResolvedValue([]);

      const result = await builder.first();

      expect(result).toBeNull();
    });
  });

  describe('count', () => {
    it('should return count of results', async () => {
      mockExecutor.query.mockResolvedValue([{}, {}, {}]);

      const count = await builder.count();

      expect(count).toBe(3);
    });
  });
});
