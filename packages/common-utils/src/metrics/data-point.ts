/**
 * Metric Data Point
 *
 * Defines the structure for persisting individual metric data points.
 * This is the shared type between metric collectors and storage backends.
 */

/**
 * Metric data point structure
 */
export interface MetricDataPoint {
  /** Metric name (e.g., "workflow.execution.count") */
  metricName: string;
  /** Metric type */
  metricType: 'counter' | 'gauge' | 'histogram';
  /** Metric value */
  value: number;
  /** Timestamp in milliseconds */
  timestamp: number;
  /** Optional labels for filtering */
  labels?: Record<string, string>;
  /** Collector name that generated this metric */
  collectorName: string;
}

/**
 * Metrics query parameters
 */
export interface MetricsQuery {
  /** Filter by metric name */
  metricName?: string;
  /** Filter by metric type */
  metricType?: 'counter' | 'gauge' | 'histogram';
  /** Time range start (Unix timestamp in milliseconds) */
  startTime?: number;
  /** Time range end (Unix timestamp in milliseconds) */
  endTime?: number;
  /** Filter by labels (all must match) */
  labels?: Record<string, string>;
  /** Filter by collector name */
  collectorName?: string;
  /** Maximum number of results */
  limit?: number;
  /** Sort order: 'asc' (oldest first) or 'desc' (newest first) */
  sortOrder?: 'asc' | 'desc';
}