/**
 * QueryExecutor advanced features tests
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { QueryExecutor } from '../../../src/executors/query.executor.js';
import type { ExecutionRecord } from '../../../src/types/common.types.js';

describe('QueryExecutor - Advanced Features', () => {
  let executor: QueryExecutor;
  let testRecords: ExecutionRecord[];

  beforeEach(() => {
    executor = new QueryExecutor({});

    testRecords = [
      {
        executionId: 'exec-1',
        workflowId: 'wf-1',
        status: 'completed',
        startTime: Date.now() - 10000,
        endTime: Date.now(),
        duration: 10000,
        input: { param: 'value1' },
        output: { result: 100 },
      },
      {
        executionId: 'exec-2',
        workflowId: 'wf-1',
        status: 'completed',
        startTime: Date.now() - 5000,
        endTime: Date.now(),
        duration: 5000,
        input: { param: 'value2' },
        output: { result: 200 },
      },
      {
        executionId: 'exec-3',
        workflowId: 'wf-2',
        status: 'failed',
        startTime: Date.now() - 3000,
        endTime: Date.now(),
        duration: 3000,
        input: { param: 'value3' },
        error: 'Test error',
      },
    ];
  });

  describe('filterBy', () => {
    it('should filter with single expression', () => {
      const filtered = executor.applyFilterExpressions(testRecords, {
        field: 'status',
        operator: 'eq',
        value: 'completed',
      });

      expect(filtered.length).toBe(2);
      expect(filtered.every((r) => r.status === 'completed')).toBe(true);
    });

    it('should filter with multiple expressions', () => {
      const filtered = executor.applyFilterExpressions(testRecords, [
        { field: 'status', operator: 'eq', value: 'completed' },
        { field: 'workflowId', operator: 'eq', value: 'wf-1' },
      ]);

      expect(filtered.length).toBe(2);
    });

    it('should support "contains" operator', () => {
      const filtered = executor.applyFilterExpressions(testRecords, {
        field: 'workflowId',
        operator: 'contains',
        value: 'wf',
      });

      expect(filtered.length).toBe(3);
    });

    it('should support "in" operator', () => {
      const filtered = executor.applyFilterExpressions(testRecords, {
        field: 'status',
        operator: 'in',
        value: ['completed', 'running'],
      });

      expect(filtered.length).toBe(2);
    });

    it('should support "regex" operator', () => {
      const filtered = executor.applyFilterExpressions(testRecords, {
        field: 'executionId',
        operator: 'regex',
        value: 'exec-[12]',
      });

      expect(filtered.length).toBe(2);
    });
  });

  describe('aggregate', () => {
    it('should count records', async () => {
      const result = await executor.aggregate(testRecords, {
        type: 'count',
      });

      expect(result[0].count).toBe(3);
    });

    it('should sum numeric field', async () => {
      const result = await executor.aggregate(testRecords, {
        type: 'sum',
        field: 'duration',
        as: 'totalDuration',
      });

      expect(result[0].totalDuration).toBe(18000);
    });

    it('should calculate average', async () => {
      const result = await executor.aggregate(testRecords, {
        type: 'avg',
        field: 'duration',
        as: 'avgDuration',
      });

      expect(result[0].avgDuration).toBe(6000);
    });

    it('should find minimum', async () => {
      const result = await executor.aggregate(testRecords, {
        type: 'min',
        field: 'duration',
        as: 'minDuration',
      });

      expect(result[0].minDuration).toBe(3000);
    });

    it('should find maximum', async () => {
      const result = await executor.aggregate(testRecords, {
        type: 'max',
        field: 'duration',
        as: 'maxDuration',
      });

      expect(result[0].maxDuration).toBe(10000);
    });

    it('should group by field', async () => {
      const result = await executor.aggregate(testRecords, {
        type: 'group_by',
        groupBy: 'status',
        as: 'statusGroups',
      });

      expect(result[0].statusGroups).toHaveProperty('completed');
      expect(result[0].statusGroups).toHaveProperty('failed');
    });

    it('should apply multiple aggregations', async () => {
      const result = await executor.aggregate(testRecords, [
        { type: 'count' },
        { type: 'sum', field: 'duration', as: 'total' },
      ]);

      expect(result).toHaveLength(2);
      expect(result[0].count).toBe(3);
      expect(result[1].total).toBe(18000);
    });
  });

  describe('export', () => {
    it('should export to JSON', () => {
      const json = executor.exportToFormat(testRecords, 'json');
      const parsed = JSON.parse(json);

      expect(Array.isArray(parsed)).toBe(true);
      expect(parsed.length).toBe(3);
      expect(parsed[0].executionId).toBe('exec-1');
    });

    it('should export to CSV', () => {
      const csv = executor.exportToFormat(testRecords, 'csv');
      const lines = csv.split('\n');

      expect(lines.length).toBe(4); // header + 3 records
      expect(lines[0]).toContain('executionId');
    });

    it('should export to XML', () => {
      const xml = executor.exportToFormat(testRecords, 'xml');

      expect(xml).toContain('<?xml');
      expect(xml).toContain('<records>');
      expect(xml).toContain('<record>');
      expect(xml).toContain('exec-1');
    });
  });

  describe('distinct', () => {
    it('should return distinct values', () => {
      const distinct = executor.getDistinct(testRecords, 'status');

      expect(distinct.length).toBe(2);
      expect(distinct).toContain('completed');
      expect(distinct).toContain('failed');
    });

    it('should return distinct workflow IDs', () => {
      const distinct = executor.getDistinct(testRecords, 'workflowId');

      expect(distinct.length).toBe(2);
      expect(distinct).toContain('wf-1');
      expect(distinct).toContain('wf-2');
    });
  });

  describe('groupBy', () => {
    it('should group records by field', () => {
      const groups = executor.groupByField(testRecords, 'status');

      expect(groups.size).toBe(2);
      expect(groups.get('completed')?.length).toBe(2);
      expect(groups.get('failed')?.length).toBe(1);
    });

    it('should group by workflow ID', () => {
      const groups = executor.groupByField(testRecords, 'workflowId');

      expect(groups.size).toBe(2);
      expect(groups.get('wf-1')?.length).toBe(2);
      expect(groups.get('wf-2')?.length).toBe(1);
    });
  });
});
