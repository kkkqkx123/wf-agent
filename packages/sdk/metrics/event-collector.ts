/**
 * Event Metrics Collector
 *
 * Collects and aggregates event metrics across all workflow executions.
 * Replaces the legacy MetricsAggregator with the new Universal Metrics system.
 *
 * Design Principles:
 * - Use Counter metrics for event occurrence tracking
 * - Support flexible label-based filtering (workflow_id, node_type, tool_name, etc.)
 * - Provide cross-execution aggregated statistics
 * - Maintain backward compatibility where possible
 */

import { BaseMetricCollector } from "../metrics/base-collector.js";
import type { MetricCollectorConfig, MetricFilter, MetricQueryResult } from "../metrics/types.js";
import { PrometheusFormatter, type PrometheusMetric } from "./utils/prometheus-formatter.js";
import { createContextualLogger } from "@sdk/utils/contextual-logger.js";

const logger = createContextualLogger({ operation: "EventMetricsCollector" });

/**
 * Event metric labels
 */
export interface EventMetricLabels {
  /** Execution ID */
  execution_id?: string;
  /** Workflow ID */
  workflow_id?: string;
  /** Node ID (for node-related events) */
  node_id?: string;
  /** Node type (for node-related events) */
  node_type?: string;
  /** Tool name (for tool-related events) */
  tool_name?: string;
  /** Event status or result */
  status?: string;
  /** Additional custom labels */
  [key: string]: string | undefined;
}

/**
 * Aggregated event statistics (backward compatible interface)
 */
export interface AggregatedEventStat {
  /** Total count of this event type across all executions */
  count: number;
  /** Timestamp of the most recent occurrence */
  lastSeen: number;
  /** Count breakdown by execution ID */
  byExecution: Map<string, number>;
  /** First seen timestamp */
  firstSeen?: number;
}

/**
 * Event metrics summary (backward compatible interface)
 */
export interface EventMetricsSummary {
  /** Total events recorded */
  totalEvents: number;
  /** Statistics by event type */
  byEventType: Map<string, AggregatedEventStat>;
  /** Active execution count */
  activeExecutions: number;
  /** Summary generation timestamp */
  generatedAt: number;
}

/**
 * EventMetricsCollector - Unified event metrics collection
 *
 * Responsibilities:
 * - Record event occurrences as Counter metrics
 * - Aggregate statistics by event type and labels
 * - Track per-execution breakdowns
 * - Provide query and filtering capabilities
 * - Support periodic reporting
 */
export class EventMetricsCollector extends BaseMetricCollector {
  constructor(config?: MetricCollectorConfig) {
    super(config);
  }

  /**
   * Record an event occurrence
   * @param eventType Event type (e.g., 'NODE_COMPLETED', 'WORKFLOW_STARTED')
   * @param executionId Execution ID
   * @param labels Additional dimension labels
   *
   * @example
   * ```typescript
   * collector.recordEvent('NODE_COMPLETED', 'exec-123', {
   *   workflow_id: 'wf-456',
   *   node_id: 'node-1',
   *   node_type: 'LLM'
   * });
   * ```
   */
  recordEvent(eventType: string, executionId: string, labels?: EventMetricLabels): void {
    if (!eventType) {
      logger.warn("recordEvent called with missing eventType");
      return;
    }
    if (!executionId) {
      logger.warn("recordEvent called with missing executionId");
      return;
    }

    // Convert to Counter metric with standardized naming
    const metricName = `event.${eventType.toLowerCase().replace(/_/g, ".")}.count`;

    // Merge execution_id into labels
    const mergedLabels: Record<string, string> = {
      execution_id: executionId,
      ...(labels || {}),
    };

    this.incrementCounter(metricName, mergedLabels);
  }

