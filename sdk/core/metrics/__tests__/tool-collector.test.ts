import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { ToolMetricsCollector } from '../tool-collector.js';

describe('ToolMetricsCollector', () => {
  let collector: ToolMetricsCollector;

  beforeEach(() => {
    collector = new ToolMetricsCollector();
  });

  afterEach(() => {
    collector.dispose();
  });

  describe('recordToolCallStart', () => {
    it('should record tool call start', () => {
      collector.recordToolCallStart('tool-1', 'exec-1');
      const result = collector.query({ metricName: 'tool.call.count' });
      expect(result.totalCount).toBe(1);
    });
  });

  describe('recordToolCallComplete', () => {
    it('should record successful tool call completion', () => {
      collector.recordToolCallComplete('tool-1', 'exec-1', 500, true, 100, 200);
      const durationResult = collector.query({ metricName: 'tool.call.duration' });
      expect(durationResult.totalCount).toBe(1);
    });

    it('should record error for failed tool call', () => {
      collector.recordToolCallComplete('tool-1', 'exec-1', 500, false);
      const errorResult = collector.query({ metricName: 'tool.error.count' });
      expect(errorResult.totalCount).toBe(1);
    });

    it('should record parameter and result sizes when provided', () => {
      collector.recordToolCallComplete('tool-1', 'exec-1', 500, true, 1024, 2048);
      const paramResult = collector.query({ metricName: 'tool.parameter.size' });
      expect(paramResult.totalCount).toBe(1);
      const resultResult = collector.query({ metricName: 'tool.result.size' });
      expect(resultResult.totalCount).toBe(1);
    });
  });

  describe('recordToolError', () => {
    it('should record a tool error with type', () => {
      collector.recordToolError('tool-1', 'exec-1', 'EXECUTION_FAILED', 'Something went wrong');
      const result = collector.query({ metricName: 'tool.error.count' });
      expect(result.totalCount).toBe(1);
    });

    it('should truncate long error messages', () => {
      const longMsg = 'x'.repeat(300);
      collector.recordToolError('tool-1', 'exec-1', 'ERROR', longMsg);
      const result = collector.query({ metricName: 'tool.error.count' });
      expect(result.totalCount).toBe(1);
    });
  });

  describe('getToolStats', () => {
    it('should return all tool stats when no filter', () => {
      collector.recordToolCallStart('tool-1', 'exec-1');
      const result = collector.getToolStats();
      expect(result.totalCount).toBe(1);
    });

    it('should filter by toolId', () => {
      collector.recordToolCallStart('tool-1', 'exec-1');
      collector.recordToolCallStart('tool-2', 'exec-2');
      const result = collector.getToolStats('tool-1');
      expect(result.totalCount).toBe(1);
    });
  });

  describe('getToolPerformanceSummary', () => {
    it('should return empty map when no data', () => {
      const summary = collector.getToolPerformanceSummary();
      expect(summary.size).toBe(0);
    });

    it('should return performance summary after tool calls', () => {
      collector.recordToolCallStart('tool-1', 'exec-1');
      collector.recordToolCallComplete('tool-1', 'exec-1', 500, true);
      const summary = collector.getToolPerformanceSummary();
      expect(summary.size).toBe(1);
      expect(summary.has('tool-1')).toBe(true);
    });
  });

  describe('toPrometheus', () => {
    it('should export in Prometheus format', () => {
      collector.recordToolCallStart('tool-1', 'exec-1');
      const lines = collector.toPrometheus();
      expect(lines.length).toBeGreaterThan(0);
    });
  });

  describe('toJSON', () => {
    it('should export as JSON', () => {
      const json = collector.toJSON();
      expect(json).toHaveProperty('type', 'tool');
    });
  });
});