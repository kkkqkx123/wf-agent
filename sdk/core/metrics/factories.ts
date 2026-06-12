/**
 * Metrics System - Metric Factory Functions
 *
 * Utility functions for creating metric instances and collectors.
 */

import { now } from "@wf-agent/common-utils";
import type {
  CounterMetric,
  GaugeMetric,
  HistogramMetric,
  SummaryMetric,
  HistogramBucket,
  PercentileValue,
  MetricCollectorConfig,
} from "./types.js";
import { WorkflowMetricsCollector } from "./workflow-collector.js";
import { EventMetricsCollector } from "./event-collector.js";
import { NodeMetricsCollector } from "./node-collector.js";
import { AgentMetricsCollector } from "./agent-collector.js";
import { ToolMetricsCollector } from "./tool-collector.js";
import { TokenMetricsCollector } from "./token-collector.js";
import { ErrorMetricsCollector } from "./error-collector.js";
import { ResourceMetricsCollector } from "./resource-collector.js";
import { AgentLoopMetricsCollector } from "./agent-loop-collector.js";
import { TemplateMetricsCollector } from "./template-collector.js";
import { ConfigMetricsCollector } from "./config-collector.js";

/**
 * Create a counter metric
 */
export function createCounter(
  metricName: string,
  value: number,
  labels?: Record<string, string>,
  metadata?: Record<string, unknown>,
): CounterMetric {
  return {
    metricName,
    metricType: "counter",
    timestamp: now(),
    labels: labels || {},
    value,
    metadata,
  };
}

/**
 * Create a gauge metric
 */
export function createGauge(
  metricName: string,
  value: number,
  labels?: Record<string, string>,
  metadata?: Record<string, unknown>,
): GaugeMetric {
  return {
    metricName,
    metricType: "gauge",
    timestamp: now(),
    labels: labels || {},
    value,
    metadata,
  };
}

/**
 * Create a histogram metric
 */
export function createHistogram(
  metricName: string,
  value: number,
  buckets: HistogramBucket[],
  sum: number,
  count: number,
  labels?: Record<string, string>,
  metadata?: Record<string, unknown>,
): HistogramMetric {
  return {
    metricName,
    metricType: "histogram",
    timestamp: now(),
    labels: labels || {},
    value,
    buckets,
    sum,
    count,
    metadata,
  };
}

/**
 * Create a summary metric
 */
export function createSummary(
  metricName: string,
  value: number,
  percentiles: PercentileValue[],
  sum: number,
  count: number,
  labels?: Record<string, string>,
  metadata?: Record<string, unknown>,
): SummaryMetric {
  return {
    metricName,
    metricType: "summary",
    timestamp: now(),
    labels: labels || {},
    value,
    percentiles,
    sum,
    count,
    metadata,
  };
}

/**
 * Create all standard metric collectors with unified configuration
 * @param config Optional collector configuration (applied to all)
 * @returns Object containing all collector instances
 */
export function createMetricsCollectors(config?: MetricCollectorConfig) {
  return {
    workflow: new WorkflowMetricsCollector(config),
    event: new EventMetricsCollector(config),
    node: new NodeMetricsCollector(config),
    agent: new AgentMetricsCollector(config),
    tool: new ToolMetricsCollector(config),
    token: new TokenMetricsCollector(config),
    error: new ErrorMetricsCollector(config),
    resource: new ResourceMetricsCollector(config),
    agentLoop: new AgentLoopMetricsCollector(config),
    template: new TemplateMetricsCollector(config),
    config: new ConfigMetricsCollector(config),
  };
}
