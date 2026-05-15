/**
 * Error Metrics Collector
 * 
 * Collects and aggregates metrics related to errors and exceptions including:
 * - Error occurrence counts by type
 * - Error recovery rates
 * - Affected executions count
 * - Error distribution patterns
 */

import { BaseMetricCollector } from "./base-collector.js";
import type { MetricCollectorConfig, MetricFilter, MetricQueryResult } from "./types.js";
import { ERROR_METRICS } from "./constants.js";
import { PrometheusFormatter, type PrometheusMetric } from "./utils/prometheus-formatter.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ operation: "ErrorMetricsCollector" });

/**
 * Error-specific metric collector
 * Extends BaseMetricCollector with error-specific convenience methods
 */
export class ErrorMetricsCollector extends BaseMetricCollector {
  constructor(config?: MetricCollectorConfig) {
    super(config);
  }

  /**
   * Record error occurrence
   * @param errorType Error type/category (e.g., "LLM_ERROR", "TOOL_ERROR", "NODE_ERROR")
   * @param executionId Execution ID where error occurred
   * @param nodeId Optional node ID where error occurred
   * @param errorMessage Optional error message
   */
  recordError(
    errorType: string,
    executionId: string,
    nodeId?: string,
    errorMessage?: string,
  ): void {
    const labels: Record<string, string> = {
      error_type: errorType,
      execution_id: executionId,
    };

    if (nodeId) {
      labels['node_id'] = nodeId;
    }

    if (errorMessage) {
      // Truncate long messages to avoid excessive label size
      labels['error_message'] = errorMessage.length > 200 ? errorMessage.substring(0, 200) + '...' : errorMessage;
    }

    this.incrementCounter(ERROR_METRICS.OCCURRENCE_COUNT, labels);
  }

  /**
   * Record successful error recovery
   * @param errorType Error type that was recovered from
   * @param executionId Execution ID
   */
  recordErrorRecovery(errorType: string, executionId: string): void {
    this.incrementCounter("error.recovery.count", {
      error_type: errorType,
      execution_id: executionId,
      status: "recovered",
    });
  }

  /**
   * Record failed error recovery
   * @param errorType Error type that failed to recover
   * @param executionId Execution ID
   */
  recordErrorRecoveryFailure(errorType: string, executionId: string): void {
    this.incrementCounter("error.recovery.count", {
      error_type: errorType,
      execution_id: executionId,
      status: "failed",
    });
  }

  /**
   * Record affected execution due to error
   * @param executionId Execution ID
   * @param errorType Error type
   */
  recordAffectedExecution(executionId: string, errorType: string): void {
    this.setGauge(ERROR_METRICS.AFFECTED_EXECUTIONS, 1, {
      execution_id: executionId,
      error_type: errorType,
    });
  }

  /**
   * Calculate and record error recovery rate
   * @param errorType Error type
   * @param recoveryRate Recovery rate (0-1)
   */
  recordRecoveryRate(errorType: string, recoveryRate: number): void {
    this.setGauge(ERROR_METRICS.RECOVERY_RATE, recoveryRate, {
      error_type: errorType,
    });
  }

  /**
   * Get error statistics by type
   * @param errorType Optional error type filter
   * @returns Aggregated statistics
   */
  getErrorStats(errorType?: string): MetricQueryResult {
    const filter: MetricFilter = errorType
      ? {
          labels: { error_type: errorType },
        }
      : {};

    return this.query(filter);
  }

  /**
   * Get comprehensive error summary
   * @returns Summary of all errors with breakdown by type
   */
  getErrorSummary(): {
    totalErrors: number;
    byType: Map<string, {
      count: number;
      affectedExecutions: number;
      recoveryRate: number;
    }>;
    topErrors: Array<{
      errorType: string;
      count: number;
    }>;
  } {
    const result = this.query({});
    
    let totalErrors = 0;
    const byType = new Map<string, {
      count: number;
      affectedExecutions: number;
      recoveryRate: number;
    }>();
    const errorCounts = new Map<string, number>();

    for (const [metricName, aggregated] of result.metrics.entries()) {
      if (metricName === ERROR_METRICS.OCCURRENCE_COUNT) {
        totalErrors += aggregated.value;

        // Aggregate by error_type
        for (const [labelKey, labelAgg] of aggregated.byLabel.entries()) {
          try {
            const labels = JSON.parse(labelKey);
            const errorType = labels.error_type;
            if (!errorType) continue;

            if (!byType.has(errorType)) {
              byType.set(errorType, {
                count: 0,
                affectedExecutions: 0,
                recoveryRate: 0,
              });
            }

            const errorStats = byType.get(errorType)!;
            errorStats.count += labelAgg.value;

            // Track for top errors
            errorCounts.set(errorType, (errorCounts.get(errorType) || 0) + labelAgg.value);
          } catch (error) {
            logger.warn("Failed to parse label key", { labelKey, error });
          }
        }
      }
    }

    // Get top errors sorted by count
    const topErrors = Array.from(errorCounts.entries())
      .sort((a, b) => b[1] - a[1])
      .slice(0, 10)
      .map(([errorType, count]) => ({
        errorType,
        count,
      }));

    return {
      totalErrors,
      byType,
      topErrors,
    };
  }

  /**
   * Export as Prometheus format
   */
  toPrometheus(): string[] {
    const summary = this.getErrorSummary();
    const metrics: PrometheusMetric[] = [];
    
    // Total errors counter
    metrics.push({
      name: 'error_total',
      type: 'counter',
      help: 'Total errors',
      samples: [{ value: summary.totalErrors }]
    });
    
    // Errors by type
    for (const [errorType, stats] of summary.byType) {
      metrics.push({
        name: 'error_by_type_total',
        type: 'counter',
        help: 'Errors grouped by type',
        samples: [{ labels: { error_type: errorType }, value: stats.count }]
      });
    }
    
    // Format all metrics
    return metrics.flatMap(m => PrometheusFormatter.formatMetric(m));
  }
  
  /**
   * Export as JSON
   */
  toJSON(): Record<string, unknown> {
    const summary = this.getErrorSummary();
    const errorsByType: Record<string, unknown> = {};
    
    for (const [errorType, stats] of summary.byType) {
      errorsByType[errorType] = {
        count: stats.count,
        affectedExecutions: stats.affectedExecutions,
        recoveryRate: stats.recoveryRate
      };
    }
    
    return {
      type: 'error',
      totalErrors: summary.totalErrors,
      byType: errorsByType,
      topErrors: summary.topErrors
    };
  }
}
