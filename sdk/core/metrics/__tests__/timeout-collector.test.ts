import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { TimeoutMetricsCollector } from '../timeout-collector.js';

describe('TimeoutMetricsCollector', () => {
  let collector: TimeoutMetricsCollector;

  beforeEach(() => {
    collector = new TimeoutMetricsCollector();
  });

  afterEach(() => {
    collector.dispose();
  });

  describe('recordRegistration', () => {
    it('should record timeout registration', () => {
      collector.recordRegistration('tag-1', 30000, 'exec-1');
      const result = collector.query({ metricName: 'timeout.registration.count' });
      expect(result.totalCount).toBe(1);
    });
  });

  describe('recordExpiration', () => {
    it('should record timeout expiration', () => {
      collector.recordExpiration('tag-1', 30000, 'exec-1');
      const result = collector.query({ metricName: 'timeout.expiration.count' });
      expect(result.totalCount).toBe(1);
    });
  });

  describe('recordCancellation', () => {
    it('should record timeout cancellation', () => {
      collector.recordCancellation('tag-1', 'user', 'exec-1');
      const result = collector.query({ metricName: 'timeout.cancellation.count' });
      expect(result.totalCount).toBe(1);
    });
  });

  describe('recordWarning', () => {
    it('should record timeout warning', () => {
      collector.recordWarning('tag-1', 5000, 'exec-1');
      const countResult = collector.query({ metricName: 'timeout.warning.count' });
      expect(countResult.totalCount).toBe(1);
      const gaugeResult = collector.query({ metricName: 'timeout.warning.remaining_time' });
      expect(gaugeResult.totalCount).toBe(1);
    });
  });

  describe('generateSummary', () => {
    it('should return summary with zero values when no registry bound', () => {
      const summary = collector.generateSummary();
      expect(summary.totalActive).toBe(0);
      expect(summary.totalExpired).toBe(0);
      expect(summary.totalCancelled).toBe(0);
      expect(summary.byTag).toEqual({});
      expect(summary.byCategory).toEqual({});
      expect(summary.averageDuration).toBe(0);
      expect(summary.timeoutRate).toBe(0);
    });
  });

  describe('collectFromRegistry', () => {
    it('should warn when registry not bound', () => {
      const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
      collector.collectFromRegistry();
      expect(warnSpy).toHaveBeenCalled();
      warnSpy.mockRestore();
    });
  });

  describe('toPrometheus', () => {
    it('should export in Prometheus format', () => {
      collector.recordRegistration('tag-1', 30000, 'exec-1');
      const lines = collector.toPrometheus();
      expect(lines.length).toBeGreaterThan(0);
    });
  });

  describe('toJSON', () => {
    it('should export as JSON with collector field', () => {
      const json = collector.toJSON();
      expect(json).toHaveProperty('collector', 'timeout');
      expect(json).toHaveProperty('summary');
    });
  });
});