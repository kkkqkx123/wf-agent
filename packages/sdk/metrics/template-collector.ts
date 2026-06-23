/**
 * Template Usage Metrics Collector
 *
 * Collects and aggregates metrics for template rendering and usage including:
 * - Template invocation counts
 * - Render duration distributions
 * - Cache hit/miss rates
 * - Error tracking
 */

import { BaseMetricCollector } from "./base-collector.js";
import type { MetricCollectorConfig, MetricFilter, MetricQueryResult } from "./types.js";
import { TEMPLATE_METRICS } from "./constants.js";
import { PrometheusFormatter, type PrometheusMetric } from "./utils/prometheus-formatter.js";
import { createContextualLogger } from "@sdk/utils/contextual-logger.js";

const logger = createContextualLogger({ operation: "TemplateMetricsCollector" });

/**
 * Template-specific metric collector
 * Extends BaseMetricCollector with template-specific convenience methods
 */
export class TemplateMetricsCollector extends BaseMetricCollector {
  constructor(config?: MetricCollectorConfig) {
    super(config);
  }

  /**
   * Record template usage
   * @param templateId Template ID or name
   * @param context Optional context (e.g., workflow_id, agent_loop_id)
   */
  recordUsage(templateId: string, context?: Record<string, string>): void {
    const labels: Record<string, string> = {
      template_id: templateId,
      ...context,
    };

    this.incrementCounter(TEMPLATE_METRICS.INSTANTIATION_COUNT, labels);
  }

  /**
   * Record template render completion
   * @param templateId Template ID or name
   * @param duration Render duration in milliseconds
   * @param success Whether render was successful
   * @param context Optional context
   */
  recordRenderComplete(
    templateId: string,
    duration: number,
    success: boolean,
    context?: Record<string, string>,
  ): void {
    const labels: Record<string, string> = {
      template_id: templateId,
      success: success.toString(),
      ...context,
    };

    // Record render duration distribution
    this.observeHistogram(TEMPLATE_METRICS.RENDER_DURATION, duration, labels);

    // Record errors if failed
    if (!success) {
      this.incrementCounter(TEMPLATE_METRICS.ERROR_COUNT, {
        template_id: templateId,
        error_type: "render_failed",
        ...context,
      });
    }
  }

  /**
   * Record cache hit
   * @param templateId Template ID or name
   * @param context Optional context
   */
  recordCacheHit(templateId: string, context?: Record<string, string>): void {
    this.incrementCounter(TEMPLATE_METRICS.CACHE_HIT_COUNT, {
      template_id: templateId,
      ...context,
    });
  }

  /**
   * Record cache miss
   * @param templateId Template ID or name
   * @param context Optional context
   */
  recordCacheMiss(templateId: string, context?: Record<string, string>): void {
    this.incrementCounter(TEMPLATE_METRICS.CACHE_MISS_COUNT, {
      template_id: templateId,
      ...context,
    });
  }

  /**
   * Record template error
   * @param templateId Template ID or name
   * @param errorType Error type/category
   * @param context Optional context
   */
  recordError(templateId: string, errorType: string, context?: Record<string, string>): void {
    this.incrementCounter(TEMPLATE_METRICS.ERROR_COUNT, {
      template_id: templateId,
      error_type: errorType,
      ...context,
    });
  }

  /**
   * Get template-specific statistics
   * @param templateId Optional template ID filter
   * @returns Aggregated statistics
   */
  getTemplateStats(templateId?: string): MetricQueryResult {
    const filter: MetricFilter = templateId
      ? {
          labels: { template_id: templateId },
        }
      : {};

    return this.query(filter);
  }

  /**
   * Get cache hit rate for a template
   * @param templateId Template ID
   * @returns Cache hit rate (0-1) or undefined if no data
   */
  getCacheHitRate(templateId: string): number | undefined {
    const hits = this.query({
      metricName: TEMPLATE_METRICS.CACHE_HIT_COUNT,
      labels: { template_id: templateId },
    });

    const misses = this.query({
      metricName: TEMPLATE_METRICS.CACHE_MISS_COUNT,
      labels: { template_id: templateId },
    });

    const hitsAgg = hits.metrics.get(TEMPLATE_METRICS.CACHE_HIT_COUNT);
    const missesAgg = misses.metrics.get(TEMPLATE_METRICS.CACHE_MISS_COUNT);

    const totalHits = hitsAgg ? hitsAgg.value : 0;
    const totalMisses = missesAgg ? missesAgg.value : 0;
    const total = totalHits + totalMisses;

    if (total === 0) {
      return undefined;
    }

    return totalHits / total;
  }

