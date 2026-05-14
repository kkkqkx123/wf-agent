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
    this.incrementCounter('workflow.execution.count', {
      workflow_id: workflowId,
      workflow_version: labels?.version || 'unknown',
      execution_type: labels?.executionType || 'MAIN',
      execution_id: executionId,
    });

    // Track active executions
    this.incrementCounter('workflow.execution.active.count', {
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
    this.incrementCounter('workflow.execution.active.count', {
      workflow_id: workflowId,
    }, -1);

    // Record duration histogram
    this.observeHistogram('workflow.execution.duration', result.duration, {
      workflow_id: workflowId,
    });

    // Record node count
    this.observeHistogram('workflow.execution.node_count', result.nodeCount, {
      workflow_id: workflowId,
    });

    // Record success or failure
    if (result.success) {
      this.incrementCounter('workflow.execution.success.count', {
        workflow_id: workflowId,
      });
    } else {
      this.incrementCounter('workflow.execution.failure.count', {
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
      metricName: 'workflow.execution.count',
      metricType: 'counter',
      ...filter,
    });

    // Query success/failure counts
    const successResult = this.query({
      metricName: 'workflow.execution.success.count',
      metricType: 'counter',
      ...filter,
    });

    const failureResult = this.query({
      metricName: 'workflow.execution.failure.count',
      metricType: 'counter',
      ...filter,
    });

    // Query duration histogram
    const durationResult = this.query({
      metricName: 'workflow.execution.duration',
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
    const countMetric = countResult.metrics.get('workflow.execution.count');
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
      successRate,
      avgDuration,
      p95Duration,
      p99Duration,
      byVersion,
    };
  }

  /**
   * Flush buffered metrics
   */
  async flush(): Promise<void> {
    const flushedCount = this.metricsBuffer.length;
    
    if (flushedCount > 0) {
      logger.debug("Flushing workflow metrics", { flushedCount });
      // TODO: Implement actual persistence (e.g., write to database, send to monitoring service)
      // For now, just clear the buffer
      this.metricsBuffer = [];
    }
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
      metricName: 'workflow.execution.count',
      metricType: 'counter',
    });

    const workflows: Map<string, { count: number; success: number; failure: number }> = new Map();

    // Aggregate by workflow_id
    const countMetric = countResult.metrics.get('workflow.execution.count');
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
      metricName: 'workflow.execution.success.count',
      metricType: 'counter',
    }).metrics.get('workflow.execution.success.count');

    if (successMetric) {
      for (const [labelKey, labelAgg] of successMetric.byLabel.entries()) {
        try {
          const labels = JSON.parse(labelKey);
          if (labels.workflow_id && workflows.has(labels.workflow_id)) {
            workflows.get(labels.workflow_id)!.success += labelAgg.value;
          }
        } catch (error) {
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
}
