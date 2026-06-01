import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { EventMetricsCollector } from '../event-collector.js';

describe('EventMetricsCollector', () => {
  let collector: EventMetricsCollector;

  beforeEach(() => {
    collector = new EventMetricsCollector();
  });

  afterEach(() => {
    collector.dispose();
  });

  describe('recordEvent', () => {
    it('should record an event counter', () => {
      collector.recordEvent('NODE_COMPLETED', 'exec-1', {
        workflow_id: 'wf-1',
        node_type: 'LLM',
      });
      const result = collector.query({ metricName: 'event.node.completed.count' });
      expect(result.totalCount).toBe(1);
    });

    it('should warn when eventType is missing', () => {
      const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
      collector.recordEvent('', 'exec-1');
      const result = collector.query({});
      expect(result.totalCount).toBe(0);
      warnSpy.mockRestore();
    });

    it('should warn when executionId is missing', () => {
      const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
      collector.recordEvent('TEST_EVENT', '');
      const result = collector.query({});
      expect(result.totalCount).toBe(0);
      warnSpy.mockRestore();
    });

    it('should convert event type to metric name correctly', () => {
      collector.recordEvent('WORKFLOW_STARTED', 'exec-1');
      const result = collector.query({ metricName: 'event.workflow.started.count' });
      expect(result.totalCount).toBe(1);
    });
  });

  describe('getStatistics', () => {
    it('should return undefined when no events recorded', () => {
      const stat = collector.getStatistics('NODE_COMPLETED');
      expect(stat).toBeUndefined();
    });

    it('should return aggregated statistics for an event type', () => {
      collector.recordEvent('NODE_COMPLETED', 'exec-1', { workflow_id: 'wf-1' });
      collector.recordEvent('NODE_COMPLETED', 'exec-1', { workflow_id: 'wf-1' });
      const stat = collector.getStatistics('NODE_COMPLETED');
      expect(stat).toBeDefined();
      expect(stat!.count).toBeGreaterThanOrEqual(2);
      expect(stat!.byExecution.has('exec-1')).toBe(true);
    });
  });

  describe('getAllStatistics', () => {
    it('should return all event statistics', () => {
      collector.recordEvent('NODE_COMPLETED', 'exec-1');
      collector.recordEvent('WORKFLOW_STARTED', 'exec-1');
      const allStats = collector.getAllStatistics();
      expect(allStats.size).toBeGreaterThanOrEqual(2);
    });
  });

  describe('generateSummary', () => {
    it('should generate a summary with total events', () => {
      collector.recordEvent('NODE_COMPLETED', 'exec-1');
      collector.recordEvent('WORKFLOW_STARTED', 'exec-2');
      const summary = collector.generateSummary();
      expect(summary.totalEvents).toBeGreaterThanOrEqual(2);
      expect(summary.byEventType.size).toBeGreaterThanOrEqual(2);
      expect(summary.generatedAt).toBeGreaterThan(0);
    });
  });

  describe('queryEvents', () => {
    it('should support advanced query filtering', () => {
      collector.recordEvent('NODE_COMPLETED', 'exec-1', { workflow_id: 'wf-1' });
      const result = collector.queryEvents({ metricName: 'event.node.completed.count' });
      expect(result.totalCount).toBe(1);
    });
  });

  describe('cleanupExecution', () => {
    it('should cleanup metrics for a specific execution', () => {
      collector.recordEvent('NODE_COMPLETED', 'exec-1');
      collector.recordEvent('NODE_COMPLETED', 'exec-2');
      const cleaned = collector.cleanupExecution('exec-1');
      expect(cleaned).toBeGreaterThan(0);
    });

    it('should return 0 for empty executionId', () => {
      const cleaned = collector.cleanupExecution('');
      expect(cleaned).toBe(0);
    });
  });

  describe('toPrometheus', () => {
    it('should export event metrics in Prometheus format', () => {
      collector.recordEvent('NODE_COMPLETED', 'exec-1');
      const lines = collector.toPrometheus();
      expect(lines.length).toBeGreaterThan(0);
    });
  });

  describe('toJSON', () => {
    it('should export event metrics as JSON', () => {
      const json = collector.toJSON();
      expect(json).toHaveProperty('type', 'event');
    });
  });
});