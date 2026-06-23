/**
 * Metrics Configuration Zod Schemas
 * Provides runtime validation schemas for Metrics configuration
 */

import { z } from "zod";
import type { MetricsConfig, MetricCollectorConfig } from "./metrics.js";

/**
 * Metric Collector Configuration Schema
 */
export const MetricCollectorConfigSchema: z.ZodType<MetricCollectorConfig> = z.object({
  bufferSize: z.number().positive("Buffer size must be positive").optional(),
  flushInterval: z.number().positive("Flush interval must be positive").optional(),
  enablePeriodicReporting: z.boolean().optional(),
});

/**
 * Metrics Configuration Schema
 */
export const MetricsConfigSchema: z.ZodType<MetricsConfig> = z.object({
  workflowMetrics: MetricCollectorConfigSchema.optional(),
  nodeMetrics: MetricCollectorConfigSchema.optional(),
  agentMetrics: MetricCollectorConfigSchema.optional(),
  eventMetrics: MetricCollectorConfigSchema.optional(),
  toolMetrics: MetricCollectorConfigSchema.optional(),
  tokenMetrics: MetricCollectorConfigSchema.optional(),
  templateMetrics: MetricCollectorConfigSchema.optional(),
  configMetrics: MetricCollectorConfigSchema.optional(),
  errorMetrics: MetricCollectorConfigSchema.optional(),
  resourceMetrics: MetricCollectorConfigSchema.optional(),
  agentLoopMetrics: MetricCollectorConfigSchema.optional(),
  enablePeriodicReporting: z.boolean().optional(),
  reportingInterval: z.number().positive("Reporting interval must be positive").optional(),
  enabled: z.boolean().optional(),
});

/**
 * Type guard for MetricsConfig
 */
export function isMetricsConfig(value: unknown): value is MetricsConfig {
  return MetricsConfigSchema.safeParse(value).success;
}

/**
 * Type guard for MetricCollectorConfig
 */
export function isMetricCollectorConfig(value: unknown): value is MetricCollectorConfig {
  return MetricCollectorConfigSchema.safeParse(value).success;
}
