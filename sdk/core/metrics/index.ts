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
} from "./constants.js";

// Factory functions
export {
  createCounter,
  createGauge,
  createHistogram,
  createSummary,
} from "./factories.js";

// Collectors
export { BaseMetricCollector } from "./base-collector.js";
export { WorkflowMetricsCollector as LegacyWorkflowMetricsCollector } from "./workflow-collector.js";
export { EventMetricsCollector, type EventMetricLabels, type AggregatedEventStat, type EventMetricsSummary } from "./event-collector.js";
export { NodeMetricsCollector as LegacyNodeMetricsCollector } from "./node-collector.js";
export { ToolMetricsCollector } from "./tool-collector.js";
export { TokenMetricsCollector, type TokenUsageData } from "./token-collector.js";
export { ErrorMetricsCollector } from "./error-collector.js";
export { ResourceMetricsCollector } from "./resource-collector.js";
export { AgentLoopMetricsCollector } from "./agent-loop-collector.js";
export { TemplateMetricsCollector } from "./template-collector.js";
export { ConfigMetricsCollector } from "./config-collector.js";

// New Enhanced Collectors (Phase 1 Implementation)
export { WorkflowMetricsCollector } from "./workflow-metrics-collector.js";
export { NodeMetricsCollector } from "./node-metrics-collector.js";
export { AgentMetricsCollector } from "./agent-metrics-collector.js";
export { MetricsRegistry, type MetricsRegistryConfig } from "./metrics-registry.js";

// Factory functions for collectors
export { createMetricsCollectors } from "./factories.js";
