/**
 * Base Metric Collector Implementation
 * 
 * Provides common functionality for all metric collectors including:
 * - Buffering and batching
 * - Periodic flushing
 * - Query support
 * - Reporting
 */

import { now } from "@wf-agent/common-utils";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import type {
  Metric,
  MetricCollector,
  MetricCollectorConfig,
  MetricFilter,
  MetricQueryResult,
  AggregatedMetric,
  MetricReportCallback,
  MetricReport,
} from "./types.js";

const logger = createContextualLogger({ operation: "BaseMetricCollector" });

/**
 * Base implementation of MetricCollector
 * Provides common functionality that can be extended by specific collectors
 */
export abstract class BaseMetricCollector implements MetricCollector {
  protected metricsBuffer: Metric[] = [];
  protected config: Required<MetricCollectorConfig>;
  protected reportCallbacks: Array<{
    callback: MetricReportCallback;
    interval: number;
    timer?: NodeJS.Timeout;
  }> = [];
  protected isStarted: boolean = false;
  private flushTimer?: NodeJS.Timeout;

  constructor(config?: MetricCollectorConfig) {
    this.config = {
      bufferSize: config?.bufferSize ?? 100,
      flushInterval: config?.flushInterval ?? 5000,
      enablePeriodicReporting: config?.enablePeriodicReporting ?? false,
      reportingInterval: config?.reportingInterval ?? 10000,
      maxAge: config?.maxAge ?? 3600000, // 1 hour default
    };

    // Start periodic flush if enabled
    if (this.config.flushInterval > 0) {
      this.startPeriodicFlush();
    }

    // Start periodic reporting if enabled
    if (this.config.enablePeriodicReporting) {
      this.startPeriodicReporting();
    }
  }

  /**
   * Record a metric
   * @param metric The metric to record
   */
  record(metric: Metric): void {
    // Validate metric
    if (!metric.metricName) {
      logger.warn("Record called with missing metricName", { metric });
      return;
    }

    // Add to buffer
    this.metricsBuffer.push(metric);

    // Check if buffer needs flushing
    if (this.metricsBuffer.length >= this.config.bufferSize) {
      this.flush().catch((error) => {
        logger.error("Failed to flush metrics buffer", { error });
      });
    }
  }

  /**
   * Record a counter increment
   */
  incrementCounter(
    metricName: string,
    labels?: Record<string, string>,
    increment: number = 1,
  ): void {
    const metric = {
      metricName,
      metricType: "counter" as const,
      timestamp: now(),
      labels: labels || {},
      value: increment,
    };
    this.record(metric);
  }

  /**
   * Record a gauge value
   */
  setGauge(metricName: string, value: number, labels?: Record<string, string>): void {
    const metric = {
      metricName,
      metricType: "gauge" as const,
      timestamp: now(),
      labels: labels || {},
      value,
    };
    this.record(metric);
  }

  /**
   * Record a histogram observation
   */
  observeHistogram(
    metricName: string,
    value: number,
    labels?: Record<string, string>,
  ): void {
    // For simplicity, we'll create a basic histogram with standard buckets
    // Subclasses can override this for custom bucket configurations
    const buckets = this.createStandardBuckets(value);
    const metric = {
      metricName,
      metricType: "histogram" as const,
      timestamp: now(),
      labels: labels || {},
      value,
      buckets,
      sum: value,
      count: 1,
    };
    this.record(metric);
  }

  /**
   * Record a summary observation
   */
  observeSummary(
    metricName: string,
    value: number,
    labels?: Record<string, string>,
  ): void {
    const metric = {
      metricName,
      metricType: "summary" as const,
      timestamp: now(),
      labels: labels || {},
      value,
      percentiles: [{ percentile: 1.0, value }],
      sum: value,
      count: 1,
    };
    this.record(metric);
  }

  /**
   * Flush buffered metrics
   * Must be implemented by subclasses to handle persistence
   */
  abstract flush(): Promise<void>;

  /**
   * Query metrics with filters
   * Default implementation performs in-memory filtering
   * Subclasses can override for optimized querying
   */
  query(filter: MetricFilter): MetricQueryResult {
    const startTime = now();

    // Filter metrics
    let filtered = this.metricsBuffer;

    if (filter.metricName) {
      filtered = filtered.filter((m) => m.metricName === filter.metricName);
    }

    if (filter.metricType) {
      filtered = filtered.filter((m) => m.metricType === filter.metricType);
    }

    if (filter.labels) {
      filtered = filtered.filter((m) => {
        return Object.entries(filter.labels!).every(
          ([key, val]) => m.labels[key] === val,
        );
      });
    }

    if (filter.timeRange) {
      filtered = filtered.filter(
        (m) =>
          m.timestamp >= filter.timeRange!.from &&
          m.timestamp <= filter.timeRange!.to,
      );
    }

    // Apply limit
    if (filter.limit) {
      filtered = filtered.slice(0, filter.limit);
    }

    // Aggregate results
    const aggregated = this.aggregateMetrics(filtered);

    const queryTime = now() - startTime;

    return {
      totalCount: filtered.length,
      metrics: aggregated,
      queryTime,
    };
  }

  /**
   * Subscribe to periodic reports
   */
  onReport(
    callback: MetricReportCallback,
    options?: { interval?: number },
  ): () => void {
    const interval = options?.interval ?? this.config.reportingInterval;

    const subscription = {
      callback,
      interval,
    };

    this.reportCallbacks.push(subscription);

    // Return unsubscribe function
    return () => {
      const index = this.reportCallbacks.findIndex(
        (cb) => cb.callback === subscription.callback && cb.interval === subscription.interval,
      );
      if (index !== -1) {
        const sub = this.reportCallbacks[index];
        if (sub?.timer) {
          clearInterval(sub.timer);
        }
        this.reportCallbacks.splice(index, 1);
      }
    };
  }

