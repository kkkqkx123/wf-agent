/**
 * Metrics System - Public API
 */

// Core types
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
  type MetricReport,
  type MetricCollector,
  type MetricExporter,
} from "./types.js";

// Constants
export {
  WORKFLOW_METRICS,
  NODE_METRICS,
  TOOL_METRICS,
  TOKEN_METRICS,
  ERROR_METRICS,
  RESOURCE_METRICS,
  AGENT_LOOP_METRICS,
  TEMPLATE_METRICS,
  CONFIG_METRICS,
  SUBGRAPH_METRICS,
  RETRY_METRICS,
} from "./constants.js";

// Factory functions
export { createCounter, createGauge, createHistogram, createSummary } from "./factories.js";

// Collectors
// ===== Runtime Metrics =====
export { BaseMetricCollector } from "./base-collector.js";
export { WorkflowMetricsCollector } from "./workflow-collector.js";
export { NodeMetricsCollector } from "./node-collector.js";
export { AgentMetricsCollector } from "./agent-collector.js";
export {
  EventMetricsCollector,
  type EventMetricLabels,
  type AggregatedEventStat,
  type EventMetricsSummary,
} from "./event-collector.js";

// ===== Resource Metrics =====
export { ToolMetricsCollector } from "./tool-collector.js";
export { TokenMetricsCollector, type TokenUsageData } from "./token-collector.js";
export { TemplateMetricsCollector } from "./template-collector.js";

// ===== Infrastructure Metrics =====
export { ErrorMetricsCollector } from "./error-collector.js";
export { ResourceMetricsCollector } from "./resource-collector.js";
export { ConfigMetricsCollector } from "./config-collector.js";
export { AgentLoopMetricsCollector } from "./agent-loop-collector.js";
export { RetryBudgetMetricsCollector } from "./retry-budget-collector.js";

export { MetricsRegistry, type MetricsRegistryConfig } from "./metrics-registry.js";

// Factory functions for collectors
export { createMetricsCollectors } from "./factories.js";

// Utilities
export { PrometheusFormatter } from "./utils/prometheus-formatter.js";
export type {
  PrometheusMetricType,
  PrometheusMetric,
  PrometheusSample,
} from "./utils/prometheus-formatter.js";

