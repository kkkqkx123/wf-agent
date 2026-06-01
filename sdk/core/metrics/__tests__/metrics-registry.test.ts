import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { MetricsRegistry } from '../metrics-registry.js';
import type { MetricCollectorConfig } from '../types.js';

describe('MetricsRegistry', () => {
  let registry: MetricsRegistry;

  beforeEach(() => {
    vi.useFakeTimers();
    registry = new MetricsRegistry();
  });

  afterEach(() => {
    registry.dispose();
    vi.useRealTimers();
  });

  describe('constructor', () => {
    it('should initialize all collectors', () => {
      const collectors = registry.getCollectors();
      expect(collectors.size).toBe(12);
      expect(collectors.has('workflow')).toBe(true);
      expect(collectors.has('node')).toBe(true);
      expect(collectors.has('agent')).toBe(true);
      expect(collectors.has('event')).toBe(true);
      expect(collectors.has('tool')).toBe(true);
      expect(collectors.has('token')).toBe(true);
      expect(collectors.has('template')).toBe(true);
      expect(collectors.has('config')).toBe(true);
      expect(collectors.has('error')).toBe(true);
      expect(collectors.has('resource')).toBe(true);
      expect(collectors.has('agentLoop')).toBe(true);
      expect(collectors.has('timeout')).toBe(true);
    });

    it('should setup periodic reporting when enabled', () => {
      const reportRegistry = new MetricsRegistry({ enablePeriodicReporting: true, reportingInterval: 5000 });
      expect(reportRegistry).toBeDefined();
      reportRegistry.dispose();
    });
  });

  describe('getCollector', () => {
    it('should return typed collectors', () => {
      const workflow = registry.getWorkflowCollector();
      expect(workflow).toBeDefined();

      const node = registry.getNodeCollector();
      expect(node).toBeDefined();

      const agent = registry.getAgentCollector();
      expect(agent).toBeDefined();

      const event = registry.getEventCollector();
      expect(event).toBeDefined();

      const tool = registry.getToolCollector();
      expect(tool).toBeDefined();

      const token = registry.getTokenCollector();
      expect(token).toBeDefined();

      const template = registry.getTemplateCollector();
      expect(template).toBeDefined();

      const config = registry.getConfigCollector();
      expect(config).toBeDefined();

      const error = registry.getErrorCollector();
      expect(error).toBeDefined();

      const resource = registry.getResourceCollector();
      expect(resource).toBeDefined();

      const agentLoop = registry.getAgentLoopCollector();
      expect(agentLoop).toBeDefined();

      const timeout = registry.getTimeoutCollector();
      expect(timeout).toBeDefined();
    });

    it('should return undefined for unknown collector name', () => {
      const unknown = registry.getCollector('unknown' as any);
      expect(unknown).toBeUndefined();
    });
  });

  describe('getCollectors', () => {
    it('should return a copy of collectors map', () => {
      const collectors = registry.getCollectors();
      collectors.delete('workflow');
      expect(registry.getCollectors().size).toBe(12);
    });
  });

  describe('flushAll', () => {
    it('should flush all collectors without error', async () => {
      const workflow = registry.getWorkflowCollector()!;
      workflow.incrementCounter('test.counter');

      await expect(registry.flushAll()).resolves.toBeUndefined();
    });
  });

  describe('generateReport', () => {
    it('should generate a report with summary', async () => {
      const workflow = registry.getWorkflowCollector()!;
      workflow.recordExecutionStart('wf-1', 'exec-1', { version: '1.0' });
      workflow.recordExecutionComplete('wf-1', 'exec-1', {
        success: true,
        duration: 1000,
        nodeCount: 5,
      });

      const report = await registry.generateReport();
      expect(report).toHaveProperty('timestamp');
      expect(report).toHaveProperty('summary');
      expect(report.summary).toHaveProperty('totalMetrics');
      expect(report.summary).toHaveProperty('byType');
      expect(report.summary).toHaveProperty('byCategory');
      expect(report).toHaveProperty('topMetrics');
      expect(report).toHaveProperty('anomalies');
    });

    it('should generate report with timeRange filter', async () => {
      const report = await registry.generateReport({
        timeRange: { from: Date.now() - 10000, to: Date.now() + 10000 },
      });
      expect(report.timeRange).toBeDefined();
    });

    it('should generate report with trends when requested', async () => {
      const workflow = registry.getWorkflowCollector()!;
      workflow.recordExecutionStart('wf-1', 'exec-1');
      workflow.recordExecutionComplete('wf-1', 'exec-1', {
        success: true,
        duration: 500,
        nodeCount: 3,
      });

      const report = await registry.generateReport({
        timeRange: { from: Date.now() - 10000, to: Date.now() + 10000 },
        includeTrends: true,
      });
      expect(report.trends).toBeDefined();
    });
  });

  describe('onReport', () => {
    it('should subscribe and unsubscribe from reports', async () => {
      const callback = vi.fn();
      const unsubscribe = registry.onReport(callback);

      await registry.generateReport();
      expect(callback).toHaveBeenCalled();

      unsubscribe();
      callback.mockClear();
      await registry.generateReport();
      expect(callback).not.toHaveBeenCalled();
    });
  });

  describe('dispose', () => {
    it('should clear all collectors and stop reporting', () => {
      registry.dispose();
      const collectors = registry.getCollectors();
      expect(collectors.size).toBe(0);
    });
  });
});