  /**
   * Clear all collected metrics
   */
  clear(): void {
    this.metricsBuffer = [];
    this.stopPeriodicFlush();
    this.stopPeriodicReporting();
    logger.info("Metrics cleared");
  }

  /**
   * Dispose the collector and release resources
   */
  dispose(): void {
    this.stopPeriodicFlush();
    this.stopPeriodicReporting();
    this.clear();
    logger.info("Metric collector disposed");
  }

  // =============================================================================
  // Protected Helper Methods
  // =============================================================================

  /**
   * Aggregate metrics by name and labels
   */
  protected aggregateMetrics(metrics: Metric[]): Map<string, AggregatedMetric> {
    const aggregated = new Map<string, AggregatedMetric>();

    for (const metric of metrics) {
      if (!aggregated.has(metric.metricName)) {
        aggregated.set(metric.metricName, {
          metricName: metric.metricName,
          metricType: metric.metricType,
          value: 0,
          byLabel: new Map(),
          timeSeries: [],
        });
      }

      const agg = aggregated.get(metric.metricName)!;

      // Update aggregated value based on metric type
      switch (metric.metricType) {
        case "counter":
          agg.value += typeof metric.value === "number" ? metric.value : 0;
          break;
        case "gauge":
          agg.value = typeof metric.value === "number" ? metric.value : 0;
          break;
        case "histogram":
        case "summary":
          agg.value = typeof metric.value === "number" ? metric.value : 0;
          break;
      }

      // Add to time series
      if (typeof metric.value === "number") {
        agg.timeSeries!.push({
          timestamp: metric.timestamp,
          value: metric.value,
        });
      }

      // Group by labels
      const labelKey = JSON.stringify(metric.labels);
      if (!agg.byLabel.has(labelKey)) {
        agg.byLabel.set(labelKey, {
          metricName: metric.metricName,
          metricType: metric.metricType,
          value: 0,
          byLabel: new Map(),
        });
      }

      const labelAgg = agg.byLabel.get(labelKey)!;
      if (typeof metric.value === "number") {
        labelAgg.value += metric.value;
      }
    }

    return aggregated;
  }

  /**
   * Create standard histogram buckets
   */
  protected createStandardBuckets(value: number): Array<{
    upperBound: number;
    count: number;
  }> {
    const standardBounds = [0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1, 2.5, 5, 10];
    const buckets: Array<{ upperBound: number; count: number }> = [];

    for (const bound of standardBounds) {
      buckets.push({
        upperBound: bound,
        count: value <= bound ? 1 : 0,
      });
    }

    // Add +Inf bucket
    buckets.push({
      upperBound: Infinity,
      count: 1,
    });

    return buckets;
  }

  /**
   * Generate a metric report
   */
  protected generateReport(): MetricReport {
    const byType: Record<string, number> = {
      counter: 0,
      gauge: 0,
      histogram: 0,
      summary: 0,
    };

    const byCategory: Record<string, number> = {};

    for (const metric of this.metricsBuffer) {
      byType[metric.metricType] = (byType[metric.metricType] || 0) + 1;

      // Extract category from metric name (first part before '.')
      const category = metric.metricName.split(".")[0] || "unknown";
      byCategory[category] = (byCategory[category] ?? 0) + 1;
    }

    // Get top metrics by value
    const sortedMetrics = [...this.metricsBuffer]
      .filter((m) => typeof m.value === "number")
      .sort((a, b) => (b.value as number) - (a.value as number))
      .slice(0, 10)
      .map((m) => ({
        metricName: m.metricName,
        value: m.value as number,
        labels: m.labels,
      }));

    return {
      timestamp: now(),
      summary: {
        totalMetrics: this.metricsBuffer.length,
        byType: byType as any,
        byCategory,
      },
      topMetrics: sortedMetrics,
    };
  }

  /**
   * Start periodic flushing
   */
  private startPeriodicFlush(): void {
    if (this.flushTimer) {
      return;
    }

    this.flushTimer = setInterval(() => {
      this.flush().catch((error) => {
        logger.error("Periodic flush failed", { error });
      });
    }, this.config.flushInterval);

    logger.debug("Periodic flush started", { interval: this.config.flushInterval });
  }

  /**
   * Stop periodic flushing
   */
  private stopPeriodicFlush(): void {
    if (this.flushTimer) {
      clearInterval(this.flushTimer);
      this.flushTimer = undefined;
      logger.debug("Periodic flush stopped");
    }
  }

  /**
   * Start periodic reporting
   */
  private startPeriodicReporting(): void {
    if (this.isStarted) {
      return;
    }

    this.isStarted = true;

    for (const subscription of this.reportCallbacks) {
      subscription.timer = setInterval(async () => {
        try {
          const report = this.generateReport();
          await subscription.callback(report);
        } catch (error) {
          logger.error("Error in report callback", { error });
        }
      }, subscription.interval);
    }

    logger.info("Periodic reporting started", {
      subscriptionCount: this.reportCallbacks.length,
    });
  }

  /**
   * Stop periodic reporting
   */
  private stopPeriodicReporting(): void {
    if (!this.isStarted) {
      return;
    }

    for (const subscription of this.reportCallbacks) {
      if (subscription.timer) {
        clearInterval(subscription.timer);
        subscription.timer = undefined;
      }
    }

    this.isStarted = false;
    logger.info("Periodic reporting stopped");
  }
}
