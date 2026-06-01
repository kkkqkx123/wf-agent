/**
 * Timeout Metrics Collector
 * 
 * Collects and reports timeout-related metrics for observability.
 * Integrates with TimeoutRegistry to provide real-time timeout monitoring.
 */

import { BaseMetricCollector } from "./base-collector.js";
import type { MetricCollectorConfig, MetricFilter, MetricQueryResult } from "./types.js";
import type { TimeoutRegistry } from "../registry/timeout-registry.js";
import { PrometheusFormatter } from "./utils/prometheus-formatter.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "TimeoutMetricsCollector" });

export class TimeoutMetricsCollector extends BaseMetricCollector {
  private timeoutRegistry?: TimeoutRegistry;
  
  constructor(config?: MetricCollectorConfig) {
    super(config);
  }

  /**
   * Bind to TimeoutRegistry for data collection
   */
  bindToRegistry(registry: TimeoutRegistry): void {
    this.timeoutRegistry = registry;
    logger.info("Bound to TimeoutRegistry");
  }

  /**
   * Record timeout registration
   */
  recordRegistration(tag: string, duration: number, executionId: string): void {
    this.incrementCounter('timeout.registration.count', {
      tag,
      execution_id: executionId,
    });
    
    this.observeHistogram('timeout.duration.configured', duration, {
      tag,
      execution_id: executionId,
    });
  }

  /**
   * Record timeout expiration
   */
  recordExpiration(tag: string, actualDuration: number, executionId: string): void {
    this.incrementCounter('timeout.expiration.count', {
      tag,
      execution_id: executionId,
    });
    
    this.observeHistogram('timeout.duration.actual', actualDuration, {
      tag,
      execution_id: executionId,
    });
  }

  /**
   * Record timeout cancellation
   */
  recordCancellation(tag: string, reason: 'user' | 'interrupted' | 'cleanup', executionId: string): void {
    this.incrementCounter('timeout.cancellation.count', {
      tag,
      reason,
      execution_id: executionId,
    });
  }

  /**
   * Record timeout warning
   */
  recordWarning(tag: string, remainingTime: number, executionId: string): void {
    this.incrementCounter('timeout.warning.count', {
      tag,
      execution_id: executionId,
    });
    
    this.setGauge('timeout.warning.remaining_time', remainingTime, {
      tag,
      execution_id: executionId,
    });
  }

  /**
   * Collect current state from TimeoutRegistry
   */
  collectFromRegistry(): void {
    if (!this.timeoutRegistry) {
      logger.warn("TimeoutRegistry not bound, skipping collection");
      return;
    }

    const stats = this.timeoutRegistry.getStats();
    
    // Record global statistics
    this.setGauge('timeout.active.count', stats.totalTimeouts);
    this.setGauge('timeout.executions.active', stats.activeExecutions);
    
    // Record by-tag statistics
    Object.entries(stats.byTag).forEach(([tag, count]) => {
      this.setGauge('timeout.active.by_tag', count, { tag });
    });
    
    // Record by-category statistics
    Object.entries(stats.byCategory).forEach(([category, count]) => {
      this.setGauge('timeout.active.by_category', count, { category });
    });
    
    // Record cumulative statistics
    this.incrementCounter('timeout.registered.total', {}, stats.totalRegistered);
    this.incrementCounter('timeout.expired.total', {}, stats.timedOutCount);
    this.incrementCounter('timeout.cancelled.total', {}, stats.cancelledCount);
    
    logger.debug("Collected timeout metrics from registry", {
      activeTimeouts: stats.totalTimeouts,
      activeExecutions: stats.activeExecutions,
    });
  }

  /**
   * Query timeout metrics
   */
  override query(filter?: MetricFilter): MetricQueryResult {
    return super.query(filter || {});
  }

  /**
   * Generate timeout-specific summary
   */
  generateSummary(): {
    totalActive: number;
    totalExpired: number;
    totalCancelled: number;
    byTag: Record<string, number>;
    byCategory: Record<string, number>;
    averageDuration: number;
    timeoutRate: number;
  } {
    if (!this.timeoutRegistry) {
      return {
        totalActive: 0,
        totalExpired: 0,
        totalCancelled: 0,
        byTag: {},
        byCategory: {},
        averageDuration: 0,
        timeoutRate: 0,
      };
    }

    const stats = this.timeoutRegistry.getStats();
    const totalCompleted = stats.timedOutCount + stats.cancelledCount;
    const timeoutRate = totalCompleted > 0 
      ? stats.timedOutCount / totalCompleted 
      : 0;

    return {
      totalActive: stats.totalTimeouts,
      totalExpired: stats.timedOutCount,
      totalCancelled: stats.cancelledCount,
      byTag: stats.byTag,
      byCategory: stats.byCategory,
      averageDuration: 0, // Would need to calculate from histogram
      timeoutRate,
    };
  }

  /**
   * Export metrics in Prometheus exposition format
   */
  toPrometheus(): string[] {
    const lines: string[] = [];
    
    // Get internal collector metrics
    lines.push(...this.exportInternalMetrics());
    
    // Query current metrics and format them
    const result = this.query({});
    
    result.metrics.forEach((aggregated) => {
      const prometheusMetrics: Array<{ name: string; value: number; labels: Record<string, string> }> = [];
      
      // Convert aggregated metric to Prometheus format
      aggregated.byLabel.forEach((labelMetric, labelKey) => {
        let labels: Record<string, string> = {};
        try {
          labels = JSON.parse(labelKey);
        } catch {
          // If parsing fails, use empty labels
        }
        
        prometheusMetrics.push({
          name: aggregated.metricName.replace(/\./g, '_'),
          value: labelMetric.value as number,
          labels,
        });
      });
      
      // Format using PrometheusFormatter
      if (prometheusMetrics.length > 0) {
        // Create samples for the metric
        const samples = prometheusMetrics.map(metric => ({
          labels: metric.labels,
          value: metric.value,
        }));
        
        const prometheusMetric = {
          name: aggregated.metricName.replace(/\./g, '_'),
          type: aggregated.metricType,
          help: `Timeout metric: ${aggregated.metricName}`,
          samples,
        };
        
        const formatted = PrometheusFormatter.formatMetric(prometheusMetric);
        lines.push(...formatted);
      }
    });
    
    return lines;
  }

  /**
   * Export metrics as JSON
   */
  toJSON(): Record<string, unknown> {
    const summary = this.generateSummary();
    const queryResult = this.query({});
    
    return {
      collector: 'timeout',
      summary,
      metrics: Array.from(queryResult.metrics.values()).map(m => ({
        name: m.metricName,
        type: m.metricType,
        value: m.value,
      })),
      internalMetrics: this.getInternalMetrics(),
    };
  }
}
