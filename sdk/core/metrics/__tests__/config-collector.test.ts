import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { ConfigMetricsCollector } from '../config-collector.js';

describe('ConfigMetricsCollector', () => {
  let collector: ConfigMetricsCollector;

  beforeEach(() => {
    collector = new ConfigMetricsCollector();
  });

  afterEach(() => {
    collector.dispose();
  });

  describe('recordAccess', () => {
    it('should record config access counter', () => {
      collector.recordAccess('config-key-1', 'workflow', { source: 'file' });
      const result = collector.query({ metricName: 'config.access.count' });
      expect(result.totalCount).toBe(1);
    });

    it('should record without optional fields', () => {
      collector.recordAccess('config-key-1');
      const result = collector.query({ metricName: 'config.access.count' });
      expect(result.totalCount).toBe(1);
    });
  });

  describe('recordLoadComplete', () => {
    it('should record load duration histogram', () => {
      collector.recordLoadComplete('config-key-1', 500, true, 'workflow', { source: 'file' });
      const result = collector.query({ metricName: 'config.load.duration' });
      expect(result.totalCount).toBe(1);
    });

    it('should record validation error for failed load', () => {
      collector.recordLoadComplete('config-key-1', 100, false);
      const errorResult = collector.query({ metricName: 'config.validation_error.count' });
      expect(errorResult.totalCount).toBe(1);
    });
  });

  describe('recordValidationError', () => {
    it('should record validation error counter', () => {
      collector.recordValidationError('config-key-1', 'missing_field', 'workflow');
      const result = collector.query({ metricName: 'config.validation_error.count' });
      expect(result.totalCount).toBe(1);
    });
  });

  describe('recordCacheHit', () => {
    it('should record cache hit counter', () => {
      collector.recordCacheHit('config-key-1', 'workflow');
      const result = collector.query({ metricName: 'config.cache.hit_count' });
      expect(result.totalCount).toBe(1);
    });
  });

  describe('recordCacheMiss', () => {
    it('should record cache miss counter', () => {
      collector.recordCacheMiss('config-key-1', 'workflow');
      const result = collector.query({ metricName: 'config.cache.miss_count' });
      expect(result.totalCount).toBe(1);
    });
  });

  describe('getCacheHitRate', () => {
    it('should return undefined when no data', () => {
      const rate = collector.getCacheHitRate('config-key-1');
      expect(rate).toBeUndefined();
    });

    it('should calculate cache hit rate', () => {
      collector.recordCacheHit('config-key-1', 'workflow');
      collector.recordCacheHit('config-key-1', 'workflow');
      collector.recordCacheMiss('config-key-1', 'workflow');
      const rate = collector.getCacheHitRate('config-key-1', 'workflow');
      expect(rate).toBeCloseTo(2 / 3);
    });
  });

  describe('getAverageLoadDuration', () => {
    it('should return 0 when no data', () => {
      const avg = collector.getAverageLoadDuration('config-key-1');
      expect(avg).toBe(0);
    });

    it('should return average load duration after loads', () => {
      collector.recordLoadComplete('config-key-1', 100, true);
      collector.recordLoadComplete('config-key-1', 200, true);
      const avg = collector.getAverageLoadDuration('config-key-1');
      expect(avg).toBeGreaterThanOrEqual(100);
    });
  });

  describe('toPrometheus', () => {
    it('should export in Prometheus format', () => {
      collector.recordAccess('config-key-1', 'workflow');
      const lines = collector.toPrometheus();
      expect(lines.length).toBeGreaterThan(0);
      expect(lines.some(l => l.includes('config_access_total'))).toBe(true);
    });
  });

  describe('toJSON', () => {
    it('should export as JSON', () => {
      const json = collector.toJSON();
      expect(json).toHaveProperty('type', 'config');
    });
  });
});