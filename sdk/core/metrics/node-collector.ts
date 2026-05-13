/**
 * Node Execution Metrics Collector
 * 
 * Collects and aggregates metrics specific to node executions including:
 * - Node execution duration distribution
 * - Success/failure rates by node type
 * - Retry counts
 * - Input/output sizes
 */

import { BaseMetricCollector } from "./base-collector.js";
import type { MetricCollectorConfig, MetricFilter, MetricQueryResult } from "./types.js";
import { NODE_METRICS } from "./constants.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ operation: "NodeMetricsCollector" });

/**
 * Node-specific metric collector
 * Extends BaseMetricCollector with node-specific convenience methods
 */
export class NodeMetricsCollector extends BaseMetricCollector {
  constructor(config?: MetricCollectorConfig) {
    super(config);
  }

  /**
   * Record node execution start
   * @param nodeId Node ID
   * @param nodeType Node type (e.g., LLM, Tool, Condition)
   * @param workflowId Workflow ID
   * @param executionId Execution ID
   */
  recordNodeStart(
    nodeId: string,
    nodeType: string,
    workflowId: string,
    executionId: string,
  ): void {
    this.incrementCounter(NODE_METRICS.EXECUTION_COUNT, {
      workflow_id: workflowId,
      execution_id: executionId,
      node_id: nodeId,
      node_type: nodeType,
      status: "started",
    });
  }

  /**
   * Record node execution completion
   * @param nodeId Node ID
   * @param nodeType Node type
   * @param workflowId Workflow ID
   * @param executionId Execution ID
   * @param duration Execution duration in milliseconds
   * @param success Whether execution was successful
   * @param inputSize Size of input data in bytes (optional)
   * @param outputSize Size of output data in bytes (optional)
   */
  recordNodeComplete(
    nodeId: string,
    nodeType: string,
    workflowId: string,
    executionId: string,
    duration: number,
    success: boolean,
    inputSize?: number,
    outputSize?: number,
  ): void {
    // Record execution duration distribution
    this.observeHistogram(NODE_METRICS.EXECUTION_DURATION, duration, {
      workflow_id: workflowId,
      execution_id: executionId,
      node_id: nodeId,
      node_type: nodeType,
      success: success.toString(),
    });

    // Record errors
    if (!success) {
      this.incrementCounter(NODE_METRICS.ERROR_COUNT, {
        workflow_id: workflowId,
        execution_id: executionId,
        node_id: nodeId,
        node_type: nodeType,
        error_type: "execution_failed",
      });
    }

    // Record input size if provided
    if (inputSize !== undefined) {
      this.setGauge(NODE_METRICS.INPUT_SIZE, inputSize, {
        workflow_id: workflowId,
        node_id: nodeId,
        node_type: nodeType,
      });
    }

    // Record output size if provided
    if (outputSize !== undefined) {
      this.setGauge(NODE_METRICS.OUTPUT_SIZE, outputSize, {
        workflow_id: workflowId,
        node_id: nodeId,
        node_type: nodeType,
      });
    }
  }

  /**
   * Record node retry
   * @param nodeId Node ID
   * @param nodeType Node type
   * @param workflowId Workflow ID
   * @param retryAttempt Retry attempt number (1-based)
   */
  recordNodeRetry(
    nodeId: string,
    nodeType: string,
    workflowId: string,
    retryAttempt: number,
  ): void {
    this.incrementCounter(NODE_METRICS.RETRY_COUNT, {
      workflow_id: workflowId,
      node_id: nodeId,
      node_type: nodeType,
      retry_attempt: retryAttempt.toString(),
    });
  }

  /**
   * Record node error with details
   * @param nodeId Node ID
   * @param nodeType Node type
   * @param workflowId Workflow ID
   * @param executionId Execution ID
   * @param errorType Error type/category
   */
  recordNodeError(
    nodeId: string,
    nodeType: string,
    workflowId: string,
    executionId: string,
    errorType: string,
  ): void {
    this.incrementCounter(NODE_METRICS.ERROR_COUNT, {
      workflow_id: workflowId,
      execution_id: executionId,
      node_id: nodeId,
      node_type: nodeType,
      error_type: errorType,
    });
  }

  /**
   * Get node-specific statistics
   * @param nodeId Optional node ID filter
   * @param nodeType Optional node type filter
   * @returns Aggregated statistics
   */
  getNodeStats(nodeId?: string, nodeType?: string): MetricQueryResult {
    const labels: Record<string, string> = {};
    
    if (nodeId) {
      labels['node_id'] = nodeId;
    }
    
    if (nodeType) {
      labels['node_type'] = nodeType;
    }

    const filter: MetricFilter = Object.keys(labels).length > 0 ? { labels } : {};
    return this.query(filter);
  }

  /**
   * Get node performance summary by type
   * Returns average duration, success rate, and error count for each node type
   * @returns Map of node type to performance metrics
   */
  getNodePerformanceByType(): Map<string, {
    avgDuration: number;
    successRate: number;
    totalExecutions: number;
    errorCount: number;
  }> {
    const result = this.query({});
    const summary = new Map<string, {
      avgDuration: number;
      successRate: number;
      totalExecutions: number;
      errorCount: number;
    }>();

    // Aggregate by node_type
    for (const [metricName, aggregated] of result.metrics.entries()) {
      if (metricName === NODE_METRICS.EXECUTION_DURATION && aggregated.byLabel) {
        for (const [labelKey, labelAgg] of aggregated.byLabel.entries()) {
          try {
            const labels = JSON.parse(labelKey);
            const nodeType = labels.node_type;
            if (!nodeType) continue;

            if (!summary.has(nodeType)) {
              summary.set(nodeType, {
                avgDuration: 0,
                successRate: 0,
                totalExecutions: 0,
                errorCount: 0,
              });
            }

            const nodeStats = summary.get(nodeType)!;
            
            // Calculate average duration from time series
            if (aggregated.timeSeries && aggregated.timeSeries.length > 0) {
              const totalDuration = aggregated.timeSeries.reduce(
                (sum, ts) => sum + ts.value,
                0,
              );
              nodeStats.avgDuration = totalDuration / aggregated.timeSeries.length;
            }

            nodeStats.totalExecutions += labelAgg.value;
          } catch (error) {
            logger.warn("Failed to parse label key", { labelKey, error });
          }
        }
      }
    }

    return summary;
  }

  /**
   * Flush metrics to storage
   * Override to implement custom persistence logic
   */
  async flush(): Promise<void> {
    const flushedCount = this.metricsBuffer.length;

    if (flushedCount > 0) {
      logger.debug("Flushing node metrics", { flushedCount });

      // TODO: Implement actual persistence (e.g., write to database, send to monitoring service)
      // For now, just clear the buffer
      this.metricsBuffer = [];
    }
  }
}