  /**
   * Get aggregated statistics for a specific event type
   * Backward compatible with old MetricsAggregator API
   *
   * @param eventType Event type to query
   * @returns Aggregated statistics or undefined if not found
   */
  getStatistics(eventType: string): AggregatedEventStat | undefined {
    const metricName = `event.${eventType.toLowerCase().replace(/_/g, ".")}.count`;

    const result = this.query({
      metricName,
      metricType: "counter",
    });

    if (result.totalCount === 0) {
      return undefined;
    }

    // Aggregate statistics from query result
    const stat: AggregatedEventStat = {
      count: 0,
      lastSeen: 0,
      byExecution: new Map(),
      firstSeen: undefined,
    };

    const aggregated = result.metrics.get(metricName);
    if (aggregated && aggregated.timeSeries) {
      stat.count = aggregated.value;

      // Extract time series data
      if (aggregated.timeSeries && aggregated.timeSeries.length > 0) {
        stat.firstSeen = aggregated.timeSeries[0]!.timestamp;
        stat.lastSeen = aggregated.timeSeries[aggregated.timeSeries.length - 1]!.timestamp;
      }

      // Group by execution_id
      for (const [labelKey, labelAgg] of aggregated.byLabel.entries()) {
        try {
          const labels = JSON.parse(labelKey);
          if (labels.execution_id) {
            stat.byExecution.set(labels.execution_id, labelAgg.value);
          }
        } catch (error) {
          logger.warn("Failed to parse label key", { labelKey, error });
        }
      }
    }

    return stat;
  }

  /**
   * Get all aggregated statistics
   * Backward compatible with old MetricsAggregator API
   *
   * @returns Map of event type to statistics
   */
  getAllStatistics(): Map<string, AggregatedEventStat> {
    const result = this.query({
      metricType: "counter",
    });

    const stats = new Map<string, AggregatedEventStat>();

    for (const [metricName, aggregated] of result.metrics.entries()) {
      // Extract event type from metric name
      if (metricName.startsWith("event.") && metricName.endsWith(".count")) {
        const eventType = metricName
          .slice(6, -6) // Remove 'event.' prefix and '.count' suffix
          .replace(/\./g, "_")
          .toUpperCase();

        const stat: AggregatedEventStat = {
          count: aggregated.value,
          lastSeen: 0,
          byExecution: new Map(),
          firstSeen: undefined,
        };

        if (aggregated.timeSeries && aggregated.timeSeries.length > 0) {
          stat.firstSeen = aggregated.timeSeries[0]!.timestamp;
          stat.lastSeen = aggregated.timeSeries[aggregated.timeSeries.length - 1]!.timestamp;
        }

        // Group by execution_id
        for (const [labelKey, labelAgg] of aggregated.byLabel.entries()) {
          try {
            const labels = JSON.parse(labelKey);
            if (labels.execution_id) {
              stat.byExecution.set(labels.execution_id, labelAgg.value);
            }
          } catch (error) {
            logger.warn("Failed to parse label key", { labelKey, error });
          }
        }

        stats.set(eventType, stat);
      }
    }

    return stats;
  }

  /**
   * Generate a complete event metrics summary
   * Backward compatible with old MetricsAggregator API
   *
   * @returns Comprehensive summary of all event metrics
   */
  generateSummary(): EventMetricsSummary {
    const allStats = this.getAllStatistics();

    // Calculate total events
    let totalEvents = 0;
    for (const stat of allStats.values()) {
      totalEvents += stat.count;
    }

    // Count active executions
    const activeExecutions = new Set<string>();
    for (const stat of allStats.values()) {
      for (const executionId of stat.byExecution.keys()) {
        activeExecutions.add(executionId);
      }
    }

    return {
      totalEvents,
      byEventType: allStats,
      activeExecutions: activeExecutions.size,
      generatedAt: Date.now(),
    };
  }

  /**
   * Query event metrics with advanced filters
   *
   * @param filter Query filter criteria
   * @returns Query result with aggregated metrics
   *
   * @example
   * ```typescript
   * // Query all NODE_COMPLETED events for a specific workflow
   * const result = collector.queryEvents({
   *   metricName: 'event.node.completed.count',
   *   labels: { workflow_id: 'wf-123' }
   * });
   * ```
   */
  queryEvents(filter: MetricFilter): MetricQueryResult {
    return this.query(filter);
  }

