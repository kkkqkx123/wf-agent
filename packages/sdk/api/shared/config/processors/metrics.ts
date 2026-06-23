/**
 * Metrics Configuration Processor
 *
 * Provides functions for processing and merging metrics configuration.
 * This module handles the business logic for metrics config without file I/O.
 *
 * Following the project architecture pattern:
 * - All configuration processing happens in a../shared/core/config layer
 * - Pure functions, no side effects
 * - No file I/O operations
 */

import type { MetricsConfig, MetricCollectorConfig } from "@wf-agent/types";

/**
 * Default metrics configuration
 * Matches current hardcoded values for backward compatibility
 */
const DEFAULT_METRICS_CONFIG: MetricsConfig = {
  enabled: true,
  enablePeriodicReporting: false,
  reportingInterval: 60000,
  workflowMetrics: { bufferSize: 100, flushInterval: 5000, enablePeriodicReporting: false },
  nodeMetrics: { bufferSize: 100, flushInterval: 5000, enablePeriodicReporting: false },
  agentMetrics: { bufferSize: 100, flushInterval: 5000, enablePeriodicReporting: false },
  eventMetrics: { bufferSize: 100, flushInterval: 5000, enablePeriodicReporting: false },
  toolMetrics: { bufferSize: 100, flushInterval: 5000, enablePeriodicReporting: false },
  tokenMetrics: { bufferSize: 100, flushInterval: 5000, enablePeriodicReporting: false },
  templateMetrics: { bufferSize: 100, flushInterval: 5000, enablePeriodicReporting: false },
  configMetrics: { bufferSize: 100, flushInterval: 5000, enablePeriodicReporting: false },
  errorMetrics: { bufferSize: 100, flushInterval: 5000, enablePeriodicReporting: false },
  resourceMetrics: { bufferSize: 100, flushInterval: 5000, enablePeriodicReporting: false },
  agentLoopMetrics: { bufferSize: 100, flushInterval: 5000, enablePeriodicReporting: false },
};

/**
 * Merge user config with defaults
 * Performs deep merge preserving the integrity of metrics configuration as a whole
 *
 * @param userConfig - User-provided partial configuration
 * @returns Merged configuration with defaults applied
 */
export function mergeMetricsWithDefaults(userConfig: Partial<MetricsConfig>): MetricsConfig {
  // Start with defaults
  const merged: MetricsConfig = { ...DEFAULT_METRICS_CONFIG };

  // Merge global settings if provided
  if (userConfig.enabled !== undefined) merged.enabled = userConfig.enabled;
  if (userConfig.enablePeriodicReporting !== undefined) {
    merged.enablePeriodicReporting = userConfig.enablePeriodicReporting;
  }
  if (userConfig.reportingInterval !== undefined) {
    merged.reportingInterval = userConfig.reportingInterval;
  }

  // Merge collector configs as complete objects (maintains module cohesion)
  // This approach treats each collector config as an atomic unit
  const collectorConfigs = [
    "workflowMetrics",
    "nodeMetrics",
    "agentMetrics",
    "eventMetrics",
    "toolMetrics",
    "tokenMetrics",
    "templateMetrics",
    "configMetrics",
    "errorMetrics",
    "resourceMetrics",
    "agentLoopMetrics",
  ] as const;

  for (const key of collectorConfigs) {
    if (userConfig[key]) {
      // Merge at collector level: user config overrides defaults for that collector
      const collectorKey = key as keyof MetricsConfig;
      (merged[collectorKey] as MetricCollectorConfig | undefined) = {
        ...(DEFAULT_METRICS_CONFIG[collectorKey] as MetricCollectorConfig | undefined),
        ...(userConfig[collectorKey] as MetricCollectorConfig | undefined),
      };
    }
  }

  return merged;
}

/**
 * Get default config for specific environment
 * Provides environment-optimized defaults
 *
 * @param env - Environment name ("development" or "production")
 * @returns Environment-specific default configuration
 */
export function getMetricsEnvironmentDefaults(env: "development" | "production"): MetricsConfig {
  if (env === "development") {
    return {
      ...DEFAULT_METRICS_CONFIG,
      enablePeriodicReporting: true, // Enable for debugging
      reportingInterval: 30000, // More frequent in dev
      errorMetrics: {
        bufferSize: 10,
        flushInterval: 1000, // Faster error reporting in dev
        enablePeriodicReporting: false,
      },
    };
  }

  // Production defaults - more conservative
  return {
    ...DEFAULT_METRICS_CONFIG,
    enablePeriodicReporting: false,
    reportingInterval: 60000,
    eventMetrics: {
      bufferSize: 500, // Larger buffer for high-volume events
      flushInterval: 5000,
      enablePeriodicReporting: false,
    },
  };
}
