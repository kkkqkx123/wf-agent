/**
 * Tool Call Metrics Collector
 * 
 * Collects and aggregates metrics specific to tool executions including:
 * - Tool call duration distribution
 * - Success/failure rates
 * - Parameter and result sizes
 * - Error distributions by tool
 */

import { BaseMetricCollector } from "./base-collector.js";
import type { MetricCollectorConfig, MetricFilter, MetricQueryResult } from "./types.js";
import { TOOL_METRICS } from "./constants.js";
import { PrometheusFormatter, type PrometheusMetric } from "./utils/prometheus-formatter.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ operation: "ToolMetricsCollector" });

/**
 * Tool-specific metric collector
 * Extends BaseMetricCollector with tool-specific convenience methods
 */
export class ToolMetricsCollector extends BaseMetricCollector {
  constructor(config?: MetricCollectorConfig) {
    super(config);
  }

  /**
   * Record tool call start
   * @param toolId Tool ID/name
   * @param executionId Execution ID
   */
  recordToolCallStart(toolId: string, executionId: string): void {
    this.incrementCounter(TOOL_METRICS.CALL_COUNT, {
      tool_id: toolId,
      execution_id: executionId,
      status: "started",
    });
  }

  /**
   * Record tool call completion
   * @param toolId Tool ID/name
   * @param executionId Execution ID
   * @param duration Execution duration in milliseconds
   * @param success Whether execution was successful
   * @param parameterSize Size of parameters in bytes (optional)
   * @param resultSize Size of result in bytes (optional)
   */
  recordToolCallComplete(
    toolId: string,
    executionId: string,
    duration: number,
    success: boolean,
    parameterSize?: number,
    resultSize?: number,
  ): void {
    // Record execution duration distribution
    this.observeHistogram(TOOL_METRICS.CALL_DURATION, duration, {
      tool_id: toolId,
      execution_id: executionId,
      success: success.toString(),
    });

    // Record errors
    if (!success) {
      this.incrementCounter(TOOL_METRICS.ERROR_COUNT, {
        tool_id: toolId,
        execution_id: executionId,
        error_type: "execution_failed",
      });
    }

    // Record parameter size if provided
    if (parameterSize !== undefined) {
      this.setGauge(TOOL_METRICS.PARAMETER_SIZE, parameterSize, {
        tool_id: toolId,
        execution_id: executionId,
      });
    }

    // Record result size if provided
    if (resultSize !== undefined) {
      this.setGauge(TOOL_METRICS.RESULT_SIZE, resultSize, {
        tool_id: toolId,
        execution_id: executionId,
      });
    }
  }

  /**
   * Record tool call error with details
   * @param toolId Tool ID/name
   * @param executionId Execution ID
   * @param errorType Error type/category
   * @param errorMessage Optional error message
   */
  recordToolError(
    toolId: string,
    executionId: string,
    errorType: string,
    errorMessage?: string,
  ): void {
    const labels: Record<string, string> = {
      tool_id: toolId,
      execution_id: executionId,
      error_type: errorType,
    };

    if (errorMessage) {
      // Truncate long messages to avoid excessive label size
      labels['error_message'] = errorMessage.length > 200 ? errorMessage.substring(0, 200) + '...' : errorMessage;
    }

    this.incrementCounter(TOOL_METRICS.ERROR_COUNT, labels);
  }

  /**
   * Get tool-specific statistics
   * @param toolId Optional tool ID filter
   * @returns Aggregated statistics
   */
  getToolStats(toolId?: string): MetricQueryResult {
    const filter: MetricFilter = toolId
      ? {
          labels: { tool_id: toolId },
        }
      : {};

    return this.query(filter);
  }

  /**
   * Get tool performance summary
   * Returns average duration, success rate, and error count for each tool
   * @returns Map of tool ID to performance metrics
   */
  getToolPerformanceSummary(): Map<string, {
    avgDuration: number;
    successRate: number;
    totalCalls: number;
    errorCount: number;
  }> {
    const result = this.query({});
    const summary = new Map<string, {
      avgDuration: number;
      successRate: number;
      totalCalls: number;
      errorCount: number;
    }>();

    // Aggregate by tool_id
    for (const [metricName, aggregated] of result.metrics.entries()) {
      if (metricName === TOOL_METRICS.CALL_DURATION && aggregated.byLabel) {
        for (const [labelKey, labelAgg] of aggregated.byLabel.entries()) {
          try {
            const labels = JSON.parse(labelKey);
            const toolId = labels.tool_id;
            if (!toolId) continue;

            if (!summary.has(toolId)) {
              summary.set(toolId, {
                avgDuration: 0,
                successRate: 0,
                totalCalls: 0,
                errorCount: 0,
              });
            }

            const toolStats = summary.get(toolId)!;
            
            // Calculate average duration from time series
            if (aggregated.timeSeries && aggregated.timeSeries.length > 0) {
              const totalDuration = aggregated.timeSeries.reduce(
                (sum, ts) => sum + ts.value,
                0,
              );
              toolStats.avgDuration = totalDuration / aggregated.timeSeries.length;
            }

            toolStats.totalCalls += labelAgg.value;
          } catch (error) {
            logger.warn("Failed to parse label key", { labelKey, error });
          }
        }
      }
    }

    return summary;
  }

  /**
   * Export as Prometheus format
   */
  toPrometheus(): string[] {
    const summary = this.getToolPerformanceSummary();
    const metrics: PrometheusMetric[] = [];
    
    // Total tool calls counter
    let totalCalls = 0;
    for (const [, stats] of summary) {
      totalCalls += stats.totalCalls;
    }
    
    metrics.push({
      name: 'tool_call_total',
      type: 'counter',
      help: 'Total tool calls',
      samples: [{ value: totalCalls }]
    });
    
    // Tool calls by tool_id
    for (const [toolId, stats] of summary) {
      metrics.push({
        name: 'tool_call_by_tool_total',
        type: 'counter',
        help: 'Tool calls grouped by tool ID',
        samples: [{ labels: { tool_id: toolId }, value: stats.totalCalls }]
      });
      
      // Average duration per tool
      metrics.push({
        name: 'tool_call_duration_seconds',
        type: 'gauge',
        help: 'Average tool call duration in seconds',
        samples: [{ labels: { tool_id: toolId }, value: stats.avgDuration / 1000 }]
      });
      
      // Error count per tool
      if (stats.errorCount > 0) {
        metrics.push({
          name: 'tool_call_error_total',
          type: 'counter',
          help: 'Tool call errors',
          samples: [{ labels: { tool_id: toolId }, value: stats.errorCount }]
        });
      }
    }
    
    // Format all metrics
    return metrics.flatMap(m => PrometheusFormatter.formatMetric(m));
  }
  
  /**
   * Export as JSON
   */
  toJSON(): Record<string, unknown> {
    const summary = this.getToolPerformanceSummary();
    const toolsData: Record<string, unknown> = {};
    
    for (const [toolId, stats] of summary) {
      toolsData[toolId] = {
        totalCalls: stats.totalCalls,
        avgDuration: stats.avgDuration,
        successRate: stats.successRate,
        errorCount: stats.errorCount
      };
    }
    
    return {
      type: 'tool',
      tools: toolsData
    };
  }
}