  /**
   * Get event statistics aggregated by a specific label
   *
   * Useful for grouping events by workflow_id, agent_loop_id, etc.
   *
   * @param labelKey Label key to group by (e.g., 'workflow_id', 'agent_loop_id')
   * @param eventTypeFilter Optional event type filter (if not provided, all events are included)
   * @returns Map of label values to event counts
   *
   * @example
   * ```typescript
   * // Get all workflows and their event counts
   * const stats = collector.getStatisticsByLabel('workflow_id');
   * // Returns { "wf-123": 152, "wf-456": 89 }
   *
   * // Get agent loops and their event counts
   * const agentStats = collector.getStatisticsByLabel('agent_loop_id');
   * // Returns { "agent-123": 45, "agent-456": 67 }
   * ```
   */
  getStatisticsByLabel(
    labelKey: string,
    eventTypeFilter?: string[],
  ): Record<string, number> {
    const result = this.query({
      metricType: "counter",
    });

    const stats: Record<string, number> = {};

    // Iterate through all event metrics
    for (const [metricName, aggregated] of result.metrics.entries()) {
      // If eventTypeFilter is specified, check if metric matches
      if (eventTypeFilter) {
        const eventType = metricName
          .slice(6, -6) // Remove 'event.' prefix and '.count' suffix
          .replace(/\./g, "_")
          .toUpperCase();
        if (!eventTypeFilter.includes(eventType)) {
          continue;
        }
      }

      // Group by specified label
      for (const [labelKeyStr, labelAgg] of aggregated.byLabel.entries()) {
        try {
          const labels = JSON.parse(labelKeyStr);
          if (labels[labelKey]) {
            const labelValue = labels[labelKey];
            stats[labelValue] = (stats[labelValue] || 0) + labelAgg.value;
          }
        } catch (error) {
          logger.warn("Failed to parse label key during aggregation", {
            labelKeyStr,
            error,
          });
        }
      }
    }

    return stats;
  }

  /**
   * Cleanup metrics for a specific execution
   * Called when an execution completes to free memory
   *
   * @param executionId Execution ID to cleanup
   * @returns Number of metrics entries cleaned
   */
  cleanupExecution(executionId: string): number {
    if (!executionId) {
      logger.warn("cleanupExecution called with empty executionId");
      return 0;
    }

    let cleanedCount = 0;

    // Filter out metrics for this execution
    const remainingMetrics = this.metricsBuffer.filter(metric => {
      if (metric.labels?.["execution_id"] === executionId) {
        cleanedCount++;
        return false;
      }
      return true;
    });

    if (cleanedCount > 0) {
      this.metricsBuffer = remainingMetrics;
      logger.debug("Cleaned up execution metrics", { executionId, cleanedCount });
    }

    return cleanedCount;
  }

  /**
   * Export event metrics in Prometheus format
   */
  toPrometheus(): string[] {
    const summary = this.generateSummary();
    const metrics: PrometheusMetric[] = [];

    // Total events published
    metrics.push({
      name: "event_published_total",
      type: "counter",
      help: "Total events published",
      samples: [{ value: summary.totalEvents }],
    });

    // Events by type
    for (const [eventType, stat] of summary.byEventType.entries()) {
      metrics.push({
        name: "event_published_by_type_total",
        type: "counter",
        help: "Events published grouped by type",
        samples: [{ labels: { event_type: eventType }, value: stat.count }],
      });
    }

    return metrics.flatMap(m => PrometheusFormatter.formatMetric(m));
  }

  /**
   * Export as JSON
   */
  toJSON(): Record<string, unknown> {
    const summary = this.generateSummary();

    // Convert Map to plain object for JSON serialization
    const byEventTypeObj: Record<
      string,
      {
        count: number;
        lastSeen: number;
        firstSeen: number;
        byExecution: Record<string, number>;
      }
    > = {};
    for (const [eventType, stat] of summary.byEventType.entries()) {
      byEventTypeObj[eventType] = {
        count: stat.count,
        lastSeen: stat.lastSeen ?? 0,
        firstSeen: stat.firstSeen ?? 0,
        byExecution: Object.fromEntries(stat.byExecution),
      };
    }

    return {
      type: "event",
      totalEvents: summary.totalEvents,
      byEventType: byEventTypeObj,
      activeExecutions: summary.activeExecutions,
      generatedAt: summary.generatedAt,
    };
  }
}
