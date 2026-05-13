/**
 * Workflow Execution Metrics Collector
 * 
 * Collects and aggregates metrics specific to workflow executions including:
 * - Execution duration
 * - Node execution counts
 * - Success/failure rates
 * - Error distributions
 */

import { BaseMetricCollector } from "./base-collector.js";
import type { MetricCollectorConfig, MetricFilter, MetricQueryResult } from "./types.js";
import { WORKFLOW_METRICS, NODE_METRICS, ERROR_METRICS } from "./constants.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ operation: "WorkflowMetricsCollector" });

/**
 * Workflow-specific metric collector
 * Extends BaseMetricCollector with workflow-specific convenience methods
 */
export class WorkflowMetricsCollector extends BaseMetricCollector {
  constructor(config?: MetricCollectorConfig) {
    super(config);
  }

  /**
   * Record workflow execution start
   * @param workflowId Workflow ID
   * @param executionId Execution ID
   */
  recordExecutionStart(workflowId: string, executionId: string): void {
    this.incrementCounter(WORKFLOW_METRICS.EXECUTION_COUNT, {
      workflow_id: workflowId,
      execution_id: executionId,
      status: "started",
    });
  }

  /**
   * Record workflow execution completion
   * @param workflowId Workflow ID
   * @param executionId Execution ID
   * @param duration Execution duration in milliseconds
   * @param nodeCount Number of nodes executed
   * @param success Whether execution was successful
   */
  recordExecutionComplete(
    workflowId: string,
    executionId: string,
    duration: number,
    nodeCount: number,
    success: boolean,
  ): void {
    // Record execution duration
    this.observeHistogram(WORKFLOW_METRICS.EXECUTION_DURATION, duration, {
      workflow_id: workflowId,
      execution_id: executionId,
      success: success.toString(),
    });

    // Record node count
    this.setGauge(WORKFLOW_METRICS.NODE_COUNT, nodeCount, {
      workflow_id: workflowId,
      execution_id: executionId,
    });

    // Record success/failure
    this.incrementCounter(WORKFLOW_METRICS.ERROR_COUNT, {
      workflow_id: workflowId,
      execution_id: executionId,
      error_type: success ? "none" : "execution_failed",
    }, success ? 0 : 1);

    // Record success rate (gauge: 1 for success, 0 for failure)
    this.setGauge(WORKFLOW_METRICS.SUCCESS_RATE, success ? 1 : 0, {
      workflow_id: workflowId,
    });
  }

  /**
   * Record node execution
   * @param workflowId Workflow ID
   * @param executionId Execution ID
   * @param nodeId Node ID
   * @param nodeType Node type
   * @param duration Node execution duration
   * @param success Whether node execution was successful
   */
  recordNodeExecution(
    workflowId: string,
    executionId: string,
    nodeId: string,
    nodeType: string,
    duration: number,
    success: boolean,
  ): void {
    // Record node execution duration
    this.observeHistogram(NODE_METRICS.EXECUTION_DURATION, duration, {
      workflow_id: workflowId,
      execution_id: executionId,
      node_id: nodeId,
      node_type: nodeType,
      success: success.toString(),
    });

    // Record node execution count
    this.incrementCounter(NODE_METRICS.EXECUTION_COUNT, {
      workflow_id: workflowId,
      node_type: nodeType,
      success: success.toString(),
    });

    // Record errors
    if (!success) {
      this.incrementCounter(NODE_METRICS.ERROR_COUNT, {
        workflow_id: workflowId,
        node_id: nodeId,
        node_type: nodeType,
      });
    }
  }

  /**
   * Record node retry
   * @param workflowId Workflow ID
   * @param nodeId Node ID
   * @param retryCount Retry attempt number
   */
  recordNodeRetry(workflowId: string, nodeId: string, retryCount: number): void {
    this.incrementCounter(NODE_METRICS.RETRY_COUNT, {
      workflow_id: workflowId,
      node_id: nodeId,
      retry_attempt: retryCount.toString(),
    });
  }

  /**
   * Record workflow error
   * @param workflowId Workflow ID
   * @param executionId Execution ID
   * @param errorType Error type/category
   * @param nodeId Optional node ID where error occurred
   */
  recordError(
    workflowId: string,
    executionId: string,
    errorType: string,
    nodeId?: string,
  ): void {
    this.incrementCounter(ERROR_METRICS.OCCURRENCE_COUNT, {
      workflow_id: workflowId,
      execution_id: executionId,
      error_type: errorType,
      node_id: nodeId || "unknown",
    });
  }

  /**
   * Get workflow-specific statistics
   * @param workflowId Optional workflow ID filter
   * @returns Aggregated statistics
   */
  getWorkflowStats(workflowId?: string): MetricQueryResult {
    const filter: MetricFilter = workflowId
      ? {
          labels: { workflow_id: workflowId },
        }
      : {};

    return this.query(filter);
  }

  /**
   * Flush metrics to storage
   * Override to implement custom persistence logic
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
}
