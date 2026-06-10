import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { NodeMetricsCollector } from '../node-collector.js';

describe('NodeMetricsCollector', () => {
  let collector: NodeMetricsCollector;

  beforeEach(() => {
    collector = new NodeMetricsCollector();
  });

  afterEach(() => {
    collector.dispose();
  });

  describe('recordTemplateInstantiation', () => {
    it('should record template instantiation counter', () => {
      collector.recordTemplateInstantiation('llm-template', 'LLM', { category: 'llm' });
      const result = collector.query({ metricName: 'node.template.instantiation.count' });
      expect(result.totalCount).toBe(1);
    });

    it('should warn on missing parameters', () => {
      const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
      collector.recordTemplateInstantiation('', '');
      const result = collector.query({});
      expect(result.totalCount).toBe(0);
      warnSpy.mockRestore();
    });
  });

  describe('recordNodeExecutionStart', () => {
    it('should record node execution start', () => {
      collector.recordNodeExecutionStart('node-1', 'LLM', 'wf-1');
      const result = collector.query({ metricName: 'node.execution.started.count' });
      expect(result.totalCount).toBe(1);
    });

    it('should warn on missing parameters', () => {
      const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
      collector.recordNodeExecutionStart('', '', '');
      const result = collector.query({});
      expect(result.totalCount).toBe(0);
      warnSpy.mockRestore();
    });
  });

  describe('recordNodeExecution', () => {
    it('should record successful node execution', () => {
      collector.recordNodeExecution('node-1', 'LLM', 'wf-1', {
        success: true,
        duration: 500,
      });
      const countResult = collector.query({ metricName: 'node.execution.count' });
      expect(countResult.totalCount).toBe(1);
      const successResult = collector.query({ metricName: 'node.execution.success.count' });
      expect(successResult.totalCount).toBe(1);
    });

    it('should record failed node execution with error type', () => {
      collector.recordNodeExecution('node-1', 'LLM', 'wf-1', {
        success: false,
        duration: 200,
        errorType: 'TIMEOUT',
      });
      const failureResult = collector.query({ metricName: 'node.execution.failure.count' });
      expect(failureResult.totalCount).toBe(1);
    });

    it('should record token usage for LLM nodes', () => {
      collector.recordNodeExecution('node-1', 'LLM', 'wf-1', {
        success: true,
        duration: 300,
        tokenUsage: 1500,
      });
      const tokenResult = collector.query({ metricName: 'node.execution.token_usage' });
      expect(tokenResult.totalCount).toBe(1);
    });

    it('should warn on missing parameters', () => {
      const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
      collector.recordNodeExecution('', '', '', { success: true, duration: 0 });
      const result = collector.query({});
      expect(result.totalCount).toBe(0);
      warnSpy.mockRestore();
    });
  });

  describe('recordSubgraphExecution', () => {
    it('should record subgraph execution metrics', () => {
      collector.recordSubgraphExecution('node-1', 'wf-1', {
        success: true,
        duration: 1000,
        subworkflowId: 'sub-1',
        depth: 2,
        variableInputCount: 5,
        variableOutputCount: 3,
      });
      const subgraphResult = collector.query({ metricName: 'subgraph.execution.count' });
      expect(subgraphResult.totalCount).toBe(1);
    });

    it('should warn on missing parameters', () => {
      const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
      collector.recordSubgraphExecution('', '', { success: true, duration: 0, subworkflowId: '', depth: 0, variableInputCount: 0, variableOutputCount: 0 });
      const result = collector.query({});
      expect(result.totalCount).toBe(0);
      warnSpy.mockRestore();
    });
  });

  describe('recordForkExecution', () => {
    it('should record fork execution metrics', () => {
      collector.recordForkExecution('fork-1', 'wf-1', {
        branchCount: 3,
        totalDuration: 2000,
        successCount: 2,
        failureCount: 1,
        maxBranchDuration: 1000,
        minBranchDuration: 300,
      });
      const forkResult = collector.query({ metricName: 'fork.execution.count' });
      expect(forkResult.totalCount).toBe(1);
    });

    it('should warn on missing parameters', () => {
      const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
      collector.recordForkExecution('', '', {
        branchCount: 0, totalDuration: 0, successCount: 0, failureCount: 0,
        maxBranchDuration: 0, minBranchDuration: 0,
      });
      const result = collector.query({});
      expect(result.totalCount).toBe(0);
      warnSpy.mockRestore();
    });
  });

  describe('recordForkBranchExecution', () => {
    it('should record fork branch execution', () => {
      collector.recordForkBranchExecution({
        nodeId: 'fork-1',
        forkPathId: 'path-1',
        duration: 500,
        status: 'completed',
      });
      const result = collector.query({ metricName: 'fork.branch.execution.duration' });
      expect(result.totalCount).toBe(1);
    });

    it('should warn on missing parameters', () => {
      const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
      collector.recordForkBranchExecution({ nodeId: '', forkPathId: '', duration: 0, status: '' });
      const result = collector.query({});
      expect(result.totalCount).toBe(0);
      warnSpy.mockRestore();
    });
  });

  describe('getSubgraphExecutionStats', () => {
    it('should return empty stats when no subgraph executions', () => {
      const stats = collector.getSubgraphExecutionStats();
      expect(stats).toEqual({});
    });
  });

  describe('toPrometheus', () => {
    it('should export node metrics in Prometheus format', () => {
      collector.recordNodeExecution('node-1', 'LLM', 'wf-1', {
        success: true,
        duration: 500,
      });
      const lines = collector.toPrometheus();
      expect(lines.length).toBeGreaterThan(0);
    });
  });

  describe('toJSON', () => {
    it('should export node metrics as JSON', () => {
      const json = collector.toJSON();
      expect(json).toHaveProperty('type', 'node');
    });
  });
});