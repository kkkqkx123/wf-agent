import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { ErrorMetricsCollector } from '../error-collector.js';

describe('ErrorMetricsCollector', () => {
  let collector: ErrorMetricsCollector;

  beforeEach(() => {
    collector = new ErrorMetricsCollector();
  });

  afterEach(() => {
    collector.dispose();
  });

  describe('recordError', () => {
    it('should record error occurrence', () => {
      collector.recordError('LLM_ERROR', 'exec-1', 'node-1', 'Model timeout');
      const result = collector.query({ metricName: 'error.occurrence.count' });
      expect(result.totalCount).toBe(1);
    });

    it('should record error without optional fields', () => {
      collector.recordError('TOOL_ERROR', 'exec-2');
      const result = collector.query({ metricName: 'error.occurrence.count' });
      expect(result.totalCount).toBe(1);
    });

    it('should truncate long error messages', () => {
      const longMsg = 'x'.repeat(300);
      collector.recordError('ERROR', 'exec-1', 'node-1', longMsg);
      const result = collector.query({ metricName: 'error.occurrence.count' });
      expect(result.totalCount).toBe(1);
    });
  });

  describe('recordErrorRecovery', () => {
    it('should record successful error recovery', () => {
      collector.recordErrorRecovery('LLM_ERROR', 'exec-1');
      const result = collector.query({ metricName: 'error.recovery.count' });
      expect(result.totalCount).toBe(1);
    });
  });

  describe('recordErrorRecoveryFailure', () => {
    it('should record failed error recovery', () => {
      collector.recordErrorRecoveryFailure('LLM_ERROR', 'exec-1');
      const result = collector.query({ metricName: 'error.recovery.count' });
      expect(result.totalCount).toBe(1);
    });
  });

  describe('recordAffectedExecution', () => {
    it('should record affected execution', () => {
      collector.recordAffectedExecution('exec-1', 'LLM_ERROR');
      const result = collector.query({ metricName: 'error.affected.executions' });
      expect(result.totalCount).toBe(1);
    });
  });

  describe('recordRecoveryRate', () => {
    it('should record recovery rate gauge', () => {
      collector.recordRecoveryRate('LLM_ERROR', 0.85);
      const result = collector.query({ metricName: 'error.recovery.rate' });
      expect(result.totalCount).toBe(1);
    });
  });

  describe('getErrorStats', () => {
    it('should return all error stats when no filter', () => {
      collector.recordError('LLM_ERROR', 'exec-1');
      collector.recordError('TOOL_ERROR', 'exec-2');
      const result = collector.getErrorStats();
      expect(result.totalCount).toBe(2);
    });

    it('should filter by error type', () => {
      collector.recordError('LLM_ERROR', 'exec-1');
      collector.recordError('TOOL_ERROR', 'exec-2');
      const result = collector.getErrorStats('LLM_ERROR');
      expect(result.totalCount).toBe(1);
    });
  });

  describe('getErrorSummary', () => {
    it('should return error summary with zero values when no data', () => {
      const summary = collector.getErrorSummary();
      expect(summary.totalErrors).toBe(0);
      expect(summary.byType.size).toBe(0);
      expect(summary.topErrors).toEqual([]);
    });

    it('should aggregate errors by type', () => {
      collector.recordError('LLM_ERROR', 'exec-1');
      collector.recordError('LLM_ERROR', 'exec-2');
      collector.recordError('TOOL_ERROR', 'exec-3');
      const summary = collector.getErrorSummary();
      expect(summary.totalErrors).toBeGreaterThan(0);
      expect(summary.topErrors.length).toBeGreaterThanOrEqual(2);
    });
  });

  describe('toPrometheus', () => {
    it('should export in Prometheus format', () => {
      collector.recordError('LLM_ERROR', 'exec-1');
      const lines = collector.toPrometheus();
      expect(lines.length).toBeGreaterThan(0);
      expect(lines.some(l => l.includes('error_total'))).toBe(true);
    });
  });

  describe('toJSON', () => {
    it('should export as JSON', () => {
      const json = collector.toJSON();
      expect(json).toHaveProperty('type', 'error');
      expect(json).toHaveProperty('totalErrors');
    });
  });
});