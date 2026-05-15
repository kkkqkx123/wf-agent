/**
 * Metrics System - Type Definitions
 * 
 * Core type definitions for the metrics collection system.
 */

// =============================================================================
// Metric Type Classification
// =============================================================================

/**
 * Metric type classification
 */
export type MetricType =
  /** Monotonically increasing counter (e.g., request count) */
  | "counter"
  /** Gauge that can increase or decrease (e.g., active executions) */
  | "gauge"
  /** Histogram for distribution analysis (e.g., latency buckets) */
  | "histogram"
  /** Summary with percentile calculations (e.g., p95, p99) */
  | "summary";

// =============================================================================
// Base Metric Structure
// =============================================================================

/**
 * Base metric structure shared by all metric types
 */
export interface BaseMetric {
  /** Unique metric name (e.g., "workflow.execution.duration") */
  metricName: string;
  /** Metric type */
  metricType: MetricType;
  /** Timestamp when metric was recorded */
  timestamp: number;
  /** Labels/dimensions for filtering and grouping */
  labels: Record<string, string>;
  /** Metric value */
  value: number | string;
  /** Optional metadata */
  metadata?: Record<string, unknown>;
}

// =============================================================================
// Specific Metric Types
// =============================================================================

/**
 * Counter metric - monotonically increasing value
 * Use case: counting events, requests, errors
 */
export interface CounterMetric extends BaseMetric {
  metricType: "counter";
  value: number;
  /** Increment amount (default: 1) */
  increment?: number;
}

/**
 * Gauge metric - value that can go up and down
 * Use case: active connections, queue length, memory usage
 */
export interface GaugeMetric extends BaseMetric {
  metricType: "gauge";
  value: number;
}

/**
 * Histogram bucket
 */
export interface HistogramBucket {
  /** Upper bound of bucket (inclusive) */
  upperBound: number;
  /** Count of observations in this bucket */
  count: number;
}

/**
 * Histogram metric - tracks value distributions
 * Use case: latency, response time, size distributions
 */
export interface HistogramMetric extends BaseMetric {
  metricType: "histogram";
  value: number;
  /** Bucket boundaries */
  buckets: HistogramBucket[];
  /** Sum of all observed values */
  sum: number;
  /** Total count of observations */
  count: number;
}

/**
 * Percentile value
 */
export interface PercentileValue {
  /** Percentile (e.g., 0.95 for p95) */
  percentile: number;
  /** Value at this percentile */
  value: number;
}

/**
 * Summary metric - provides percentile calculations
 * Use case: p95 latency, p99 response time
 */
export interface SummaryMetric extends BaseMetric {
  metricType: "summary";
  value: number;
  /** Pre-calculated percentiles */
  percentiles: PercentileValue[];
  /** Sum of all observed values */
  sum: number;
  /** Total count of observations */
  count: number;
}

/**
 * Union type for all metric types
 */
export type Metric = CounterMetric | GaugeMetric | HistogramMetric | SummaryMetric;

// =============================================================================
// Metric Collection and Querying
// =============================================================================

/**
 * Filter criteria for querying metrics
 */
export interface MetricFilter {
  /** Filter by metric name (supports wildcards) */
  metricName?: string;
  /** Filter by metric type */
  metricType?: MetricType;
  /** Filter by label key-value pairs */
  labels?: Record<string, string>;
  /** Filter by time range */
  timeRange?: {
    from: number;
    to: number;
  };
  /** Maximum number of results */
  limit?: number;
}

/**
 * Aggregated metric result
 */
export interface AggregatedMetric {
  metricName: string;
  metricType: MetricType;
  /** Aggregated value (sum, avg, max, min depending on type) */
  value: number;
  /** Grouped by label values */
  byLabel: Map<string, AggregatedMetric>;
  /** Time series data points */
  timeSeries?: Array<{ timestamp: number; value: number }>;
  /** Metadata */
  metadata?: Record<string, unknown>;
}

/**
 * Query result containing aggregated metrics
 */
export interface MetricQueryResult {
  /** Total number of matching metrics */
  totalCount: number;
  /** Aggregated metrics grouped by name */
  metrics: Map<string, AggregatedMetric>;
  /** Query execution time */
  queryTime: number;
}

// =============================================================================
// Metric Collector Interface
// =============================================================================

/**
 * Configuration for metric collectors
 */
export interface MetricCollectorConfig {
  /** Buffer size before automatic flush (default: 100) */
  bufferSize?: number;
  /** Flush interval in milliseconds (default: 5000) */
  flushInterval?: number;
  /** Enable periodic reporting (default: false) */
  enablePeriodicReporting?: boolean;
  /** Reporting interval in milliseconds (default: 10000) */
  reportingInterval?: number;
  /** Maximum age of metrics to keep (in ms, default: 1 hour) */
  maxAge?: number;
}

/**
 * Callback for metric reporting
 */
export type MetricReportCallback = (report: MetricReport) => void | Promise<void>;

/**
 * Metric report generated periodically
 */
export interface MetricReport {
  /** Report generation timestamp */
  timestamp: number;
  /** Summary statistics */
  summary: {
    totalMetrics: number;
    byType: Record<MetricType, number>;
    byCategory: Record<string, number>;
  };
  /** Top N metrics by value */
  topMetrics: Array<{
    metricName: string;
    value: number;
    labels: Record<string, string>;
  }>;
  /** Anomalies detected */
  anomalies?: Array<{
    metricName: string;
    description: string;
    severity: "low" | "medium" | "high";
  }>;
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

// =============================================================================
// Metric Exporter Interface
// =============================================================================

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
