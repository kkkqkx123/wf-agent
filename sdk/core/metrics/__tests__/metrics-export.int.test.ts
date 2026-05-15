/**
 * Metrics Export Integration Test
 * 
 * Tests the complete metrics export flow from collectors to API
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { WorkflowMetricsCollector } from '../workflow-collector.js';
import { NodeMetricsCollector } from '../node-collector.js';
import { AgentMetricsCollector } from '../agent-collector.js';
import { EventMetricsCollector } from '../event-collector.js';
import { PrometheusFormatter } from '../utils/prometheus-formatter.js';

describe('Metrics Export Integration', () => {
  let workflowCollector: WorkflowMetricsCollector;
  let nodeCollector: NodeMetricsCollector;
  let agentCollector: AgentMetricsCollector;
  let eventCollector: EventMetricsCollector;

  beforeEach(() => {
    workflowCollector = new WorkflowMetricsCollector();
    nodeCollector = new NodeMetricsCollector();
    agentCollector = new AgentMetricsCollector();
    eventCollector = new EventMetricsCollector();
  });

  it('should export all collectors to Prometheus format and combine them', () => {
    // Record some workflow metrics
    workflowCollector.recordExecutionStart('wf-1', 'exec-1', { version: '1.0' });
    workflowCollector.recordExecutionComplete('wf-1', 'exec-1', {
      success: true,
      duration: 1000,
      nodeCount: 5
    });

    // Record some node metrics
    nodeCollector.recordTemplateInstantiation('llm-template', 'LLM');
    nodeCollector.recordNodeExecutionStart('node-1', 'LLM', 'wf-1');
    nodeCollector.recordNodeExecution('node-1', 'LLM', 'wf-1', {
      success: true,
      duration: 500
    });

    // Record some agent metrics
    agentCollector.recordExecutionStart('profile-1', 'config-1', 'agent-exec-1');
    agentCollector.recordExecutionComplete('profile-1', {
      iterations: 3,
      toolCallCount: 2,
      duration: 2000,
      success: true
    });

    // Record some event metrics
    eventCollector.recordEvent('NODE_COMPLETED', 'exec-1', {
      workflow_id: 'wf-1',
      node_type: 'LLM'
    });

    // Export each collector
    const workflowLines = workflowCollector.toPrometheus();
    const nodeLines = nodeCollector.toPrometheus();
    const agentLines = agentCollector.toPrometheus();
    const eventLines = eventCollector.toPrometheus();

    // Combine all metrics
    const allMetrics = [workflowLines, nodeLines, agentLines, eventLines];
    const combined = PrometheusFormatter.combine(allMetrics);

    // Verify combined output
    expect(combined).toContain('workflow_execution_total');
    expect(combined).toContain('node_execution_total');
    expect(combined).toContain('agent_loop_execution_total');
    expect(combined).toContain('event_published_total');
    expect(combined).toContain('# Generated at');
    expect(combined.endsWith('\n')).toBe(true);
  });

  it('should export all collectors to JSON format', () => {
    // Record some metrics
    workflowCollector.recordExecutionStart('wf-1', 'exec-1');
    workflowCollector.recordExecutionComplete('wf-1', 'exec-1', {
      success: true,
      duration: 1000,
      nodeCount: 5
    });

    nodeCollector.recordTemplateInstantiation('test-template', 'LLM');
    
    agentCollector.recordExecutionStart('profile-1', 'config-1', 'agent-exec-1');
    agentCollector.recordExecutionComplete('profile-1', {
      iterations: 2,
      toolCallCount: 1,
      duration: 1500,
      success: true
    });

    eventCollector.recordEvent('WORKFLOW_STARTED', 'exec-1');

    // Export each collector to JSON
    const workflowJson = workflowCollector.toJSON();
    const nodeJson = nodeCollector.toJSON();
    const agentJson = agentCollector.toJSON();
    const eventJson = eventCollector.toJSON();

    // Verify JSON structure
    expect(workflowJson).toHaveProperty('type', 'workflow');
    expect(nodeJson).toHaveProperty('type', 'node');
    expect(agentJson).toHaveProperty('type', 'agent');
    expect(eventJson).toHaveProperty('type', 'event');

    // Verify all are JSON serializable
    const combined = {
      timestamp: Date.now(),
      workflow: workflowJson,
      node: nodeJson,
      agent: agentJson,
      event: eventJson
    };

    const serialized = JSON.stringify(combined, null, 2);
    expect(() => JSON.parse(serialized)).not.toThrow();
  });

  it('should handle empty collectors gracefully', () => {
    const workflowLines = workflowCollector.toPrometheus();
    const nodeLines = nodeCollector.toPrometheus();
    const agentLines = agentCollector.toPrometheus();
    const eventLines = eventCollector.toPrometheus();

    const allMetrics = [workflowLines, nodeLines, agentLines, eventLines];
    const combined = PrometheusFormatter.combine(allMetrics);

    // Should still produce valid output with timestamp
    expect(combined).toContain('# Generated at');
    expect(typeof combined).toBe('string');
  });

  it('should maintain metric consistency between exports', () => {
    // Record metrics
    workflowCollector.recordExecutionStart('wf-1', 'exec-1', { version: '2.0' });
    workflowCollector.recordExecutionComplete('wf-1', 'exec-1', {
      success: false,
      duration: 3000,
      nodeCount: 10,
      errorType: 'TIMEOUT'
    });

    // Export twice
    const prometheus1 = workflowCollector.toPrometheus();
    const prometheus2 = workflowCollector.toPrometheus();

    // Both exports should be identical (no state mutation)
    expect(prometheus1).toEqual(prometheus2);

    // JSON exports should also be consistent
    const json1 = workflowCollector.toJSON();
    const json2 = workflowCollector.toJSON();
    expect(JSON.stringify(json1)).toBe(JSON.stringify(json2));
  });
});
