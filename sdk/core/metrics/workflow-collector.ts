/**
 * Workflow Metrics Collector
 * 
 * Tracks workflow execution metrics including:
 * - Execution count by workflow ID and version
 * - Success/failure rates
 * - Execution duration distributions
 * - Active execution counts
 */

import { BaseMetricCollector } from "./base-collector.js";
import type { MetricCollectorConfig } from "./types.js";
import { WORKFLOW_METRICS } from "./constants.js";
import { PrometheusFormatter, type PrometheusMetric } from "./utils/prometheus-formatter.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "WorkflowMetricsCollector" });

export class WorkflowMetricsCollector extends BaseMetricCollector {
  constructor(config?: MetricCollectorConfig) {
    super(config);
  }

  /**
   * Record workflow execution start
   */
  recordExecutionStart(workflowId: string, executionId: string, labels?: {
    version?: string;
    executionType?: 'MAIN' | 'FORK_JOIN' | 'TRIGGERED_SUBWORKFLOW';
  }): void {
    if (!workflowId || !executionId) {
      logger.warn("recordExecutionStart called with missing parameters", { workflowId, executionId });
      return;
    }

    // Increment execution count
    this.incrementCounter(WORKFLOW_METRICS.EXECUTION_COUNT, {
      workflow_id: workflowId,
      workflow_version: labels?.version || 'unknown',
      execution_type: labels?.executionType || 'MAIN',
      execution_id: executionId,
    });

    // Track active executions
    this.incrementCounter(WORKFLOW_METRICS.ACTIVE_COUNT, {
      workflow_id: workflowId,
    });

    logger.debug("Recorded workflow execution start", { workflowId, executionId });
  }

  /**
   * Record workflow execution completion
   */
  recordExecutionComplete(workflowId: string, executionId: string, result: {
    success: boolean;
    duration: number;
    nodeCount: number;
    errorType?: string;
  }): void {
    if (!workflowId || !executionId) {
      logger.warn("recordExecutionComplete called with missing parameters");
      return;
    }

    // Decrement active executions
    this.incrementCounter(WORKFLOW_METRICS.ACTIVE_COUNT, {
      workflow_id: workflowId,
    }, -1);

    // Record duration histogram
    this.observeHistogram(WORKFLOW_METRICS.EXECUTION_DURATION, result.duration, {
      workflow_id: workflowId,
    });

    // Record node count
    this.observeHistogram(WORKFLOW_METRICS.NODE_COUNT, result.nodeCount, {
      workflow_id: workflowId,
    });

    // Record success or failure
    if (result.success) {
      this.incrementCounter(WORKFLOW_METRICS.SUCCESS_COUNT, {
        workflow_id: workflowId,
      });
    } else {
      this.incrementCounter(WORKFLOW_METRICS.FAILURE_COUNT, {
        workflow_id: workflowId,
        error_type: result.errorType || 'unknown',
      });
    }

    logger.debug("Recorded workflow execution complete", { 
      workflowId, 
      executionId, 
      success: result.success,
      duration: result.duration 
    });
  }

  /**
   * Get workflow usage statistics
   */
  getWorkflowUsageStats(workflowId?: string): {
    totalExecutions: number;
    successCount: number;
    failureCount: number;
    successRate: number;
    avgDuration: number;
    p95Duration: number;
    p99Duration: number;
    byVersion: Record<string, number>;
  } {
    const filter = workflowId ? {
      labels: { workflow_id: workflowId }
    } : {};

    // Query execution count
    const countResult = this.query({
      metricName: WORKFLOW_METRICS.EXECUTION_COUNT,
      metricType: 'counter',
      ...filter,
    });

    // Query success/failure counts
    const successResult = this.query({
      metricName: WORKFLOW_METRICS.SUCCESS_COUNT,
      metricType: 'counter',
      ...filter,
    });

    const failureResult = this.query({
      metricName: WORKFLOW_METRICS.FAILURE_COUNT,
      metricType: 'counter',
      ...filter,
    });

    // Query duration histogram
    const durationResult = this.query({
      metricName: WORKFLOW_METRICS.EXECUTION_DURATION,
      metricType: 'histogram',
      ...filter,
    });

    const totalExecutions = countResult.totalCount;
    const successCount = successResult.totalCount;
    const failureCount = failureResult.totalCount;
    const successRate = totalExecutions > 0 ? successCount / totalExecutions : 0;

    // Calculate duration statistics from histogram
    let avgDuration = 0;
    let p95Duration = 0;
    let p99Duration = 0;

    const durationMetric = durationResult.metrics.get('workflow.execution.duration');
    if (durationMetric && durationMetric.timeSeries && durationMetric.timeSeries.length > 0) {
      const values = durationMetric.timeSeries.map(ts => ts.value).sort((a, b) => a - b);
      avgDuration = values.reduce((sum, v) => sum + v, 0) / values.length;
      p95Duration = values[Math.floor(values.length * 0.95)] || 0;
      p99Duration = values[Math.floor(values.length * 0.99)] || 0;
    }

    // Group by version
    const byVersion: Record<string, number> = {};
    const countMetric = countResult.metrics.get(WORKFLOW_METRICS.EXECUTION_COUNT);
    if (countMetric) {
      for (const [labelKey, labelAgg] of countMetric.byLabel.entries()) {
        try {
          const labels = JSON.parse(labelKey);
          if (labels.workflow_version) {
            byVersion[labels.workflow_version] = (byVersion[labels.workflow_version] || 0) + labelAgg.value;
          }
        } catch (error) {
          logger.warn("Failed to parse label key", { labelKey, error });
        }
      }
    }

    return {
      totalExecutions,
      successCount,
      failureCount,
      successRate,
      avgDuration,
      p95Duration,
      p99Duration,
      byVersion,
    };
  }

