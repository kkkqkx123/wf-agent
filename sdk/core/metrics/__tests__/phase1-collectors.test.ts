/**
 * Phase 1 Metrics Collectors - Basic Functionality Test
 * 
 * Tests the core functionality of the new metrics collectors:
 * - WorkflowMetricsCollector
 * - NodeMetricsCollector  
 * - AgentMetricsCollector
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { WorkflowMetricsCollector } from '../workflow-metrics-collector.js';
import { NodeMetricsCollector } from '../node-metrics-collector.js';
import { AgentMetricsCollector } from '../agent-metrics-collector.js';

describe('Phase 1: Metrics Collectors', () => {
  describe('WorkflowMetricsCollector', () => {
    let collector: WorkflowMetricsCollector;

    beforeEach(() => {
      collector = new WorkflowMetricsCollector();
    });

    it('should record execution start and complete', () => {
      // Record execution start
      collector.recordExecutionStart('workflow-1', 'exec-1', {
        version: '1.0',
        executionType: 'MAIN',
      });

      // Record execution complete
      collector.recordExecutionComplete('workflow-1', 'exec-1', {
        success: true,
        duration: 3200,
        nodeCount: 5,
      });

      // Verify stats
      const stats = collector.getWorkflowUsageStats('workflow-1');
      expect(stats.totalExecutions).toBe(1);
      expect(stats.successRate).toBe(1.0);
      expect(stats.byVersion['1.0']).toBe(1);
    });

    it('should track multiple executions', () => {
      // Record multiple executions
      for (let i = 0; i < 5; i++) {
        collector.recordExecutionStart('workflow-1', `exec-${i}`, {
          version: i < 3 ? '1.0' : '2.0',
        });
        
        collector.recordExecutionComplete('workflow-1', `exec-${i}`, {
          success: i < 4, // 4 success, 1 failure
          duration: 1000 + i * 500,
          nodeCount: 5,
        });
      }

      const stats = collector.getWorkflowUsageStats('workflow-1');
      expect(stats.totalExecutions).toBe(5);
      expect(stats.successRate).toBe(0.8); // 4/5
      expect(stats.byVersion['1.0']).toBe(3);
      expect(stats.byVersion['2.0']).toBe(2);
    });

    it('should get top workflows', () => {
      // Record executions for multiple workflows
      collector.recordExecutionStart('wf-1', 'exec-1');
      collector.recordExecutionComplete('wf-1', 'exec-1', { success: true, duration: 1000, nodeCount: 3 });
      
      collector.recordExecutionStart('wf-2', 'exec-2');
      collector.recordExecutionComplete('wf-2', 'exec-2', { success: true, duration: 2000, nodeCount: 5 });
      collector.recordExecutionStart('wf-2', 'exec-3');
      collector.recordExecutionComplete('wf-2', 'exec-3', { success: true, duration: 2500, nodeCount: 5 });

      const topWorkflows = collector.getTopWorkflows(2);
      expect(topWorkflows.length).toBe(2);
      expect(topWorkflows[0].workflowId).toBe('wf-2');
      expect(topWorkflows[0].executionCount).toBe(2);
      expect(topWorkflows[1].workflowId).toBe('wf-1');
      expect(topWorkflows[1].executionCount).toBe(1);
    });

    it('should flush metrics', async () => {
      collector.recordExecutionStart('wf-1', 'exec-1');
      await collector.flush();
      
      // After flush, buffer should be cleared
      // Note: This is a basic test, actual persistence would need more verification
    });
  });

  describe('NodeMetricsCollector', () => {
    let collector: NodeMetricsCollector;

    beforeEach(() => {
      collector = new NodeMetricsCollector();
    });

    it('should record template instantiation', () => {
      collector.recordTemplateInstantiation('code-analyzer', 'LLM', {
        category: 'analysis',
      });

      collector.recordTemplateInstantiation('test-runner', 'SCRIPT', {
        category: 'testing',
      });

      const topTemplates = collector.getTopNodeTemplates(2);
      expect(topTemplates.length).toBe(2);
      expect(topTemplates[0].templateName).toBe('code-analyzer');
      expect(topTemplates[0].nodeType).toBe('LLM');
    });

    it('should record node execution', () => {
      collector.recordNodeExecutionStart('node-1', 'LLM', 'workflow-1');
      
      collector.recordNodeExecution('node-1', 'LLM', 'workflow-1', {
        success: true,
        duration: 2500,
        tokenUsage: 1500,
      });

      const stats = collector.getNodeExecutionStatsByType();
      expect(stats['LLM']).toBeDefined();
      expect(stats['LLM'].totalCount).toBe(1);
      expect(stats['LLM'].successRate).toBe(1.0);
    });

    it('should track multiple node types', () => {
      // Record different node types
      collector.recordNodeExecution('node-1', 'LLM', 'wf-1', {
        success: true,
        duration: 2000,
      });
      
      collector.recordNodeExecution('node-2', 'SCRIPT', 'wf-1', {
        success: true,
        duration: 500,
      });
      
      collector.recordNodeExecution('node-3', 'TOOL', 'wf-1', {
        success: false,
        duration: 100,
        errorType: 'timeout',
      });

      const stats = collector.getNodeExecutionStatsByType();
      expect(Object.keys(stats).length).toBe(3);
      expect(stats['LLM'].totalCount).toBe(1);
      expect(stats['SCRIPT'].totalCount).toBe(1);
      expect(stats['TOOL'].totalCount).toBe(1);
      expect(stats['TOOL'].successRate).toBe(0);
    });
  });

  describe('AgentMetricsCollector', () => {
    let collector: AgentMetricsCollector;

    beforeEach(() => {
      collector = new AgentMetricsCollector();
    });

    it('should record agent execution', () => {
      collector.recordExecutionStart('senior-dev', 'config-1', 'agent-exec-1');
      
      collector.recordExecutionComplete('senior-dev', {
        iterations: 5,
        toolCallCount: 12,
        duration: 15000,
        tokenUsage: 8000,
        success: true,
      });

      const stats = collector.getAgentStats('senior-dev');
      expect(stats.totalExecutions).toBe(1);
      expect(stats.avgIterations).toBe(5);
      expect(stats.byProfile['senior-dev']).toBe(1);
    });

    it('should record iterations and tool calls', () => {
      collector.recordIteration('profile-1', 1);
      collector.recordIteration('profile-1', 2);
      collector.recordIteration('profile-1', 3);

      collector.recordToolCall('search_code', 'profile-1', {
        success: true,
        duration: 500,
      });

      collector.recordToolCall('read_file', 'profile-1', {
        success: true,
        duration: 200,
      });

      const stats = collector.getAgentStats('profile-1');
      expect(stats.totalExecutions).toBe(0); // No execution recorded yet
      expect(stats.byProfile['profile-1']).toBeUndefined(); // No executions counted
    });

    it('should aggregate by profile', () => {
      // Record executions for multiple profiles
      collector.recordExecutionStart('profile-1', 'config-1', 'exec-1');
      collector.recordExecutionComplete('profile-1', {
        iterations: 3,
        toolCallCount: 8,
        duration: 10000,
        success: true,
      });

      collector.recordExecutionStart('profile-2', 'config-2', 'exec-2');
      collector.recordExecutionComplete('profile-2', {
        iterations: 7,
        toolCallCount: 15,
        duration: 20000,
        success: true,
      });

      collector.recordExecutionStart('profile-2', 'config-2', 'exec-3');
      collector.recordExecutionComplete('profile-2', {
        iterations: 5,
        toolCallCount: 10,
        duration: 15000,
        success: true,
      });

      const stats = collector.getAgentStats();
      expect(stats.totalExecutions).toBe(3);
      expect(stats.byProfile['profile-1']).toBe(1);
      expect(stats.byProfile['profile-2']).toBe(2);
    });
  });
});
