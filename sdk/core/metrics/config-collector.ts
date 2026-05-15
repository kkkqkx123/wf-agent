/**
 * Configuration Access Metrics Collector
 * 
 * Collects and aggregates metrics for configuration loading and access including:
 * - Configuration access counts
 * - Load duration distributions
 * - Cache hit/miss rates
 * - Validation error tracking
 */

import { BaseMetricCollector } from "./base-collector.js";
import type { MetricCollectorConfig, MetricFilter, MetricQueryResult } from "./types.js";
import { CONFIG_METRICS } from "./constants.js";
import { PrometheusFormatter, type PrometheusMetric } from "./utils/prometheus-formatter.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ operation: "ConfigMetricsCollector" });

/**
 * Configuration-specific metric collector
 * Extends BaseMetricCollector with config-specific convenience methods
 */
export class ConfigMetricsCollector extends BaseMetricCollector {
  constructor(config?: MetricCollectorConfig) {
    super(config);
  }

  /**
   * Record configuration access
   * @param configPath Configuration path or key
   * @param configType Configuration type (e.g., 'workflow', 'agent', 'tool')
   * @param context Optional context
   */
  recordAccess(
    configPath: string,
    configType?: string,
    context?: Record<string, string>,
  ): void {
    const labels: Record<string, string> = {
      config_path: configPath,
      ...(configType && { config_type: configType }),
      ...context,
    };

    this.incrementCounter(CONFIG_METRICS.ACCESS_COUNT, labels);
  }

  /**
   * Record configuration load completion
   * @param configPath Configuration path
   * @param duration Load duration in milliseconds
   * @param success Whether load was successful
   * @param configType Configuration type
   * @param context Optional context
   */
  recordLoadComplete(
    configPath: string,
    duration: number,
    success: boolean,
    configType?: string,
    context?: Record<string, string>,
  ): void {
    const labels: Record<string, string> = {
      config_path: configPath,
      success: success.toString(),
      ...(configType && { config_type: configType }),
      ...context,
    };

    // Record load duration distribution
    this.observeHistogram(CONFIG_METRICS.LOAD_DURATION, duration, labels);

    // Record validation errors if failed
    if (!success) {
      this.incrementCounter(CONFIG_METRICS.VALIDATION_ERROR_COUNT, {
        config_path: configPath,
        error_type: "load_failed",
        ...(configType && { config_type: configType }),
        ...context,
      });
    }
  }

  /**
   * Record cache hit
   * @param configPath Configuration path
   * @param configType Configuration type
   * @param context Optional context
   */
  recordCacheHit(
    configPath: string,
    configType?: string,
    context?: Record<string, string>,
  ): void {
    const labels: Record<string, string> = {
      config_path: configPath,
      ...(configType && { config_type: configType }),
      ...context,
    };

    this.incrementCounter(CONFIG_METRICS.CACHE_HIT_COUNT, labels);
  }

  /**
   * Record cache miss
   * @param configPath Configuration path
   * @param configType Configuration type
   * @param context Optional context
   */
  recordCacheMiss(
    configPath: string,
    configType?: string,
    context?: Record<string, string>,
  ): void {
    const labels: Record<string, string> = {
      config_path: configPath,
      ...(configType && { config_type: configType }),
      ...context,
    };

    this.incrementCounter(CONFIG_METRICS.CACHE_MISS_COUNT, labels);
  }

  /**
   * Record validation error
   * @param configPath Configuration path
   * @param errorType Error type/category
   * @param configType Configuration type
   * @param context Optional context
   */
  recordValidationError(
    configPath: string,
    errorType: string,
    configType?: string,
    context?: Record<string, string>,
  ): void {
    this.incrementCounter(CONFIG_METRICS.VALIDATION_ERROR_COUNT, {
      config_path: configPath,
      error_type: errorType,
      ...(configType && { config_type: configType }),
      ...context,
    });
  }

  /**
   * Get configuration-specific statistics
   * @param configPath Optional configuration path filter
   * @param configType Optional configuration type filter
   * @returns Aggregated statistics
   */
  getConfigStats(configPath?: string, configType?: string): MetricQueryResult {
    const labels: Record<string, string> = {};
    if (configPath) {
      labels['config_path'] = configPath;
    }
    if (configType) {
      labels['config_type'] = configType;
    }

    const filter: MetricFilter = Object.keys(labels).length > 0 ? { labels } : {};

    return this.query(filter);
  }

