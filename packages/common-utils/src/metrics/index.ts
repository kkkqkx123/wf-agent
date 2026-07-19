/**
 * Metrics Module - Shared Types and Interfaces
 *
 * Provides generic, domain-agnostic metric types and collector interfaces
 * that can be used across all packages in the monorepo.
 */

// Core metric types
export {
  type MetricType,
  type BaseMetric,
  type CounterMetric,
  type GaugeMetric,
  type HistogramBucket,
  type HistogramMetric,
  type PercentileValue,
  type SummaryMetric,
  type Metric,
  type MetricFilter,
  type AggregatedMetric,
  type MetricQueryResult,
  type MetricCollectorConfig,
  type MetricReportCallback,
  type TrendDataPoint,
  type MetricTrend,
  type MetricReport,
} from "./types.js";

// Collector interfaces
export {
  type MetricCollector,
  type MetricExporter,
} from "./collector.js";

// Data point types (cross-package persistence contract)
export {
  type MetricDataPoint,
  type MetricsQuery,
} from "./data-point.js";