  /**
   * Get average render duration for a template
   * @param templateId Template ID
   * @returns Average render duration in milliseconds
   */
  getAverageRenderDuration(templateId: string): number {
    const result = this.query({
      metricName: TEMPLATE_METRICS.RENDER_DURATION,
      labels: { template_id: templateId },
    });

    const aggregated = result.metrics.get(TEMPLATE_METRICS.RENDER_DURATION);
    if (!aggregated || aggregated.value === 0) {
      return 0;
    }

    // For histogram, value represents the last observed value
    // To get true average, we'd need to track sum/count separately
    // For now, return the aggregated value as an approximation
    return aggregated.value;
  }

  /**
   * Export as Prometheus format
   */
  toPrometheus(): string[] {
    const result = this.query({});
    const metrics: PrometheusMetric[] = [];

    // Extract totals
    let totalUsage = 0;
    let totalCacheHits = 0;
    let totalCacheMisses = 0;
    let totalErrors = 0;

    for (const [metricName, aggregated] of result.metrics.entries()) {
      switch (metricName) {
        case TEMPLATE_METRICS.INSTANTIATION_COUNT:
          totalUsage += aggregated.value;
          break;
        case TEMPLATE_METRICS.CACHE_HIT_COUNT:
          totalCacheHits += aggregated.value;
          break;
        case TEMPLATE_METRICS.CACHE_MISS_COUNT:
          totalCacheMisses += aggregated.value;
          break;
        case TEMPLATE_METRICS.ERROR_COUNT:
          totalErrors += aggregated.value;
          break;
      }
    }

    // Total usage counter
    metrics.push({
      name: "template_usage_total",
      type: "counter",
      help: "Total template usages",
      samples: [{ value: totalUsage }],
    });

    // Cache hits
    metrics.push({
      name: "template_cache_hit_total",
      type: "counter",
      help: "Total cache hits",
      samples: [{ value: totalCacheHits }],
    });

    // Cache misses
    metrics.push({
      name: "template_cache_miss_total",
      type: "counter",
      help: "Total cache misses",
      samples: [{ value: totalCacheMisses }],
    });

    // Errors
    if (totalErrors > 0) {
      metrics.push({
        name: "template_error_total",
        type: "counter",
        help: "Total template errors",
        samples: [{ value: totalErrors }],
      });
    }

    // By template breakdown
    for (const [metricName, aggregated] of result.metrics.entries()) {
      if (aggregated.byLabel && Object.keys(aggregated.byLabel).length > 0) {
        for (const [labelKey, labelAgg] of Object.entries(aggregated.byLabel)) {
          try {
            const labels = JSON.parse(labelKey);
            const templateId = labels.template_id;
            if (!templateId) continue;

            metrics.push({
              name: `${metricName}_by_template`,
              type: "counter",
              help: `Template metric by template ID`,
              samples: [{ labels: { template_id: templateId }, value: labelAgg.value }],
            });
          } catch (error) {
            logger.warn("Failed to parse label key", { labelKey, error });
          }
        }
      }
    }

    // Format all metrics
    return metrics.flatMap(m => PrometheusFormatter.formatMetric(m));
  }

  /**
   * Export as JSON
   */
  toJSON(): Record<string, unknown> {
    const result = this.query({});
    const templatesData: Record<string, unknown> = {};

    // Aggregate by template_id
    for (const [metricName, aggregated] of result.metrics.entries()) {
      if (aggregated.byLabel) {
        for (const [labelKey, labelAgg] of Object.entries(aggregated.byLabel)) {
          try {
            const labels = JSON.parse(labelKey);
            const templateId = labels.template_id;
            if (!templateId) continue;

            if (!templatesData[templateId]) {
              templatesData[templateId] = {};
            }

            const templateData = templatesData[templateId] as Record<string, number>;
            templateData[metricName] = labelAgg.value;
          } catch (error) {
            logger.warn("Failed to parse label key", { labelKey, error });
          }
        }
      }
    }

    return {
      type: "template",
      templates: templatesData,
    };
  }
}