  /**
   * Get cache hit rate for a configuration
   * @param configPath Configuration path
   * @param configType Configuration type
   * @returns Cache hit rate (0-1) or undefined if no data
   */
  getCacheHitRate(configPath: string, configType?: string): number | undefined {
    const labels: Record<string, string> = { config_path: configPath };
    if (configType) {
      labels['config_type'] = configType;
    }

    const hits = this.query({
      metricName: CONFIG_METRICS.CACHE_HIT_COUNT,
      labels,
    });

    const misses = this.query({
      metricName: CONFIG_METRICS.CACHE_MISS_COUNT,
      labels,
    });

    const hitsAgg = hits.metrics.get(CONFIG_METRICS.CACHE_HIT_COUNT);
    const missesAgg = misses.metrics.get(CONFIG_METRICS.CACHE_MISS_COUNT);

    const totalHits = hitsAgg ? hitsAgg.value : 0;
    const totalMisses = missesAgg ? missesAgg.value : 0;
    const total = totalHits + totalMisses;

    if (total === 0) {
      return undefined;
    }

    return totalHits / total;
  }

  /**
   * Get average load duration for a configuration
   * @param configPath Configuration path
   * @param configType Configuration type
   * @returns Average load duration in milliseconds
   */
  getAverageLoadDuration(configPath: string, configType?: string): number {
    const labels: Record<string, string> = { config_path: configPath };
    if (configType) {
      labels['config_type'] = configType;
    }

    const result = this.query({
      metricName: CONFIG_METRICS.LOAD_DURATION,
      labels,
    });

    const aggregated = result.metrics.get(CONFIG_METRICS.LOAD_DURATION);
    if (!aggregated || aggregated.value === 0) {
      return 0;
    }

    return aggregated.value;
  }

  /**
   * Flush metrics to storage
   * Override to implement custom persistence logic
   */
  async flush(): Promise<void> {
    const flushedCount = this.metricsBuffer.length;

    if (flushedCount > 0) {
      logger.debug("Flushing configuration metrics", { flushedCount });

      // TODO: Implement actual persistence
      this.metricsBuffer = [];
    }
  }

  /**
   * Export as Prometheus format
   */
  toPrometheus(): string[] {
    const result = this.query({});
    const metrics: PrometheusMetric[] = [];
    
    // Extract totals
    let totalAccess = 0;
    let totalCacheHits = 0;
    let totalCacheMisses = 0;
    let totalValidationErrors = 0;
    
    for (const [metricName, aggregated] of result.metrics.entries()) {
      switch (metricName) {
        case CONFIG_METRICS.ACCESS_COUNT:
          totalAccess += aggregated.value;
          break;
        case CONFIG_METRICS.CACHE_HIT_COUNT:
          totalCacheHits += aggregated.value;
          break;
        case CONFIG_METRICS.CACHE_MISS_COUNT:
          totalCacheMisses += aggregated.value;
          break;
        case CONFIG_METRICS.VALIDATION_ERROR_COUNT:
          totalValidationErrors += aggregated.value;
          break;
      }
    }
    
    // Total access counter
    metrics.push({
      name: 'config_access_total',
      type: 'counter',
      help: 'Total configuration accesses',
      samples: [{ value: totalAccess }]
    });
    
    // Cache hits
    metrics.push({
      name: 'config_cache_hit_total',
      type: 'counter',
      help: 'Total cache hits',
      samples: [{ value: totalCacheHits }]
    });
    
    // Cache misses
    metrics.push({
      name: 'config_cache_miss_total',
      type: 'counter',
      help: 'Total cache misses',
      samples: [{ value: totalCacheMisses }]
    });
    
    // Validation errors
    if (totalValidationErrors > 0) {
      metrics.push({
        name: 'config_validation_error_total',
        type: 'counter',
        help: 'Total validation errors',
        samples: [{ value: totalValidationErrors }]
      });
    }
    
    // Format all metrics
    return metrics.flatMap(m => PrometheusFormatter.formatMetric(m));
  }
  
  /**
   * Export as JSON
   */
  toJSON(): Record<string, unknown> {
    const summary = {
      totalAccess: 0,
      totalCacheHits: 0,
      totalCacheMisses: 0,
      totalValidationErrors: 0
    };
    
    const result = this.query({});
    for (const [metricName, aggregated] of result.metrics.entries()) {
      switch (metricName) {
        case CONFIG_METRICS.ACCESS_COUNT:
          summary.totalAccess += aggregated.value;
          break;
        case CONFIG_METRICS.CACHE_HIT_COUNT:
          summary.totalCacheHits += aggregated.value;
          break;
        case CONFIG_METRICS.CACHE_MISS_COUNT:
          summary.totalCacheMisses += aggregated.value;
          break;
        case CONFIG_METRICS.VALIDATION_ERROR_COUNT:
          summary.totalValidationErrors += aggregated.value;
          break;
      }
    }
    
    return {
      type: 'config',
      summary
    };
  }
}