  /**
   * Get top workflows by execution count
   */
  getTopWorkflows(limit: number = 10): Array<{
    workflowId: string;
    executionCount: number;
    successRate: number;
  }> {
    const countResult = this.query({
      metricName: WORKFLOW_METRICS.EXECUTION_COUNT,
      metricType: 'counter',
    });

    const workflows: Map<string, { count: number; success: number; failure: number }> = new Map();

    // Aggregate by workflow_id
    const countMetric = countResult.metrics.get(WORKFLOW_METRICS.EXECUTION_COUNT);
    if (countMetric) {
      for (const [labelKey, labelAgg] of countMetric.byLabel.entries()) {
        try {
          const labels = JSON.parse(labelKey);
          if (labels.workflow_id) {
            const wfId = labels.workflow_id;
            if (!workflows.has(wfId)) {
              workflows.set(wfId, { count: 0, success: 0, failure: 0 });
            }
            workflows.get(wfId)!.count += labelAgg.value;
          }
        } catch (error) {
          logger.warn("Failed to parse label key", { labelKey, error });
        }
      }
    }

    // Get success counts
    const successMetric = this.query({
      metricName: WORKFLOW_METRICS.SUCCESS_COUNT,
      metricType: 'counter',
    }).metrics.get(WORKFLOW_METRICS.SUCCESS_COUNT);

    if (successMetric) {
      for (const [labelKey, labelAgg] of successMetric.byLabel.entries()) {
        try {
          const labels = JSON.parse(labelKey);
          if (labels.workflow_id && workflows.has(labels.workflow_id)) {
            workflows.get(labels.workflow_id)!.success += labelAgg.value;
          }
        } catch (_error) {
          // Ignore parsing errors
        }
      }
    }

    // Sort and return top N
    return Array.from(workflows.entries())
      .map(([workflowId, stats]) => ({
        workflowId,
        executionCount: stats.count,
        successRate: stats.count > 0 ? stats.success / stats.count : 0,
      }))
      .sort((a, b) => b.executionCount - a.executionCount)
      .slice(0, limit);
  }

  /**
   * Export workflow metrics in Prometheus format
   */
  toPrometheus(): string[] {
    const stats = this.getWorkflowUsageStats();
    const metrics: PrometheusMetric[] = [];
    
    // Total executions counter
    metrics.push({
      name: 'workflow_execution_total',
      type: 'counter',
      help: 'Total workflow executions',
      samples: [{ value: stats.totalExecutions }]
    });
    
    // Success rate gauge
    metrics.push({
      name: 'workflow_execution_success_rate',
      type: 'gauge',
      help: 'Workflow execution success rate (0-1)',
      samples: [{ value: stats.successRate }]
    });
    
    // Duration summary with quantiles
    metrics.push({
      name: 'workflow_execution_duration_seconds',
      type: 'summary',
      help: 'Workflow execution duration in seconds',
      samples: [
        { labels: { quantile: '0.5' }, value: stats.avgDuration / 1000 },
        { labels: { quantile: '0.95' }, value: stats.p95Duration / 1000 },
        { labels: { quantile: '0.99' }, value: stats.p99Duration / 1000 },
      ]
    });
    
    // Executions by version
    for (const [version, count] of Object.entries(stats.byVersion)) {
      metrics.push({
        name: 'workflow_execution_by_version_total',
        type: 'counter',
        help: 'Workflow executions grouped by version',
        samples: [{ labels: { version }, value: count }]
      });
    }
    
    // Format all metrics
    return metrics.flatMap(m => PrometheusFormatter.formatMetric(m));
  }
  
  /**
   * Export as JSON
   */
  toJSON(): Record<string, unknown> {
    return {
      type: 'workflow',
      stats: this.getWorkflowUsageStats(),
      topWorkflows: this.getTopWorkflows(10)
    };
  }
}
