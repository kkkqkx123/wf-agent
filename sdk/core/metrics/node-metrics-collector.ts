/**
 * Node Metrics Collector
 * 
 * Tracks node template and execution metrics including:
 * - Template instantiation count
 * - Node execution frequency by type
 * - Execution duration and success rate
 * - Token usage for LLM nodes
 */

import { BaseMetricCollector } from "./base-collector.js";
import type { MetricCollectorConfig } from "./types.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "NodeMetricsCollector" });

export class NodeMetricsCollector extends BaseMetricCollector {
  constructor(config?: MetricCollectorConfig) {
    super(config);
  }

  /**
   * Record node template instantiation
   */
  recordTemplateInstantiation(templateName: string, nodeType: string, metadata?: {
    category?: string;
    tags?: string[];
  }): void {
    if (!templateName || !nodeType) {
      logger.warn("recordTemplateInstantiation called with missing parameters");
      return;
    }

    this.incrementCounter('node.template.instantiation.count', {
      template_name: templateName,
      node_type: nodeType,
      category: metadata?.category || 'uncategorized',
    });

    logger.debug("Recorded node template instantiation", { templateName, nodeType });
  }

  /**
   * Record node execution start
   */
  recordNodeExecutionStart(nodeId: string, nodeType: string, workflowId: string): void {
    if (!nodeId || !nodeType || !workflowId) {
      logger.warn("recordNodeExecutionStart called with missing parameters");
      return;
    }

    this.incrementCounter('node.execution.started.count', {
      node_type: nodeType,
      node_id: nodeId,
      workflow_id: workflowId,
    });
  }

  /**
   * Record node execution completion
   */
  recordNodeExecution(nodeId: string, nodeType: string, workflowId: string, result: {
    success: boolean;
    duration: number;
    tokenUsage?: number;
    errorType?: string;
  }): void {
    if (!nodeId || !nodeType || !workflowId) {
      logger.warn("recordNodeExecution called with missing parameters");
      return;
    }

    // Record execution count
    this.incrementCounter('node.execution.count', {
      node_type: nodeType,
      node_id: nodeId,
      workflow_id: workflowId,
    });

    // Record duration
    this.observeHistogram('node.execution.duration', result.duration, {
      node_type: nodeType,
      node_id: nodeId,
    });

    // Record success/failure
    if (result.success) {
      this.incrementCounter('node.execution.success.count', {
        node_type: nodeType,
      });
    } else {
      this.incrementCounter('node.execution.failure.count', {
        node_type: nodeType,
        error_type: result.errorType || 'unknown',
      });
    }

    // For LLM nodes, track token usage
    if (result.tokenUsage !== undefined && nodeType === 'LLM') {
      this.observeHistogram('node.execution.token_usage', result.tokenUsage, {
        node_type: nodeType,
      });
    }

    logger.debug("Recorded node execution", { 
      nodeId, 
      nodeType, 
      workflowId,
      success: result.success,
      duration: result.duration 
    });
  }

  /**
   * Get most frequently used node templates
   */
  getTopNodeTemplates(limit: number = 10): Array<{
    templateName: string;
    nodeType: string;
    instantiationCount: number;
  }> {
    const result = this.query({
      metricName: 'node.template.instantiation.count',
      metricType: 'counter',
    });

    const templates: Map<string, { name: string; type: string; count: number }> = new Map();

    const metric = result.metrics.get('node.template.instantiation.count');
    if (metric) {
      for (const [labelKey, labelAgg] of metric.byLabel.entries()) {
        try {
          const labels = JSON.parse(labelKey);
          if (labels.template_name && labels.node_type) {
            const key = `${labels.template_name}:${labels.node_type}`;
            templates.set(key, {
              name: labels.template_name,
              type: labels.node_type,
              count: labelAgg.value,
            });
          }
        } catch (error) {
          logger.warn("Failed to parse label key", { labelKey, error });
        }
      }
    }

    return Array.from(templates.values())
      .sort((a, b) => b.count - a.count)
      .slice(0, limit)
      .map(t => ({
        templateName: t.name,
        nodeType: t.type,
        instantiationCount: t.count,
      }));
  }

  /**
   * Get node execution statistics by type
   */
  getNodeExecutionStatsByType(): Record<string, {
    totalCount: number;
    successRate: number;
    avgDuration: number;
  }> {
    const result: Record<string, { totalCount: number; successCount: number; totalDuration: number }> = {};

    // Get counts by type
    const countResult = this.query({
      metricName: 'node.execution.count',
      metricType: 'counter',
    });

    const countMetric = countResult.metrics.get('node.execution.count');
    if (countMetric) {
      for (const [labelKey, labelAgg] of countMetric.byLabel.entries()) {
        try {
          const labels = JSON.parse(labelKey);
          if (labels.node_type) {
            const nodeType = labels.node_type;
            if (!result[nodeType]) {
              result[nodeType] = { totalCount: 0, successCount: 0, totalDuration: 0 };
            }
            result[nodeType].totalCount += labelAgg.value;
          }
        } catch (error) {
          // Ignore
        }
      }
    }

    // Get success counts
    const successResult = this.query({
      metricName: 'node.execution.success.count',
      metricType: 'counter',
    });

    const successMetric = successResult.metrics.get('node.execution.success.count');
    if (successMetric) {
      for (const [labelKey, labelAgg] of successMetric.byLabel.entries()) {
        try {
          const labels = JSON.parse(labelKey);
          if (labels.node_type) {
            const nodeType = labels.node_type;
            if (result[nodeType]) {
              result[nodeType].successCount += labelAgg.value;
            }
          }
        } catch (error) {
          // Ignore
        }
      }
    }

    // Calculate final stats
    const stats: Record<string, { totalCount: number; successRate: number; avgDuration: number }> = {};
    for (const [nodeType, data] of Object.entries(result)) {
      stats[nodeType] = {
        totalCount: data.totalCount,
        successRate: data.totalCount > 0 ? data.successCount / data.totalCount : 0,
        avgDuration: 0, // Would need to query duration histogram
      };
    }

    return stats;
  }

  /**
   * Flush buffered metrics
   */
  async flush(): Promise<void> {
    const flushedCount = this.metricsBuffer.length;
    
    if (flushedCount > 0) {
      logger.debug("Flushing node metrics", { flushedCount });
      // TODO: Implement actual persistence
      this.metricsBuffer = [];
    }
  }
}
