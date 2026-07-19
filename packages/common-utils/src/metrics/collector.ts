/**
 * Metric Collector Interfaces
 *
 * Defines the contract for metric collectors.
 * Any package can implement these interfaces to participate in the metrics system.
 */

import type {
  Metric,
  MetricFilter,
  MetricQueryResult,
  MetricReportCallback,
} from "./types.js";

/**
 * Metric exporter interface
 * Each collector can implement this to support multiple export formats
 */
export interface MetricExporter {
  /**
   * Export metrics in Prometheus exposition format
   * @returns Array of formatted metric lines (without trailing newline)
   */
  toPrometheus(): string[];

  /**
   * Export metrics as JSON
   * @returns JSON-serializable object
   */
  toJSON(): Record<string, unknown>;
}

/**
 * Core interface for metric collectors
 * Each component (workflow, node, tool, etc.) should implement its own collector
 */
export interface MetricCollector extends MetricExporter {
  /**
   * Record a metric
   * @param metric The metric to record
   */
  record(metric: Metric): void;

  /**
   * Record a counter increment
   * @param metricName Metric name
   * @param labels Dimension labels
   * @param increment Amount to increment (default: 1)
   */
  incrementCounter(metricName: string, labels?: Record<string, string>, increment?: number): void;

  /**
   * Record a gauge value
   * @param metricName Metric name
   * @param value Current value
   * @param labels Dimension labels
   */
  setGauge(metricName: string, value: number, labels?: Record<string, string>): void;

  /**
   * Record a histogram observation
   * @param metricName Metric name
   * @param value Observed value
   * @param labels Dimension labels
   */
  observeHistogram(metricName: string, value: number, labels?: Record<string, string>): void;

  /**
   * Record a summary observation
   * @param metricName Metric name
   * @param value Observed value
   * @param labels Dimension labels
   */
  observeSummary(metricName: string, value: number, labels?: Record<string, string>): void;

  /**
   * Flush buffered metrics
   */
  flush(): Promise<void>;

  /**
   * Query metrics with filters
   * @param filter Query filter criteria
   * @returns Query result
   */
  query(filter: MetricFilter): MetricQueryResult;

  /**
   * Subscribe to periodic reports
   * @param callback Callback function
   * @param options Subscription options
   * @returns Unsubscribe function
   */
  onReport(callback: MetricReportCallback, options?: { interval?: number }): () => void;

  /**
   * Clear all collected metrics
   */
  clear(): void;

  /**
   * Dispose the collector and release resources
   */
  dispose(): void;
}