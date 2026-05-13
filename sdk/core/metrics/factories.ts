/**
 * Metrics System - Metric Factory Functions
 * 
 * Utility functions for creating metric instances.
 */

import { now } from "@wf-agent/common-utils";
import type {
  CounterMetric,
  GaugeMetric,
  HistogramMetric,
  SummaryMetric,
  HistogramBucket,
  PercentileValue,
} from "./types.js";

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
