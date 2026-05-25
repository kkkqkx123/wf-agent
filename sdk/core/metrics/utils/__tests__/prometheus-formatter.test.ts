/**
 * Prometheus Formatter Unit Tests
 */

import { describe, it, expect } from 'vitest';
import { PrometheusFormatter } from '../prometheus-formatter.js';
import type { PrometheusMetric } from '../prometheus-formatter.js';

describe('PrometheusFormatter', () => {
  describe('formatLabels', () => {
    it('should format labels correctly', () => {
      const labels = { node_type: 'LLM', status: 'success' };
      const result = PrometheusFormatter.formatLabels(labels);
      expect(result).toBe('{node_type="LLM",status="success"}');
    });
    
    it('should handle empty labels', () => {
      expect(PrometheusFormatter.formatLabels({})).toBe('');
      expect(PrometheusFormatter.formatLabels(undefined as any)).toBe('');
    });
    
    it('should escape special characters', () => {
      const labels = { message: 'value with "quotes" and \\backslash' };
      const result = PrometheusFormatter.formatLabels(labels);
      expect(result).toContain('\\"');
      expect(result).toContain('\\\\');
    });
    
    it('should escape newlines', () => {
      const labels = { message: 'line1\nline2' };
      const result = PrometheusFormatter.formatLabels(labels);
      expect(result).toContain('\\n');
    });
  });

  describe('formatSample', () => {
    it('should format sample without labels', () => {
      const result = PrometheusFormatter.formatSample('test_counter', { value: 42 });
      expect(result).toBe('test_counter 42');
    });
    
    it('should format sample with labels', () => {
      const result = PrometheusFormatter.formatSample('test_counter', { 
        labels: { type: 'http' }, 
        value: 100 
      });
      expect(result).toBe('test_counter{type="http"} 100');
    });
    
    it('should format sample with timestamp', () => {
      const timestamp = Date.now();
      const result = PrometheusFormatter.formatSample('test_counter', { 
        value: 100,
        timestamp 
      });
      expect(result).toBe(`test_counter 100 ${timestamp}`);
    });
  });

  describe('formatMetric', () => {
    it('should format complete metric', () => {
      const metric: PrometheusMetric = {
        name: 'test_counter',
        type: 'counter',
        help: 'Test counter',
        samples: [
          { value: 10 },
          { labels: { label1: 'value1' }, value: 20 }
        ]
      };
      
      const lines = PrometheusFormatter.formatMetric(metric);
      expect(lines).toEqual([
        '# HELP test_counter Test counter',
        '# TYPE test_counter counter',
        'test_counter 10',
        'test_counter{label1="value1"} 20'
      ]);
    });
    
    it('should format gauge metric', () => {
      const metric: PrometheusMetric = {
        name: 'test_gauge',
        type: 'gauge',
        help: 'Test gauge',
        samples: [{ value: 3.14 }]
      };
      
      const lines = PrometheusFormatter.formatMetric(metric);
      expect(lines[0]).toBe('# HELP test_gauge Test gauge');
      expect(lines[1]).toBe('# TYPE test_gauge gauge');
      expect(lines[2]).toBe('test_gauge 3.14');
    });
    
    it('should format histogram metric', () => {
      const metric: PrometheusMetric = {
        name: 'test_histogram',
        type: 'histogram',
        help: 'Test histogram',
        samples: [
          { labels: { le: '0.5' }, value: 5 },
          { labels: { le: '1.0' }, value: 10 },
          { labels: { le: '+Inf' }, value: 15 }
        ]
      };
      
      const lines = PrometheusFormatter.formatMetric(metric);
      expect(lines.length).toBe(5); // HELP + TYPE + 3 samples
    });
    
    it('should format summary metric with quantiles', () => {
      const metric: PrometheusMetric = {
        name: 'test_summary',
        type: 'summary',
        help: 'Test summary',
        samples: [
          { labels: { quantile: '0.5' }, value: 100 },
          { labels: { quantile: '0.95' }, value: 200 },
          { labels: { quantile: '0.99' }, value: 300 }
        ]
      };
      
      const lines = PrometheusFormatter.formatMetric(metric);
      expect(lines).toContain('test_summary{quantile="0.5"} 100');
      expect(lines).toContain('test_summary{quantile="0.95"} 200');
      expect(lines).toContain('test_summary{quantile="0.99"} 300');
    });
  });

  describe('combine', () => {
    it('should combine multiple metrics', () => {
      const metrics = [
        ['# HELP metric1 Help 1', '# TYPE metric1 counter', 'metric1 10'],
        ['# HELP metric2 Help 2', '# TYPE metric2 gauge', 'metric2 20']
      ];
      
      const result = PrometheusFormatter.combine(metrics, false);
      expect(result).toContain('metric1 10');
      expect(result).toContain('metric2 20');
      expect(result.endsWith('\n')).toBe(true);
    });
    
    it('should add timestamp when enabled', () => {
      const metrics = [['# HELP test Test', '# TYPE test counter', 'test 1']];
      const result = PrometheusFormatter.combine(metrics, true);
      expect(result).toContain('# Generated at');
    });
    
    it('should not add timestamp when disabled', () => {
      const metrics = [['# HELP test Test', '# TYPE test counter', 'test 1']];
      const result = PrometheusFormatter.combine(metrics, false);
      expect(result).not.toContain('# Generated at');
    });
    
    it('should handle empty metrics array', () => {
      const result = PrometheusFormatter.combine([], false);
      expect(result).toBe('\n');
    });
  });
});
